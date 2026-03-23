use iced::widget::{button, container, row, text, Space};
use iced::{Color, Element};

use crate::message::Message;
use crate::node::{ConnectionStatus, NodeId};
use crate::theme::colors;

/// A colored dot + label indicating connection state.
/// For failed connections, pass the node_id to enable a retry button.
pub fn status_badge_with_retry(
    status: &ConnectionStatus,
    node_id: NodeId,
) -> Element<'_, Message> {
    let (color, label) = match status {
        ConnectionStatus::Connecting => (colors::LAVA, "Connecting..."),
        ConnectionStatus::FetchingInfo => (colors::LAVA, "Fetching info..."),
        ConnectionStatus::Streaming => (colors::GEOTHERMAL, "Connected"),
        ConnectionStatus::Reconnecting(_) => (colors::EMBER, "Reconnecting..."),
        ConnectionStatus::Failed(_) => (colors::MAGMA, "Failed"),
    };

    let dot = container(Space::new())
        .width(8)
        .height(8)
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(color.into()),
            border: iced::Border {
                radius: 4.0.into(),
                ..Default::default()
            },
            ..Default::default()
        });

    let label_text = text(label)
        .size(11)
        .color(color);

    let mut badge_row = row![dot, label_text]
        .spacing(6)
        .align_y(iced::Alignment::Center);

    if matches!(status, ConnectionStatus::Failed(_)) {
        let retry_btn = button(
            text("Retry").size(10).color(colors::EMBER),
        )
        .on_press(Message::RetryConnection(node_id))
        .padding([2, 8])
        .style(|_theme: &iced::Theme, status| {
            let border_color = match status {
                button::Status::Hovered => colors::EMBER,
                _ => colors::SCORIA,
            };
            button::Style {
                background: None,
                border: iced::Border {
                    color: border_color,
                    width: 1.0,
                    radius: 4.0.into(),
                },
                text_color: colors::EMBER,
                ..Default::default()
            }
        });
        badge_row = badge_row.push(retry_btn);
    }

    badge_row.into()
}


/// Throttle status badge — shows active throttle state.
pub fn throttle_badge<'a>(active: bool, reason: &str) -> Element<'a, Message> {
    if !active {
        return Space::new().into();
    }

    let color = match reason {
        "thermal" => colors::ERUPTION,
        "power" => colors::COPPER,
        _ => colors::EMBER,
    };

    let label = match reason {
        "thermal" => "THERMAL THROTTLE",
        "power" => "POWER THROTTLE",
        _ => "THROTTLING",
    };

    container(
        text(label)
            .size(10)
            .color(Color::WHITE),
    )
    .padding([2, 8])
    .style(move |_theme: &iced::Theme| container::Style {
        background: Some(color.into()),
        border: iced::Border {
            radius: 4.0.into(),
            ..Default::default()
        },
        ..Default::default()
    })
    .into()
}
