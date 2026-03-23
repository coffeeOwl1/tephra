use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::Json;
use axum::routing::get;
use axum::Router;
use serde_json::{json, Value};
use tokio::sync::{broadcast, RwLock};
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

use crate::models::{HistoryResponse, Snapshot, SystemInfo, ThrottleEvent, WorkloadEvent};
use crate::monitor::{self, SystemState};
use crate::openapi;

pub struct AppState {
    pub system_state: RwLock<SystemState>,
    pub sse_tx: broadcast::Sender<SseMessage>,
    pub interval_ms: u64,
}

#[derive(Clone)]
pub enum SseMessage {
    Snapshot(Snapshot),
    Throttle(ThrottleEvent),
    WorkloadStart { id: u32, start_time: String },
    WorkloadEnd(WorkloadEvent),
}

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/api/v1/system", get(system_info))
        .route("/api/v1/snapshot", get(snapshot))
        .route("/api/v1/history", get(history))
        .route("/api/v1/events", get(events))
        .route("/api/v1/openapi.json", get(openapi_spec))
        .with_state(state)
}

async fn health() -> Json<Value> {
    Json(json!({"status": "ok"}))
}

async fn openapi_spec() -> Json<Value> {
    let spec: Value = serde_json::from_str(openapi::OPENAPI_SPEC)
        .expect("embedded OpenAPI spec is invalid JSON");
    Json(spec)
}

async fn system_info(State(state): State<Arc<AppState>>) -> Json<SystemInfo> {
    let s = state.system_state.read().await;
    Json(SystemInfo {
        hostname: monitor::get_hostname(),
        cpu_model: monitor::get_cpu_model(),
        core_count: s.cores.len(),
        max_freq_mhz: s.max_freq_mhz,
        scaling_driver: monitor::get_scaling_driver(),
        governor: monitor::get_governor(),
        ram_gb: monitor::get_total_ram_gb(),
        agent_version: env!("CARGO_PKG_VERSION"),
    })
}

async fn snapshot(State(state): State<Arc<AppState>>) -> Json<Snapshot> {
    let s = state.system_state.read().await;
    Json(Snapshot::from_state(&s))
}

async fn history(State(state): State<Arc<AppState>>) -> Json<HistoryResponse> {
    let s = state.system_state.read().await;
    Json(HistoryResponse::from_state(&s, state.interval_ms))
}

async fn events(
    State(state): State<Arc<AppState>>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    let rx = state.sse_tx.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|result| {
        match result {
            Ok(msg) => {
                let event = match msg {
                    SseMessage::Snapshot(snap) => Event::default()
                        .event("snapshot")
                        .json_data(&snap)
                        .ok(),
                    SseMessage::Throttle(t) => Event::default()
                        .event("throttle")
                        .json_data(&t)
                        .ok(),
                    SseMessage::WorkloadStart { id, start_time } => Event::default()
                        .event("workload_start")
                        .json_data(&serde_json::json!({"id": id, "start_time": start_time}))
                        .ok(),
                    SseMessage::WorkloadEnd(w) => Event::default()
                        .event("workload_end")
                        .json_data(&w)
                        .ok(),
                };
                event.map(Ok)
            }
            Err(_) => None, // Lagged receiver, skip
        }
    });

    Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
}

/// Sampling loop: reads sensors, broadcasts SSE events.
pub async fn sampling_loop(state: Arc<AppState>) {
    let interval = Duration::from_millis(state.interval_ms);
    let mut prev_throttle = false;
    let mut prev_workload_count = 0usize;
    let mut prev_active_workload_id: Option<u32> = None;

    loop {
        tokio::time::sleep(interval).await;

        let snapshot = {
            let mut s = state.system_state.write().await;
            s.sample();

            let snap = Snapshot::from_state(&s);

            // Check for throttle events (rising edge)
            if s.throttle_active && !prev_throttle {
                let _ = state.sse_tx.send(SseMessage::Throttle(ThrottleEvent {
                    reason: s.throttle_reason.as_str(),
                    temp_c: s.temp_c,
                    ppt_watts: (s.ppt_watts * 10.0).round() / 10.0,
                }));
            }
            prev_throttle = s.throttle_active;

            // Check for new workload start
            if let Some(ref wl) = s.active_workload {
                if prev_active_workload_id != Some(wl.id) {
                    let _ = state.sse_tx.send(SseMessage::WorkloadStart {
                        id: wl.id,
                        start_time: format!(
                            "{:02}:{:02}:{:02}",
                            wl.start_wall.0, wl.start_wall.1, wl.start_wall.2
                        ),
                    });
                    prev_active_workload_id = Some(wl.id);
                }
            } else {
                prev_active_workload_id = None;
            }

            // Check for completed workloads
            if s.completed_workloads.len() > prev_workload_count {
                for seg in &s.completed_workloads[prev_workload_count..] {
                    let _ = state
                        .sse_tx
                        .send(SseMessage::WorkloadEnd(WorkloadEvent::from_segment(seg)));
                }
            }
            prev_workload_count = s.completed_workloads.len();

            snap
        };

        let _ = state.sse_tx.send(SseMessage::Snapshot(snapshot));
    }
}
