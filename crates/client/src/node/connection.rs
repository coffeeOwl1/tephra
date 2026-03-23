use std::net::SocketAddr;

use futures::stream::BoxStream;
use futures::StreamExt;
use iced::futures::SinkExt;
use iced::Subscription;
use reqwest_eventsource::{Event as SseEvent, EventSource};

use crate::message::{Message, NodeMessage};
use crate::net::api_types::{
    HistoryResponse, Snapshot, SystemInfo, ThrottleEvent, WorkloadEndEvent, WorkloadStartEvent,
};
use crate::node::NodeId;

const MAX_BACKOFF_SECS: u64 = 30;
const MAX_RETRIES: u32 = 10;

/// Creates an iced Subscription that manages the full lifecycle for one node,
/// with automatic reconnection on disconnect.
pub fn node_subscription(
    id: NodeId,
    addr: SocketAddr,
    client: reqwest::Client,
) -> Subscription<Message> {
    let base = format!("http://{addr}");

    iced::advanced::subscription::from_recipe(NodeRecipe {
        id,
        addr,
        base,
        client,
    })
}

struct NodeRecipe {
    id: NodeId,
    addr: SocketAddr,
    base: String,
    client: reqwest::Client,
}

impl iced::advanced::subscription::Recipe for NodeRecipe {
    type Output = Message;

    fn hash(&self, state: &mut iced::advanced::subscription::Hasher) {
        use std::hash::Hash;
        std::any::TypeId::of::<Self>().hash(state);
        self.addr.hash(state);
        self.id.0.hash(state);
    }

    fn stream(
        self: Box<Self>,
        _input: iced::advanced::subscription::EventStream,
    ) -> BoxStream<'static, Message> {
        let NodeRecipe {
            id, base, client, addr,
        } = *self;

        let stream = iced::stream::channel(
            1,
            move |mut output: iced::futures::channel::mpsc::Sender<Message>| async move {
                macro_rules! send {
                    ($msg:expr) => {
                        if output.send($msg).await.is_err() {
                            return; // Receiver dropped — node was removed
                        }
                    };
                }

                let mut retry_count: u32 = 0;

                loop {
                    // -- Connect phase --
                    if retry_count > 0 {
                        if retry_count >= MAX_RETRIES {
                            send!(Message::Node(
                                id,
                                NodeMessage::ConnectionFailed(format!(
                                    "Failed after {MAX_RETRIES} attempts"
                                ))
                            ));
                            return;
                        }
                        let backoff = (1u64 << retry_count.min(5)).min(MAX_BACKOFF_SECS);
                        send!(Message::Node(
                            id,
                            NodeMessage::Disconnected(format!("Reconnecting in {backoff}s..."))
                        ));
                        tokio::time::sleep(std::time::Duration::from_secs(backoff)).await;
                    }

                    // 1. Fetch system info
                    let info = match fetch_json::<SystemInfo>(
                        &client,
                        &format!("{base}/api/v1/system"),
                    )
                    .await
                    {
                        Ok(info) => info,
                        Err(e) => {
                            tracing::warn!("Node {addr} connect failed: {e}");
                            retry_count += 1;
                            continue;
                        }
                    };
                    send!(Message::Node(id, NodeMessage::SystemInfoFetched(info)));

                    // 2. Fetch history
                    match fetch_json::<HistoryResponse>(
                        &client,
                        &format!("{base}/api/v1/history"),
                    )
                    .await
                    {
                        Ok(hist) => {
                            send!(Message::Node(id, NodeMessage::HistoryFetched(hist)));
                        }
                        Err(e) => {
                            tracing::warn!("Failed to fetch history for {addr}: {e}");
                        }
                    }

                    // 3. Open SSE stream
                    let request = client.get(format!("{base}/api/v1/events"));
                    let mut es = match EventSource::new(request) {
                        Ok(es) => es,
                        Err(e) => {
                            tracing::warn!("Node {addr} SSE failed: {e}");
                            retry_count += 1;
                            continue;
                        }
                    };

                    send!(Message::Node(id, NodeMessage::Connected));
                    retry_count = 0; // Reset on successful connection

                    // 4. Process SSE events until disconnect
                    let mut disconnected = false;
                    while let Some(event) = es.next().await {
                        match event {
                            Ok(SseEvent::Open) => {}
                            Ok(SseEvent::Message(msg)) => {
                                let node_msg = match msg.event.as_str() {
                                    "snapshot" => serde_json::from_str::<Snapshot>(&msg.data)
                                        .ok()
                                        .map(NodeMessage::SnapshotReceived),
                                    "throttle" => {
                                        serde_json::from_str::<ThrottleEvent>(&msg.data)
                                            .ok()
                                            .map(NodeMessage::ThrottleEvent)
                                    }
                                    "workload_start" => {
                                        serde_json::from_str::<WorkloadStartEvent>(&msg.data)
                                            .ok()
                                            .map(NodeMessage::WorkloadStart)
                                    }
                                    "workload_end" => {
                                        serde_json::from_str::<WorkloadEndEvent>(&msg.data)
                                            .ok()
                                            .map(NodeMessage::WorkloadEnd)
                                    }
                                    _ => None,
                                };

                                if let Some(nm) = node_msg {
                                    send!(Message::Node(id, nm));
                                }
                            }
                            Err(e) => {
                                tracing::warn!("Node {addr} SSE error: {e}");
                                disconnected = true;
                                break;
                            }
                        }
                    }

                    if disconnected {
                        retry_count += 1;
                        // Loop will continue with backoff
                    } else {
                        // Stream ended cleanly (server shutdown?)
                        send!(Message::Node(
                            id,
                            NodeMessage::Disconnected("Server closed connection".into())
                        ));
                        retry_count += 1;
                    }
                }
            },
        );

        Box::pin(stream)
    }
}

const REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

async fn fetch_json<T: serde::de::DeserializeOwned>(
    client: &reqwest::Client,
    url: &str,
) -> Result<T, String> {
    let resp = client
        .get(url)
        .timeout(REQUEST_TIMEOUT)
        .send()
        .await
        .map_err(|e| format!("Request failed: {e}"))?;
    resp.json::<T>()
        .await
        .map_err(|e| format!("JSON parse failed: {e}"))
}
