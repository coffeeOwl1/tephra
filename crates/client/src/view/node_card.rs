use iced::widget::{button, column, container, row, text, Space};
use iced::{Element, Length};

use crate::message::Message;
use crate::node::NodeState;
use crate::theme::colors;
use crate::view::components::metric_pill::metric_compact;
use crate::view::components::status_badge::{status_badge_with_retry, throttle_badge};

/// Dashboard card for a single node.
/// Shows hostname, CPU model, key metrics, and connection status.
pub fn node_card(node: &NodeState) -> Element<'_, Message> {
    let id = node.id;

    // Header: hostname + status dot
    let hostname = text(node.display_name())
        .size(14)
        .color(colors::PUMICE);

    let status = status_badge_with_retry(&node.status, node.id);

    let unread = node.unread_event_count();
    let mut header = row![hostname].spacing(6).align_y(iced::Alignment::Center);
    if unread > 0 {
        header = header.push(
            container(
                text(format!("{unread}")).size(9).color(iced::Color::WHITE),
            )
            .padding([1, 5])
            .style(|_: &iced::Theme| container::Style {
                background: Some(colors::EMBER.into()),
                border: iced::Border {
                    radius: 8.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }),
        );
    }
    header = header.push(Space::new().width(Length::Fill)).push(status);

    let mut col = column![header].spacing(4);

    // CPU model subtitle (truncated)
    if let Some(info) = &node.system_info {
        let model = if info.cpu_model.len() > 30 {
            format!("{}…", &info.cpu_model[..29])
        } else {
            info.cpu_model.clone()
        };
        col = col.push(
            text(format!("{} | {}c", model, info.core_count))
                .size(10)
                .color(colors::TEPHRA),
        );
    }

    // Metrics
    if let Some(snap) = &node.snapshot {
        use crate::node::history::compute_trend;

        let temp_color = colors::temp_color(snap.temp_c);
        let power_color = colors::power_color(snap.ppt_watts);
        let util_color = colors::util_color(snap.avg_util_pct);

        let tt = compute_trend(&node.history.temp_c);
        let pt = compute_trend(&node.history.ppt_watts);
        let ft = compute_trend(&node.history.avg_freq_mhz);
        let ut = compute_trend(&node.history.avg_util_pct);

        let temp = metric_compact("Temp", &format!("{}°", snap.temp_c), "C", temp_color, Some(tt));
        let power = metric_compact("Power", &format!("{:.1}", snap.ppt_watts), "W", power_color, Some(pt));
        let freq = metric_compact("Freq", &format!("{}", snap.avg_freq_mhz), "MHz", colors::MINERAL, Some(ft));
        let util = metric_compact("Util", &format!("{:.1}", snap.avg_util_pct), "%", util_color, Some(ut));

        let metrics_top = row![temp, power].spacing(12);
        let metrics_bot = row![freq, util].spacing(12);

        col = col.push(metrics_top).push(metrics_bot);

        // Fan row (or spacer for consistent card height)
        if snap.fan_detected {
            let fan_color = if snap.fan_rpm == 0 {
                colors::MAGMA
            } else {
                colors::TEPHRA
            };
            col = col.push(metric_compact(
                "Fan",
                &format!("{}", snap.fan_rpm),
                "RPM",
                fan_color,
                None,
            ));
        } else {
            // Spacer matching fan row height for consistent card sizing
            col = col.push(Space::new().height(20));
        }

        // Throttle badge (always reserve space for consistent card height)
        if snap.throttle_active {
            col = col.push(throttle_badge(true, &snap.throttle_reason));
        } else {
            col = col.push(Space::new().height(18));
        }

        // Temperature sparkline
        col = col.push(crate::view::charts::sparkline::sparkline(
            &node.history.temp_c,
            colors::temp_color(snap.temp_c),
            &node.caches.sparkline,
        ));
    }

    // Throttle flash border
    let throttle_flash = node.is_throttle_flashing();
    let card_border_color = if throttle_flash { colors::LAVA } else { colors::SCORIA };
    let card_border_width = if throttle_flash { 2.0 } else { 1.0 };

    // Make the entire card clickable — fixed width for grid layout
    let card_content = container(col.padding(10))
        .width(300)
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(colors::BASALT.into()),
            border: iced::Border {
                color: card_border_color,
                width: card_border_width,
                radius: 8.0.into(),
            },
            ..Default::default()
        });

    button(card_content)
        .on_press(Message::NavigateDetail(id))
        .padding(0)
        .style(|_theme: &iced::Theme, status| {
            let bg = match status {
                button::Status::Hovered => Some(colors::SCORIA.into()),
                _ => None,
            };
            button::Style {
                background: bg,
                border: iced::Border {
                    radius: 8.0.into(),
                    ..Default::default()
                },
                text_color: colors::PUMICE,
                ..Default::default()
            }
        })
        .width(300)
        .into()
}
