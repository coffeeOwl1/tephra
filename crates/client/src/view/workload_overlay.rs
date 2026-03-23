use iced::widget::{button, column, container, row, text, Space};
use iced::{Element, Length};

use crate::message::Message;
use crate::node::NodeState;
use crate::theme::colors;

/// Centered modal popup showing detailed workload stats.
pub fn workload_overlay<'a>(node: &'a NodeState, idx: usize) -> Element<'a, Message> {
    let wl = match node.completed_workloads.get(idx) {
        Some(wl) => wl,
        None => return Space::new().into(),
    };

    let total = node.completed_workloads.len();

    // Title
    let title = text(format!(
        "Workload #{} — {} → {} — {:.0}s",
        wl.id, wl.start_time, wl.end_time, wl.duration_secs
    ))
    .size(16)
    .color(colors::PUMICE);

    // Navigation
    let prev_btn = button(text("←").size(14).color(colors::PUMICE))
        .on_press(Message::WorkloadOverlayPrev)
        .padding([4, 12])
        .style(|_: &iced::Theme, _| button::Style {
            background: None,
            text_color: colors::PUMICE,
            ..Default::default()
        });

    let next_btn = button(text("→").size(14).color(colors::PUMICE))
        .on_press(Message::WorkloadOverlayNext)
        .padding([4, 12])
        .style(|_: &iced::Theme, _| button::Style {
            background: None,
            text_color: colors::PUMICE,
            ..Default::default()
        });

    let close_btn = button(text("✕").size(14).color(colors::TEPHRA))
        .on_press(Message::CloseWorkloadOverlay)
        .padding([4, 12])
        .style(|_: &iced::Theme, status| {
            let color = match status {
                button::Status::Hovered => colors::MAGMA,
                _ => colors::TEPHRA,
            };
            button::Style {
                background: None,
                text_color: color,
                ..Default::default()
            }
        });

    let nav = row![
        prev_btn,
        text(format!("{}/{}", idx + 1, total)).size(12).color(colors::TEPHRA),
        next_btn,
        Space::new().width(Length::Fill),
        text("w/Esc close").size(10).color(colors::TEPHRA),
        close_btn,
    ]
    .align_y(iced::Alignment::Center)
    .spacing(8);

    // Stats
    let stat = |label: &str, value: String, color: iced::Color| -> Element<'a, Message> {
        row![
            text(format!("{label}:")).size(12).color(colors::TEPHRA),
            Space::new().width(8),
            text(value).size(12).color(color),
        ]
        .align_y(iced::Alignment::Center)
        .into()
    };

    let mut stats = column![].spacing(4);
    stats = stats.push(stat("Avg temperature", format!("{:.1}°C", wl.avg_temp), colors::PUMICE));
    stats = stats.push(stat("Peak temperature", format!("{}°C", wl.peak_temp), colors::temp_color(wl.peak_temp)));
    stats = stats.push(stat("Avg power", format!("{:.1} W", wl.avg_ppt), colors::PUMICE));
    stats = stats.push(stat("Peak power", format!("{:.1} W", wl.peak_ppt), colors::power_color(wl.peak_ppt)));
    stats = stats.push(stat("Energy", format!("{:.3} Wh", wl.energy_wh), colors::PUMICE));
    stats = stats.push(stat("Avg frequency", format!("{} MHz", wl.avg_freq), colors::PUMICE));
    stats = stats.push(stat("Avg utilization", format!("{:.1}%", wl.avg_util), colors::PUMICE));

    if wl.thermal_events > 0 || wl.power_events > 0 {
        stats = stats.push(stat(
            "Throttle events",
            format!("{} thermal, {} power", wl.thermal_events, wl.power_events),
            colors::MAGMA,
        ));
    } else {
        stats = stats.push(stat("Throttle", "No throttle events".to_string(), colors::GEOTHERMAL));
    }

    let dialog = container(
        column![title, nav, Space::new().height(8), stats]
            .spacing(8)
            .padding(24)
            .width(500),
    )
    .style(|_: &iced::Theme| container::Style {
        background: Some(colors::BASALT.into()),
        border: iced::Border {
            color: colors::SCORIA,
            width: 1.0,
            radius: 12.0.into(),
        },
        ..Default::default()
    });

    // Center with dark backdrop
    container(
        container(dialog)
            .width(Length::Shrink)
            .center_x(Length::Fill),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .center_y(Length::Fill)
    .style(|_: &iced::Theme| container::Style {
        background: Some(iced::Color::from_rgba(0.0, 0.0, 0.0, 0.6).into()),
        ..Default::default()
    })
    .into()
}
