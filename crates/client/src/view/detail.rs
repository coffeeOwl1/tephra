use iced::widget::{button, column, container, row, scrollable, text, Space};
use iced::{Element, Length};

use crate::app::App;
use crate::message::{DetailTab, Message};
use crate::node::NodeState;
use crate::theme::colors;
use crate::view::components::metric_pill::{metric_pill, metric_pill_styled};
use crate::view::components::status_badge::{status_badge_with_retry, throttle_badge};
use crate::view::dashboard::global_header;

/// The detail view for a single node.
pub fn view<'a>(app: &'a App, node: &'a NodeState, tab: DetailTab) -> Element<'a, Message> {
    let nav_bar = build_nav_bar(app, node, tab);
    let metrics_strip = build_metrics_strip(node);
    let tab_content = build_tab_content(app, node, tab);

    // Paused indicator
    let mut header_items = column![].spacing(4);
    if app.paused {
        header_items = header_items.push(
            container(
                text("PAUSED")
                    .size(12)
                    .color(iced::Color::WHITE),
            )
            .padding([4, 12])
            .style(|_: &iced::Theme| container::Style {
                background: Some(colors::LAVA.into()),
                border: iced::Border {
                    radius: 4.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }),
        );
    }

    if app.compact {
        header_items = header_items.push(
            container(
                text("COMPACT")
                    .size(12)
                    .color(colors::TEPHRA),
            )
            .padding([4, 12])
            .style(|_: &iced::Theme| container::Style {
                background: Some(colors::SCORIA.into()),
                border: iced::Border {
                    radius: 4.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }),
        );
    }

    // Keyboard hint bar
    let hints = text("Esc=Dashboard  1-5=Tabs  ←/→=Nodes  r=Reset  p=Pause  c=Compact  w=Workloads")
        .size(10)
        .color(colors::TEPHRA);

    let content = column![global_header(app, true), nav_bar, header_items, metrics_strip, tab_content, hints]
        .spacing(16)
        .padding(24);

    scrollable(content).into()
}

/// Top navigation: node tab strip + status/remove.
fn build_nav_bar<'a>(app: &'a App, node: &'a NodeState, active_tab: DetailTab) -> Element<'a, Message> {
    // Node tabs (for switching between nodes without going back to dashboard)
    let mut top_row = row![].spacing(4).align_y(iced::Alignment::Center);
    for id in &app.node_order {
        if let Some(n) = app.nodes.get(id) {
            let is_active = n.id == node.id;
            let tab_id = *id;
            let tab_btn = button(
                text(n.display_name())
                    .size(12)
                    .color(if is_active { colors::PUMICE } else { colors::TEPHRA }),
            )
            .on_press(Message::NavigateDetail(tab_id))
            .padding([4, 12])
            .style(move |_theme: &iced::Theme, status| {
                let bg = if is_active {
                    Some(colors::SCORIA.into())
                } else {
                    match status {
                        button::Status::Hovered => Some(colors::with_alpha(colors::SCORIA, 0.5).into()),
                        _ => None,
                    }
                };
                button::Style {
                    background: bg,
                    border: iced::Border {
                        radius: 4.0.into(),
                        ..Default::default()
                    },
                    text_color: if is_active { colors::PUMICE } else { colors::TEPHRA },
                    ..Default::default()
                }
            });
            top_row = top_row.push(tab_btn);
        }
    }

    // Throttle badge — stays visible (dimmed) for 4s after throttle ends
    let throttle_active = node.snapshot.as_ref().is_some_and(|s| s.throttle_active);
    if throttle_active {
        let reason = node.snapshot.as_ref().map(|s| s.throttle_reason.as_str()).unwrap_or("");
        top_row = top_row.push(Space::new().width(8));
        top_row = top_row.push(throttle_badge(reason, false));
    } else if node.is_throttle_lingering() {
        top_row = top_row.push(Space::new().width(8));
        top_row = top_row.push(throttle_badge(&node.last_throttle_reason, true));
    }

    let status = status_badge_with_retry(&node.status, node.id);

    let remove_btn = button(
        text("Remove").size(11).color(colors::TEPHRA),
    )
    .on_press(Message::RemoveNode(node.id))
    .padding([4, 12])
    .style(|_theme: &iced::Theme, status| {
        let border_color = match status {
            button::Status::Hovered => colors::MAGMA,
            _ => colors::SCORIA,
        };
        button::Style {
            background: None,
            border: iced::Border {
                color: border_color,
                width: 1.0,
                radius: 4.0.into(),
            },
            text_color: colors::TEPHRA,
            ..Default::default()
        }
    });

    top_row = top_row
        .push(Space::new().width(Length::Fill))
        .push(status)
        .push(Space::new().width(8))
        .push(remove_btn);

    // Detail tabs: Overview | Cores | Events | History
    let detail_tabs = build_detail_tabs(active_tab);

    column![top_row, detail_tabs]
        .spacing(8)
        .into()
}

/// Overview | Cores | Events tab strip.
fn build_detail_tabs(active: DetailTab) -> Element<'static, Message> {
    let tabs = [
        (DetailTab::Overview, "Overview"),
        (DetailTab::Cores, "Cores"),
        (DetailTab::Events, "Events"),
        (DetailTab::History, "History"),
        (DetailTab::Alerts, "Alerts"),
    ];

    let mut tab_row = row![].spacing(2);
    for (tab, label) in tabs {
        let is_active = tab == active;
        let tab_btn = button(
            text(label)
                .size(13)
                .color(if is_active { colors::EMBER } else { colors::TEPHRA }),
        )
        .on_press(Message::SwitchTab(tab))
        .padding([6, 16])
        .style(move |_theme: &iced::Theme, _status| {
            let border = if is_active {
                iced::Border {
                    color: colors::EMBER,
                    width: 0.0,
                    radius: 4.0.into(),
                }
            } else {
                iced::Border {
                    radius: 4.0.into(),
                    ..Default::default()
                }
            };
            let bg = if is_active {
                Some(colors::with_alpha(colors::EMBER, 0.1).into())
            } else {
                None
            };
            button::Style {
                background: bg,
                border,
                text_color: if is_active { colors::EMBER } else { colors::TEPHRA },
                ..Default::default()
            }
        });
        tab_row = tab_row.push(tab_btn);
    }

    tab_row.into()
}

/// Summary metrics strip at the top of the detail view.
fn build_metrics_strip(node: &NodeState) -> Element<'_, Message> {
    let Some(snap) = &node.snapshot else {
        return text("Waiting for data...")
            .size(14)
            .color(colors::TEPHRA)
            .into();
    };

    use crate::node::history::{compute_sigma, compute_trend, compute_trend_smooth};

    let temp_rate = snap.temp_rate_cs as f64 / 100.0;
    let thermal_state = thermal_state_label(temp_rate);
    let thermal_color = thermal_state_color(temp_rate);

    let temp_trend = compute_trend(&node.history.temp_c);
    let power_trend = compute_trend_smooth(&node.history.ppt_watts); // 10s window, ±5%
    let freq_trend = compute_trend(&node.history.avg_freq_mhz);
    let util_trend = compute_trend_smooth(&node.history.avg_util_pct); // also noisy

    // Thermal stability: σ over last 60 samples (~30s)
    let sigma_str = match compute_sigma(&node.history.temp_c, 60) {
        Some(s) => format!("σ{:.1}", s),
        None => String::new(),
    };

    // dT/dt as numeric value
    let rate_str = format!("{:+.1}°/s", temp_rate);

    let temp_subtitle = if sigma_str.is_empty() {
        format!("peak {}° | {} | {}", snap.peak_temp, thermal_state, rate_str)
    } else {
        format!("peak {}° | {} | {} | {}", snap.peak_temp, thermal_state, rate_str, sigma_str)
    };

    let temp = metric_pill_styled(
        "Temperature",
        &format!("{}°C", snap.temp_c),
        &temp_subtitle,
        colors::temp_color(snap.temp_c),
        Some(temp_trend),
        Some(thermal_color),
    );

    // Power subtitle with per-core power
    let per_core_str = match node.per_core_power() {
        Some(pcw) => format!(" | {:.1}W/{}c", pcw, node.busy_core_count()),
        None => String::new(),
    };
    let power = metric_pill(
        "Package Power",
        &format!("{:.1} W", snap.ppt_watts),
        &format!("peak {:.1}W | {:.3} Wh{}", snap.peak_ppt, snap.energy_wh, per_core_str),
        colors::power_color(snap.ppt_watts),
        Some(power_trend),
    );

    // Efficiency with baseline delta — compute color separately
    let (efficiency_str, efficiency_color) = match node.efficiency_delta() {
        Some((eff, delta)) => {
            let sign = if delta >= 0.0 { "+" } else { "" };
            let color = if delta < -5.0 {
                colors::MAGMA
            } else if delta > 5.0 {
                colors::GEOTHERMAL
            } else {
                colors::TEPHRA
            };
            (format!("{:.0} MHz/W ({sign}{:.0}%)", eff, delta), color)
        }
        None if snap.ppt_watts > 0.1 => (
            format!("{:.0} MHz/W", snap.avg_freq_mhz as f64 / snap.ppt_watts),
            colors::TEPHRA,
        ),
        _ => ("—".to_string(), colors::TEPHRA),
    };

    // Clock ratio color coding
    let clock_ratio = node.system_info.as_ref().map(|info| {
        if info.max_freq_mhz > 0 {
            snap.avg_freq_mhz as f64 / info.max_freq_mhz as f64 * 100.0
        } else {
            0.0
        }
    });
    let clock_ratio_str = match clock_ratio {
        Some(r) => format!(" | {:.0}% of max", r),
        None => String::new(),
    };

    let freq_subtitle = format!("peak {}{} | {}", snap.peak_freq, clock_ratio_str, efficiency_str);
    // Use efficiency delta to color the frequency pill accent
    let freq_accent = if efficiency_color == colors::TEPHRA {
        colors::MINERAL
    } else {
        efficiency_color
    };
    let freq = metric_pill(
        "Frequency",
        &format!("{} MHz", snap.avg_freq_mhz),
        &freq_subtitle,
        freq_accent,
        Some(freq_trend),
    );

    // Utilization pill with active workload indicator
    let util_subtitle = if let Some(active) = &node.active_workload {
        format!(
            "{} cores | WORKLOAD #{} [{:.0}s]",
            snap.cores.len(),
            active.id,
            node.workload_elapsed_secs(),
        )
    } else {
        format!("{} cores", snap.cores.len())
    };

    let util = metric_pill(
        "Utilization",
        &format!("{:.1}%", snap.avg_util_pct),
        &util_subtitle,
        colors::util_color(snap.avg_util_pct),
        Some(util_trend),
    );

    let mut metrics = row![temp, power, freq, util].spacing(24);

    // Fan pill if detected
    if snap.fan_detected {
        let fan_color = if snap.fan_rpm == 0 {
            colors::MAGMA
        } else {
            colors::TEPHRA
        };
        let fan_trend = compute_trend(&node.history.fan_rpm);
        let fan = metric_pill(
            "Fan",
            &format!("{} RPM", snap.fan_rpm),
            &format!("peak {}", snap.peak_fan),
            fan_color,
            Some(fan_trend),
        );
        metrics = metrics.push(fan);
    }

    // Throttle flash: yellow border highlight for 2s after state change
    let throttle_flashing = node.is_throttle_flashing();

    let mut strip = column![].spacing(8);
    strip = strip.push(metrics);

    // System info line with event counts right-justified
    if let Some(info) = &node.system_info {
        let version_str = if info.agent_version.is_empty() {
            String::new()
        } else {
            format!(" | v{}", info.agent_version)
        };
        let info_text = text(format!(
            "{} | {} | {} | Uptime: {}{}",
            info.cpu_model,
            info.scaling_driver,
            info.governor,
            format_uptime(snap.uptime_secs),
            version_str,
        ))
        .size(11)
        .color(colors::TEPHRA);

        let mut info_row = row![info_text].align_y(iced::Alignment::Center);

        // Session-only event counts right-justified on the same line
        let session_thermal = node.throttle_log.iter().filter(|e| e.reason == "thermal").count();
        let session_power = node.throttle_log.iter().filter(|e| e.reason != "thermal").count();
        if session_thermal > 0 || session_power > 0 || node.throttle_ticks > 0 {
            let throttle_time = node.throttle_secs();
            let time_str = if throttle_time > 0.0 {
                format!(" | {:.1}s throttled", throttle_time)
            } else {
                String::new()
            };
            let events_text = text(format!(
                "Thermal: {} Power: {}{}",
                session_thermal, session_power, time_str
            ))
            .size(11)
            .color(if session_thermal > 0 { colors::MAGMA } else { colors::TEPHRA });
            info_row = info_row
                .push(Space::new().width(Length::Fill))
                .push(events_text);
        }

        strip = strip.push(info_row);
    }

    // Version compatibility warning
    if let Some(warning) = &node.version_warning {
        strip = strip.push(
            text(warning.clone())
                .size(11)
                .color(colors::LAVA),
        );
    }

    let border_color = if throttle_flashing {
        colors::LAVA
    } else {
        colors::SCORIA
    };
    let border_width = if throttle_flashing { 2.0 } else { 1.0 };

    container(strip.padding(16))
        .width(Length::Fill)
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(colors::BASALT.into()),
            border: iced::Border {
                color: border_color,
                width: border_width,
                radius: 8.0.into(),
            },
            ..Default::default()
        })
        .into()
}

/// Build the content for the active tab.
fn build_tab_content<'a>(app: &'a App, node: &'a NodeState, tab: DetailTab) -> Element<'a, Message> {
    match tab {
        DetailTab::Overview => build_overview(node, app.compact),
        DetailTab::Cores => build_cores(node),
        DetailTab::Events => build_events(node),
        DetailTab::History => build_history(node),
        DetailTab::Alerts => crate::view::settings::alerts_tab(node, &app.alert_defaults),
    }
}

/// Overview tab: 2x2 grid of live charts.
fn build_overview<'a>(node: &'a NodeState, compact: bool) -> Element<'a, Message> {
    use crate::view::charts::line_chart::{line_chart, LineChartConfig};

    let chart_style = |_theme: &iced::Theme| container::Style {
        background: Some(colors::BASALT.into()),
        border: iced::Border {
            color: colors::SCORIA,
            width: 1.0,
            radius: 8.0.into(),
        },
        ..Default::default()
    };

    let temp_chart = container(
        line_chart(
            &node.history.temp_c,
            &LineChartConfig { color: colors::EMBER, label: "Temperature", unit: "°C", peak: node.snapshot.as_ref().map(|s| s.peak_temp as f64), threshold: Some(95.0), y_min: Some(25.0), y_max: Some(100.0) },
            &node.caches.temp,
        ),
    )
    .width(Length::Fill)
    .padding(4)
    .style(chart_style);

    let power_chart = container(
        line_chart(
            &node.history.ppt_watts,
            &LineChartConfig { color: colors::COPPER, label: "Power", unit: "W", peak: node.snapshot.as_ref().map(|s| s.peak_ppt), threshold: None, y_min: None, y_max: None },
            &node.caches.power,
        ),
    )
    .width(Length::Fill)
    .padding(4)
    .style(chart_style);

    let freq_chart = container(
        line_chart(
            &node.history.avg_freq_mhz,
            &LineChartConfig { color: colors::MINERAL, label: "Frequency", unit: "MHz", peak: node.snapshot.as_ref().map(|s| s.peak_freq as f64), threshold: None, y_min: None, y_max: None },
            &node.caches.freq,
        ),
    )
    .width(Length::Fill)
    .padding(4)
    .style(chart_style);

    let util_chart = container(
        line_chart(
            &node.history.avg_util_pct,
            &LineChartConfig { color: colors::LAVA, label: "Utilization", unit: "%", peak: None, threshold: None, y_min: None, y_max: None },
            &node.caches.util,
        ),
    )
    .width(Length::Fill)
    .padding(8)
    .style(chart_style);

    let top_row = row![temp_chart, power_chart].spacing(16);
    let bot_row = row![freq_chart, util_chart].spacing(16);

    let mut chart_col = column![].spacing(16);

    // Temperature duration curve first (most important graph)
    if !compact {
        let duration_section = build_temp_duration(node);
        chart_col = chart_col.push(duration_section);
    }

    chart_col = chart_col.push(top_row).push(bot_row);

    // Fan chart (only if fan detected and not compact)
    let fan_detected = node.snapshot.as_ref().is_some_and(|s| s.fan_detected);
    if fan_detected && !compact {
        // Check for fan stopped warning
        let fan_rpm = node.snapshot.as_ref().map(|s| s.fan_rpm).unwrap_or(0);
        let had_fan_running = node.history.fan_rpm.iter().any(|&v| v > 0.0);
        let fan_stopped = fan_rpm == 0 && had_fan_running;

        let fan_label = if fan_stopped {
            "Fan RPM  !! FAN STOPPED"
        } else {
            "Fan RPM"
        };
        let fan_color = if fan_stopped {
            colors::MAGMA
        } else if fan_rpm < 500 {
            colors::LAVA
        } else {
            colors::GEOTHERMAL
        };

        let fan_chart = container(
            line_chart(
                &node.history.fan_rpm,
                &LineChartConfig {
                    color: fan_color,
                    label: fan_label,
                    unit: "RPM",
                    peak: node.snapshot.as_ref().map(|s| s.peak_fan as f64),
                    threshold: None,
                    y_min: Some(0.0),
                    y_max: None,
                },
                &node.caches.fan,
            ),
        )
        .width(Length::Fill)
        .padding(4)
        .style(chart_style);

        chart_col = chart_col.push(fan_chart);
    }

    chart_col.into()
}

/// Temperature duration curve — fixed range from 60°C to 100°C in 10° steps.
/// Temperature duration curve — rolling 10-degree window.
/// Starts at 60–69°C (all zeros), slides up as peak rises. Never below 60°C.
/// Bars represent longest continuous streak at-or-above each degree.
/// Active portion (currently ongoing streak) shown with brighter color.
fn build_temp_duration<'a>(node: &'a NodeState) -> Element<'a, Message> {
    let peak = node.client_peak_temp();
    let top_temp = peak.max(69) as u32;
    let bot_temp = (top_temp - 9).max(60);

    // Normalize bar width to the longest streak at the bottom (widest) degree
    let max_streak = node.longest_streak_secs(bot_temp as i32);

    let mut rows = column![
        text("Temp Duration (longest continuous / total at or above)")
            .size(12)
            .color(colors::PUMICE),
    ]
    .spacing(2);

    for temp in (bot_temp..=top_temp).rev() {
        let streak = node.longest_streak_secs(temp as i32);
        let total = node.cumulative_temp_secs(temp as i32);
        let current = node.current_streak_secs(temp as i32);

        let ratio = if max_streak > 0.0 {
            (streak / max_streak).min(1.0) as f32
        } else {
            0.0
        };
        let temp_color = colors::temp_color(temp as i32);

        // Split bar into active (current ongoing) and historical portions
        let active_ratio = if streak > 0.0 {
            (current / streak).min(1.0) as f32
        } else {
            0.0
        };

        let total_bar_pct = (ratio * 100.0) as u16 + 1;
        let active_pct = (active_ratio * total_bar_pct as f32) as u16;
        let historical_pct = total_bar_pct.saturating_sub(active_pct);
        let empty_pct = (100u16).saturating_sub(total_bar_pct).max(1);

        let label = text(format!("{temp}°C"))
            .size(11)
            .color(temp_color);

        let time_label = text(format!(
            "{} ({})",
            format_short_duration(streak),
            format_short_duration(total),
        ))
        .size(10)
        .color(colors::TEPHRA);

        // Build bar row: only include active/hist segments when non-zero
        let mut bar_row = row![container(label).width(45)]
            .spacing(2)
            .align_y(iced::Alignment::Center);

        if active_pct > 0 {
            bar_row = bar_row.push(
                container(Space::new())
                    .width(Length::FillPortion(active_pct))
                    .height(12)
                    .style(move |_: &iced::Theme| container::Style {
                        background: Some(colors::with_alpha(temp_color, 0.85).into()),
                        ..Default::default()
                    }),
            );
        }
        if historical_pct > 0 {
            bar_row = bar_row.push(
                container(Space::new())
                    .width(Length::FillPortion(historical_pct))
                    .height(12)
                    .style(move |_: &iced::Theme| container::Style {
                        background: Some(colors::with_alpha(temp_color, 0.45).into()),
                        ..Default::default()
                    }),
            );
        }

        bar_row = bar_row
            .push(Space::new().width(Length::FillPortion(empty_pct)))
            .push(time_label);

        rows = rows.push(bar_row);
    }

    container(rows.padding(12))
        .width(Length::Fill)
        .style(|_: &iced::Theme| container::Style {
            background: Some(colors::BASALT.into()),
            border: iced::Border {
                color: colors::SCORIA,
                width: 1.0,
                radius: 8.0.into(),
            },
            ..Default::default()
        })
        .into()
}

fn format_short_duration(secs: f64) -> String {
    if secs >= 60.0 {
        format!("{}m{:.0}s", secs as u64 / 60, secs % 60.0)
    } else {
        format!("{:.1}s", secs)
    }
}


/// Cores tab: per-core heatmap using Canvas.
fn build_cores<'a>(node: &'a NodeState) -> Element<'a, Message> {
    use crate::view::charts::core_grid::core_grid;

    let Some(snap) = &node.snapshot else {
        return text("Waiting for data...").size(14).color(colors::TEPHRA).into();
    };

    container(
        column![
            text("Per-Core Frequency & Utilization")
                .size(14)
                .color(colors::PUMICE),
            core_grid(&snap.cores),
        ]
        .spacing(12)
        .padding(16),
    )
    .width(Length::Fill)
    .style(|_theme: &iced::Theme| container::Style {
        background: Some(colors::BASALT.into()),
        border: iced::Border {
            color: colors::SCORIA,
            width: 1.0,
            radius: 8.0.into(),
        },
        ..Default::default()
    })
    .into()
}

/// Events tab: throttle events + workload segments with full stats.
fn build_events<'a>(node: &'a NodeState) -> Element<'a, Message> {
    let event_style = |_theme: &iced::Theme| container::Style {
        background: Some(colors::BASALT.into()),
        border: iced::Border {
            color: colors::SCORIA,
            width: 1.0,
            radius: 6.0.into(),
        },
        ..Default::default()
    };

    // -- Throttle Events Section --
    let throttle_header = row![
        text("Throttle Events").size(14).color(colors::PUMICE),
        Space::new().width(Length::Fill),
        text(format!("{} events", node.throttle_log.len())).size(11).color(colors::TEPHRA),
    ].align_y(iced::Alignment::Center);

    let mut throttle_col = column![throttle_header].spacing(6);

    if node.throttle_log.is_empty() {
        throttle_col = throttle_col.push(
            text("No throttle events recorded").size(12).color(colors::TEPHRA),
        );
    } else {
        for evt in node.throttle_log.iter().rev().take(30) {
            let (icon, color) = if evt.reason == "thermal" {
                ("T", colors::MAGMA)
            } else {
                ("P", colors::COPPER)
            };
            let evt_row = container(
                row![
                    text(icon).size(12).color(color),
                    Space::new().width(8),
                    text(format!("{}°C", evt.temp_c)).size(12).color(colors::PUMICE),
                    Space::new().width(8),
                    text(format!("{:.1}W", evt.ppt_watts)).size(12).color(colors::PUMICE),
                    Space::new().width(8),
                    text(&evt.reason).size(11).color(colors::TEPHRA),
                ]
                .align_y(iced::Alignment::Center)
                .padding([4, 8]),
            )
            .style(event_style);
            throttle_col = throttle_col.push(evt_row);
        }
    }

    // Throttle time summary
    if node.throttle_ticks > 0 {
        throttle_col = throttle_col.push(
            text(format!("Total throttle time: {:.1}s", node.throttle_secs()))
                .size(11)
                .color(colors::LAVA),
        );
    }

    // -- Workloads Section --
    let workload_header = row![
        text("Workloads").size(14).color(colors::PUMICE),
        Space::new().width(Length::Fill),
        text(format!("{} completed", node.completed_workloads.len())).size(11).color(colors::TEPHRA),
    ].align_y(iced::Alignment::Center);

    let mut workload_col = column![workload_header].spacing(6);

    // Active workload first
    if let Some(active) = &node.active_workload {
        let active_card = container(
            row![
                text(format!("#{}", active.id)).size(12).color(colors::GEOTHERMAL),
                Space::new().width(8),
                text(format!("Started {}", active.start_time)).size(12).color(colors::PUMICE),
                Space::new().width(8),
                text("ACTIVE").size(10).color(colors::GEOTHERMAL),
            ]
            .align_y(iced::Alignment::Center)
            .padding([4, 8]),
        )
        .style(|_theme: &iced::Theme| container::Style {
            background: Some(colors::with_alpha(colors::GEOTHERMAL, 0.08).into()),
            border: iced::Border {
                color: colors::GEOTHERMAL,
                width: 1.0,
                radius: 6.0.into(),
            },
            ..Default::default()
        });
        workload_col = workload_col.push(active_card);
    }

    if node.completed_workloads.is_empty() && node.active_workload.is_none() {
        workload_col = workload_col.push(
            text("No workloads detected").size(12).color(colors::TEPHRA),
        );
    } else {
        for wl in node.completed_workloads.iter().rev().take(15) {
            let has_throttle = wl.thermal_events > 0 || wl.power_events > 0;
            let wl_color = if has_throttle { colors::LAVA } else { colors::MINERAL };

            let line1 = row![
                text(format!("#{}", wl.id)).size(12).color(wl_color),
                Space::new().width(6),
                text(format!("{} → {}", wl.start_time, wl.end_time)).size(12).color(colors::PUMICE),
                Space::new().width(6),
                text(format!("{:.0}s", wl.duration_secs)).size(12).color(colors::TEPHRA),
            ].align_y(iced::Alignment::Center);

            let line2 = row![
                text(format!("avg {:.0}°C", wl.avg_temp)).size(11).color(colors::TEPHRA),
                Space::new().width(6),
                text(format!("peak {}°C", wl.peak_temp)).size(11).color(colors::temp_color(wl.peak_temp)),
                Space::new().width(6),
                text(format!("avg {:.1}W", wl.avg_ppt)).size(11).color(colors::TEPHRA),
                Space::new().width(6),
                text(format!("{:.1}%", wl.avg_util)).size(11).color(colors::TEPHRA),
                Space::new().width(6),
                text(format!("{:.3} Wh", wl.energy_wh)).size(11).color(colors::TEPHRA),
            ].align_y(iced::Alignment::Center);

            let mut wl_content = column![line1, line2].spacing(2).padding([4, 8]);
            if has_throttle {
                wl_content = wl_content.push(
                    text(format!("Throttle: {} thermal, {} power", wl.thermal_events, wl.power_events))
                        .size(10)
                        .color(colors::MAGMA),
                );
            }

            let wl_card = container(wl_content).style(event_style);
            workload_col = workload_col.push(wl_card);
        }
    }

    // Put both sections side by side, each scrollable anchored to bottom
    let content = row![
        container(
            scrollable(throttle_col.padding(12))
                .anchor_bottom()
                .height(Length::Shrink),
        )
        .width(Length::Fill)
        .style(|_theme: &iced::Theme| container::Style {
            background: Some(colors::BASALT.into()),
            border: iced::Border { color: colors::SCORIA, width: 1.0, radius: 8.0.into() },
            ..Default::default()
        }),
        container(
            scrollable(workload_col.padding(12))
                .anchor_bottom()
                .height(Length::Shrink),
        )
        .width(Length::Fill)
        .style(|_theme: &iced::Theme| container::Style {
            background: Some(colors::BASALT.into()),
            border: iced::Border { color: colors::SCORIA, width: 1.0, radius: 8.0.into() },
            ..Default::default()
        }),
    ].spacing(16);

    content.into()
}

/// History tab: server-reported all-time stats from before this session.
fn build_history<'a>(node: &'a NodeState) -> Element<'a, Message> {
    let Some(snap) = &node.snapshot else {
        return text("Waiting for data...").size(14).color(colors::TEPHRA).into();
    };

    let stat_row = |label: &'static str, value: String, color: iced::Color| -> Element<'a, Message> {
        row![
            container(text(label).size(12).color(colors::TEPHRA)).width(180),
            text(value).size(12).color(color),
        ]
        .align_y(iced::Alignment::Center)
        .spacing(8)
        .into()
    };

    let mut col = column![
        text("Server Lifetime Stats").size(14).color(colors::PUMICE),
        text("Cumulative statistics reported by the server since it started.")
            .size(11)
            .color(colors::TEPHRA),
        Space::new().height(8),
    ]
    .spacing(4);

    col = col.push(stat_row(
        "Uptime",
        format_uptime(snap.uptime_secs),
        colors::PUMICE,
    ));
    col = col.push(stat_row(
        "Total energy consumed",
        format!("{:.3} Wh", snap.energy_wh),
        colors::PUMICE,
    ));

    col = col.push(Space::new().height(12));
    col = col.push(text("All-Time Peaks").size(13).color(colors::PUMICE));

    col = col.push(stat_row(
        "Peak temperature",
        format!("{}°C", snap.peak_temp),
        colors::temp_color(snap.peak_temp),
    ));
    col = col.push(stat_row(
        "Peak power draw",
        format!("{:.1} W", snap.peak_ppt),
        colors::power_color(snap.peak_ppt),
    ));
    col = col.push(stat_row(
        "Peak frequency",
        format!("{} MHz", snap.peak_freq),
        colors::MINERAL,
    ));
    if snap.fan_detected {
        col = col.push(stat_row(
            "Peak fan speed",
            format!("{} RPM", snap.peak_fan),
            colors::PUMICE,
        ));
    }

    col = col.push(Space::new().height(12));
    col = col.push(text("Throttle History").size(13).color(colors::PUMICE));

    col = col.push(stat_row(
        "Thermal throttle events",
        format!("{}", snap.thermal_events),
        if snap.thermal_events > 0 { colors::MAGMA } else { colors::TEPHRA },
    ));
    col = col.push(stat_row(
        "Power throttle events",
        format!("{}", snap.power_events),
        if snap.power_events > 0 { colors::COPPER } else { colors::TEPHRA },
    ));

    if let Some(info) = &node.system_info {
        col = col.push(Space::new().height(12));
        col = col.push(text("System").size(13).color(colors::PUMICE));

        col = col.push(stat_row("CPU model", info.cpu_model.clone(), colors::PUMICE));
        col = col.push(stat_row("Cores", format!("{}", info.core_count), colors::PUMICE));
        col = col.push(stat_row("Max frequency", format!("{} MHz", info.max_freq_mhz), colors::PUMICE));
        col = col.push(stat_row("Scaling driver", info.scaling_driver.clone(), colors::PUMICE));
        col = col.push(stat_row("Governor", info.governor.clone(), colors::PUMICE));
        col = col.push(stat_row("RAM", format!("{:.1} GB", info.ram_gb), colors::PUMICE));
        col = col.push(stat_row("Agent version", info.agent_version.clone(), colors::PUMICE));
    }

    container(col.padding(12))
        .width(Length::Fill)
        .style(|_: &iced::Theme| container::Style {
            background: Some(colors::BASALT.into()),
            border: iced::Border {
                color: colors::SCORIA,
                width: 1.0,
                radius: 8.0.into(),
            },
            ..Default::default()
        })
        .into()
}

fn thermal_state_label(rate: f64) -> &'static str {
    if rate > 0.5 {
        "CLIMBING"
    } else if rate < -0.5 {
        "COOLING"
    } else if rate.abs() < 0.1 {
        "STABLE"
    } else {
        "SETTLING"
    }
}

fn thermal_state_color(rate: f64) -> iced::Color {
    if rate > 0.5 {
        colors::MAGMA      // CLIMBING — danger
    } else if rate < -0.5 {
        colors::GEOTHERMAL  // COOLING — good
    } else if rate.abs() < 0.1 {
        colors::MINERAL     // STABLE — calm
    } else {
        colors::LAVA        // SETTLING — transitional
    }
}

fn format_uptime(secs: f64) -> String {
    let total = secs as u64;
    let h = total / 3600;
    let m = (total % 3600) / 60;
    let s = total % 60;
    if h > 0 {
        format!("{h}h {m:02}m {s:02}s")
    } else if m > 0 {
        format!("{m}m {s:02}s")
    } else {
        format!("{s}s")
    }
}
