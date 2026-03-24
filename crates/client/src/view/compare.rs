use iced::widget::{button, column, container, pick_list, row, scrollable, text, Space};
use iced::{Color, Element, Length};

use crate::app::App;
use crate::message::{EventFilter, Message, SummaryColumn};
use crate::node::NodeId;
use crate::theme::colors;
use crate::view::charts::multi_line_chart::{multi_line_chart, ChartSeries, MultiLineConfig};
use crate::view::dashboard::global_header;

/// The comparison dashboard — all nodes overlaid on shared charts.
pub fn view(app: &App) -> Element<'_, Message> {
    let top_header = global_header(app, false);

    let sub_header = row![
        text("Node Comparison").size(20).color(colors::PUMICE),
    ]
    .align_y(iced::Alignment::Center);

    // Collect ordered nodes with assigned colors
    let nodes: Vec<_> = app
        .node_order
        .iter()
        .enumerate()
        .filter_map(|(idx, id)| app.nodes.get(id).map(|n| (idx, n)))
        .collect();

    if nodes.is_empty() {
        let content = column![
            top_header,
            sub_header,
            Space::new().height(60),
            text("No nodes to compare").size(16).color(colors::TEPHRA),
        ]
        .spacing(16)
        .padding(24);
        return scrollable(content).into();
    }

    let chart_style = |_theme: &iced::Theme| container::Style {
        background: Some(colors::BASALT.into()),
        border: iced::Border {
            color: colors::SCORIA,
            width: 1.0,
            radius: 8.0.into(),
        },
        ..Default::default()
    };

    // Pre-compute display names (owned)
    let names: Vec<String> = nodes.iter().map(|(_, n)| n.display_name()).collect();

    // -- Overlay charts --
    let make_series = |get_buf: fn(&crate::node::NodeState) -> &crate::node::history::RingBuffer<f64>| {
        nodes
            .iter()
            .enumerate()
            .map(|(i, (idx, n))| ChartSeries {
                data: get_buf(n),
                color: colors::node_color(*idx),
                label: names[i].clone(),
            })
            .collect::<Vec<_>>()
    };

    let temp_chart = container(multi_line_chart(
        make_series(|n| &n.history.temp_c),
        &MultiLineConfig {
            title: "Temperature",
            unit: "°C",
            y_min: Some(25.0),
            y_max: Some(100.0),
            threshold: Some(95.0),
        },
        &app.compare_caches.temp,
    ))
    .width(Length::Fill)
    .padding(4)
    .style(chart_style);

    let power_chart = container(multi_line_chart(
        make_series(|n| &n.history.ppt_watts),
        &MultiLineConfig {
            title: "Package Power",
            unit: "W",
            y_min: None,
            y_max: None,
            threshold: None,
        },
        &app.compare_caches.power,
    ))
    .width(Length::Fill)
    .padding(4)
    .style(chart_style);

    let freq_chart = container(multi_line_chart(
        make_series(|n| &n.history.avg_freq_mhz),
        &MultiLineConfig {
            title: "Frequency",
            unit: "MHz",
            y_min: None,
            y_max: None,
            threshold: None,
        },
        &app.compare_caches.freq,
    ))
    .width(Length::Fill)
    .padding(4)
    .style(chart_style);

    let util_chart = container(multi_line_chart(
        make_series(|n| &n.history.avg_util_pct),
        &MultiLineConfig {
            title: "Utilization",
            unit: "%",
            y_min: None,
            y_max: None,
            threshold: None,
        },
        &app.compare_caches.util,
    ))
    .width(Length::Fill)
    .padding(4)
    .style(chart_style);

    // -- Fleet power chart (sum of all nodes) --
    let fleet_power_chart = container(multi_line_chart(
        vec![ChartSeries {
            data: &app.fleet_power_history,
            color: colors::COPPER,
            label: "Fleet Total".into(),
        }],
        &MultiLineConfig {
            title: "Fleet Power",
            unit: "W",
            y_min: None,
            y_max: None,
            threshold: None,
        },
        &app.compare_caches.fleet_power,
    ))
    .width(Length::Fill)
    .padding(4)
    .style(chart_style);

    // -- Event console --
    let event_console = build_event_console(&nodes, &names, app.console_filter);

    // -- Summary table --
    let summary = build_summary_table(&nodes, &names, app.summary_sort);

    // -- Thermal ranking --
    let ranking = build_thermal_ranking(&nodes, &names);

    // -- Aggregate stats --
    let aggregates = build_aggregates(&nodes);

    let charts_top = row![temp_chart, power_chart].spacing(16);
    let charts_bot = row![freq_chart, util_chart].spacing(16);
    let charts_extra = row![fleet_power_chart, event_console].spacing(16);
    let stats_row = row![ranking].spacing(16);

    let hints = text("Esc=Dashboard  d=Toggle compare")
        .size(10)
        .color(colors::TEPHRA);

    let content = column![
        top_header,
        sub_header,
        aggregates,
        summary,
        stats_row,
        charts_top,
        charts_bot,
        charts_extra,
        hints,
    ]
    .spacing(16)
    .padding(24);

    scrollable(content).into()
}

/// Summary table: one row per node with key metrics, sortable by column.
/// Clickable node name that navigates to the node's detail overview.
fn node_name_btn(name: &str, size: f32, color: Color, node_id: NodeId) -> Element<'static, Message> {
    let name = name.to_string();
    button(text(name).size(size).color(color))
        .on_press(Message::NavigateDetail(node_id))
        .padding(0)
        .style(move |_theme: &iced::Theme, status| {
            let text_color = match status {
                button::Status::Hovered => colors::EMBER,
                _ => color,
            };
            button::Style {
                background: None,
                border: iced::Border::default(),
                text_color,
                ..Default::default()
            }
        })
        .into()
}

fn build_summary_table<'a>(
    nodes: &[(usize, &crate::node::NodeState)],
    names: &[String],
    sort: (SummaryColumn, bool),
) -> Element<'a, Message> {
    let (sort_col, ascending) = sort;

    // Sort indicator
    let sort_arrow = if ascending { " \u{25B2}" } else { " \u{25BC}" };

    let sort_header = |label: &str, col: SummaryColumn, w: u32| -> Element<'a, Message> {
        let display = if sort_col == col {
            format!("{}{}", label, sort_arrow)
        } else {
            label.to_string()
        };
        let label_color = if sort_col == col {
            colors::PUMICE
        } else {
            colors::TEPHRA
        };
        button(text(display).size(10).color(label_color))
            .on_press(Message::SetSummarySort(col))
            .padding([1, 2])
            .width(w)
            .style(|_: &iced::Theme, status| {
                let text_color = match status {
                    button::Status::Hovered => colors::EMBER,
                    _ => colors::TEPHRA,
                };
                button::Style {
                    background: None,
                    text_color,
                    ..Default::default()
                }
            })
            .into()
    };

    let header_row = row![
        sort_header("Node", SummaryColumn::Node, 110),
        sort_header("Cores", SummaryColumn::Cores, 42),
        sort_header("Temp", SummaryColumn::Temp, 50),
        sort_header("Peak", SummaryColumn::Peak, 45),
        sort_header("Cli Pk", SummaryColumn::ClientPeak, 50),
        sort_header("T≥95", SummaryColumn::T95, 50),
        sort_header("Power", SummaryColumn::Power, 55),
        sort_header("Pk Pwr", SummaryColumn::PeakPower, 55),
        sort_header("Freq", SummaryColumn::Freq, 60),
        sort_header("Util", SummaryColumn::Util, 45),
        sort_header("Eff", SummaryColumn::Efficiency, 60),
        sort_header("Fan", SummaryColumn::Fan, 55),
        sort_header("Energy", SummaryColumn::Energy, 60),
        sort_header("Uptime", SummaryColumn::Uptime, 65),
        sort_header("Thr t", SummaryColumn::ThrottleTime, 45),
        sort_header("Evts", SummaryColumn::Throttle, 40),
    ]
    .spacing(4);

    // Build sortable row data: (original_index, sort_key as f64)
    let mut indices: Vec<usize> = (0..nodes.len()).collect();
    indices.sort_by(|&a, &b| {
        let key = |i: usize| -> f64 {
            let (_, node) = &nodes[i];
            match sort_col {
                SummaryColumn::Node => 0.0, // sorted alphabetically below
                SummaryColumn::Cores => node
                    .system_info
                    .as_ref()
                    .map(|si| si.core_count as f64)
                    .unwrap_or(0.0),
                SummaryColumn::Temp => {
                    node.snapshot.as_ref().map(|s| s.temp_c as f64).unwrap_or(0.0)
                }
                SummaryColumn::Peak => {
                    node.snapshot.as_ref().map(|s| s.peak_temp as f64).unwrap_or(0.0)
                }
                SummaryColumn::ClientPeak => node.client_peak_temp() as f64,
                SummaryColumn::T95 => node.cumulative_temp_secs(95),
                SummaryColumn::Power => {
                    node.snapshot.as_ref().map(|s| s.ppt_watts).unwrap_or(0.0)
                }
                SummaryColumn::PeakPower => {
                    node.snapshot.as_ref().map(|s| s.peak_ppt).unwrap_or(0.0)
                }
                SummaryColumn::Freq => node
                    .snapshot
                    .as_ref()
                    .map(|s| s.avg_freq_mhz as f64)
                    .unwrap_or(0.0),
                SummaryColumn::Util => {
                    node.snapshot.as_ref().map(|s| s.avg_util_pct).unwrap_or(0.0)
                }
                SummaryColumn::Fan => {
                    node.snapshot.as_ref().map(|s| s.fan_rpm as f64).unwrap_or(0.0)
                }
                SummaryColumn::Energy => {
                    node.snapshot.as_ref().map(|s| s.energy_wh).unwrap_or(0.0)
                }
                SummaryColumn::Uptime => {
                    node.snapshot.as_ref().map(|s| s.uptime_secs).unwrap_or(0.0)
                }
                SummaryColumn::Efficiency => node
                    .snapshot
                    .as_ref()
                    .filter(|s| s.ppt_watts > 1.0)
                    .map(|s| s.avg_freq_mhz as f64 / s.ppt_watts)
                    .unwrap_or(0.0),
                SummaryColumn::ThrottleTime => node.throttle_secs(),
                SummaryColumn::Throttle => node
                    .snapshot
                    .as_ref()
                    .map(|s| (s.thermal_events + s.power_events) as f64)
                    .unwrap_or(0.0),
            }
        };
        if sort_col == SummaryColumn::Node {
            let cmp = names[a].to_lowercase().cmp(&names[b].to_lowercase());
            return if ascending { cmp } else { cmp.reverse() };
        }
        let ka = key(a);
        let kb = key(b);
        let cmp = ka.partial_cmp(&kb).unwrap_or(std::cmp::Ordering::Equal);
        if ascending { cmp } else { cmp.reverse() }
    });

    let mut col = column![
        text("Summary").size(14).color(colors::PUMICE),
        header_row,
    ]
    .spacing(4);

    for &i in &indices {
        let (idx, node) = &nodes[i];
        let color = colors::node_color(*idx);

        if let Some(snap) = &node.snapshot {
            let cores_str = node
                .system_info
                .as_ref()
                .map(|si| format!("{}", si.core_count))
                .unwrap_or_else(|| "—".into());

            let throttle_color = if snap.throttle_active { colors::MAGMA } else { colors::TEPHRA };

            let evts = snap.thermal_events + snap.power_events;
            let evts_str = if evts > 0 { format!("{}", evts) } else { "—".into() };

            let thr_time = node.throttle_secs();
            let thr_time_str = if thr_time > 0.0 {
                format_duration(thr_time)
            } else {
                "—".into()
            };

            let cli_pk = node.client_peak_temp();
            let cli_pk_color = colors::temp_color(cli_pk);

            let t95 = node.cumulative_temp_secs(95);
            let t95_str = if t95 > 0.0 { format_duration(t95) } else { "—".into() };
            let t95_color = if t95 > 0.0 { colors::temp_color(95) } else { colors::TEPHRA };

            let eff_str = if snap.ppt_watts > 1.0 {
                format!("{:.0} M/W", snap.avg_freq_mhz as f64 / snap.ppt_watts)
            } else {
                "—".into()
            };

            let fan_str = if snap.fan_detected {
                format!("{}", snap.fan_rpm)
            } else {
                "—".into()
            };

            let uptime_str = format_uptime(snap.uptime_secs);

            col = col.push(
                row![
                    container(node_name_btn(&names[i], 11.0, color, node.id)).width(110),
                    container(text(cores_str).size(11).color(colors::TEPHRA)).width(42),
                    container(
                        text(format!("{}°C", snap.temp_c))
                            .size(11)
                            .color(colors::temp_color(snap.temp_c)),
                    )
                    .width(50),
                    container(
                        text(format!("{}°", snap.peak_temp))
                            .size(11)
                            .color(colors::TEPHRA),
                    )
                    .width(45),
                    container(
                        text(format!("{}°", cli_pk))
                            .size(11)
                            .color(cli_pk_color),
                    )
                    .width(50),
                    container(text(t95_str).size(11).color(t95_color)).width(50),
                    container(
                        text(format!("{:.1}W", snap.ppt_watts))
                            .size(11)
                            .color(colors::TEPHRA),
                    )
                    .width(55),
                    container(
                        text(format!("{:.1}W", snap.peak_ppt))
                            .size(11)
                            .color(colors::TEPHRA),
                    )
                    .width(55),
                    container(
                        text(format!("{}MHz", snap.avg_freq_mhz))
                            .size(11)
                            .color(colors::TEPHRA),
                    )
                    .width(60),
                    container(
                        text(format!("{:.0}%", snap.avg_util_pct))
                            .size(11)
                            .color(colors::TEPHRA),
                    )
                    .width(45),
                    container(text(eff_str).size(11).color(colors::TEPHRA)).width(60),
                    container(text(fan_str).size(11).color(colors::TEPHRA)).width(55),
                    container(
                        text(format!("{:.2}Wh", snap.energy_wh))
                            .size(11)
                            .color(colors::TEPHRA),
                    )
                    .width(60),
                    container(text(uptime_str).size(11).color(colors::TEPHRA)).width(65),
                    container(text(thr_time_str).size(11).color(throttle_color)).width(45),
                    container(text(evts_str).size(11).color(throttle_color)).width(40),
                ]
                .spacing(4),
            );
        } else {
            col = col.push(
                row![
                    container(node_name_btn(&names[i], 11.0, color, node.id)).width(110),
                    text("connecting...").size(11).color(colors::TEPHRA),
                ]
                .spacing(4),
            );
        }
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

/// Format a short duration (seconds) compactly: "45s", "2m10s", "1h5m".
fn format_duration(secs: f64) -> String {
    let total = secs as u64;
    let h = total / 3600;
    let m = (total % 3600) / 60;
    let s = total % 60;
    if h > 0 {
        format!("{}h{}m", h, m)
    } else if m > 0 {
        format!("{}m{}s", m, s)
    } else {
        format!("{}s", s)
    }
}

/// Format uptime seconds into a human-readable string.
fn format_uptime(secs: f64) -> String {
    let total = secs as u64;
    let d = total / 86400;
    let h = (total % 86400) / 3600;
    let m = (total % 3600) / 60;
    if d > 0 {
        format!("{}d {}h", d, h)
    } else if h > 0 {
        format!("{}h {}m", h, m)
    } else {
        format!("{}m", m)
    }
}

/// Horizontal thermal ranking bars, sorted hottest-first.
fn build_thermal_ranking<'a>(
    nodes: &[(usize, &crate::node::NodeState)],
    names: &[String],
) -> Element<'a, Message> {
    let mut entries: Vec<(String, i32, iced::Color, NodeId)> = nodes
        .iter()
        .enumerate()
        .filter_map(|(i, (idx, n))| {
            let temp = n.snapshot.as_ref()?.temp_c;
            Some((names[i].clone(), temp, colors::node_color(*idx), n.id))
        })
        .collect();
    entries.sort_by(|a, b| b.1.cmp(&a.1)); // hottest first

    // Fixed 100°C ceiling — virtually all modern CPUs throttle at or below 100°C
    let max_temp = 100.0_f32;

    let mut col = column![
        text("Thermal Ranking").size(14).color(colors::PUMICE),
        // Header row to align with summary table
        row![
            container(text("Node").size(10).color(colors::TEPHRA)).width(100),
            Space::new().width(Length::Fill),
            container(text("Temp").size(10).color(colors::TEPHRA)).width(45),
        ]
        .spacing(4),
    ]
    .spacing(4);

    for (name, temp, color, node_id) in &entries {
        let ratio = (*temp as f32 / max_temp).clamp(0.0, 1.0);
        let bar_pct = (ratio * 100.0) as u16 + 1;
        let empty_pct = 100u16.saturating_sub(bar_pct).max(1);
        let temp_color = colors::temp_color(*temp);

        let bar = container(Space::new())
            .width(Length::FillPortion(bar_pct))
            .height(14)
            .style(move |_: &iced::Theme| container::Style {
                background: Some(colors::with_alpha(temp_color, 0.6).into()),
                ..Default::default()
            });

        col = col.push(
            row![
                container(node_name_btn(name, 10.0, *color, *node_id)).width(100),
                bar,
                Space::new().width(Length::FillPortion(empty_pct)),
                text(format!("{}°C", temp)).size(10).color(temp_color),
            ]
            .spacing(4)
            .align_y(iced::Alignment::Center),
        );
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

/// Aggregate fleet statistics.
fn build_aggregates<'a>(nodes: &[(usize, &crate::node::NodeState)]) -> Element<'a, Message> {
    let mut total_power = 0.0;
    let mut max_temp = 0i32;
    let mut max_temp_node = String::from("—");
    let mut total_throttle_events = 0u32;
    let mut active_throttle_count = 0u32;
    let mut total_energy = 0.0;

    for (_, node) in nodes {
        if let Some(snap) = &node.snapshot {
            total_power += snap.ppt_watts;
            if snap.temp_c > max_temp {
                max_temp = snap.temp_c;
                max_temp_node = node.display_name();
            }
            total_throttle_events += snap.thermal_events + snap.power_events;
            if snap.throttle_active {
                active_throttle_count += 1;
            }
            total_energy += snap.energy_wh;
        }
    }

    let metrics = row![
        stat_pill(
            "Fleet Power",
            &format!("{:.1} W", total_power),
            colors::COPPER,
        ),
        stat_pill(
            "Hottest Node",
            &format!("{}°C ({})", max_temp, max_temp_node),
            colors::temp_color(max_temp),
        ),
        stat_pill(
            "Active Throttle",
            &format!("{} / {}", active_throttle_count, nodes.len()),
            if active_throttle_count > 0 {
                colors::MAGMA
            } else {
                colors::GEOTHERMAL
            },
        ),
        stat_pill(
            "Total Events",
            &format!("{}", total_throttle_events),
            if total_throttle_events > 0 {
                colors::LAVA
            } else {
                colors::TEPHRA
            },
        ),
        stat_pill(
            "Total Energy",
            &format!("{:.2} Wh", total_energy),
            colors::COPPER,
        ),
    ]
    .spacing(24);

    container(metrics.padding(16))
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

fn stat_pill<'a>(label: &str, value: &str, color: iced::Color) -> Element<'a, Message> {
    let accent = container(Space::new())
        .width(3)
        .height(Length::Fill)
        .style(move |_: &iced::Theme| container::Style {
            background: Some(color.into()),
            ..Default::default()
        });

    let content = column![
        text(label.to_string()).size(11).color(colors::TEPHRA),
        text(value.to_string()).size(16).color(color),
    ]
    .spacing(2);

    container(row![accent, content].spacing(8).height(40))
        .width(Length::FillPortion(1))
        .into()
}

/// Running event console — throttle events from all nodes.
fn build_event_console<'a>(
    nodes: &[(usize, &crate::node::NodeState)],
    names: &[String],
    filter: EventFilter,
) -> Element<'a, Message> {
    // Collect all throttle events with node info, newest first
    let mut events: Vec<(String, Color, String, NodeId)> = Vec::new();

    for (i, (idx, node)) in nodes.iter().enumerate() {
        let color = colors::node_color(*idx);
        let name = &names[i];

        // Active throttle status
        if let Some(snap) = &node.snapshot {
            if snap.throttle_active {
                let reason_lc = snap.throttle_reason.to_lowercase();
                let matches = match filter {
                    EventFilter::All => true,
                    EventFilter::Thermal => reason_lc.contains("thermal"),
                    EventFilter::Power => reason_lc.contains("power"),
                };
                if matches {
                    events.push((
                        name.clone(),
                        color,
                        format!(
                            "ACTIVE {} — {}°C {:.1}W",
                            snap.throttle_reason.to_uppercase(),
                            snap.temp_c,
                            snap.ppt_watts,
                        ),
                        node.id,
                    ));
                }
            }
        }

        // Historical throttle events (most recent first, limit 20 per node)
        for ts_evt in node.throttle_log.iter().rev().take(20) {
            let evt = &ts_evt.event;
            let reason_lc = evt.reason.to_lowercase();
            let matches = match filter {
                EventFilter::All => true,
                EventFilter::Thermal => reason_lc.contains("thermal"),
                EventFilter::Power => reason_lc.contains("power"),
            };
            if !matches {
                continue;
            }
            let reason_color = if evt.reason == "thermal" {
                colors::ERUPTION
            } else {
                colors::COPPER
            };
            let time = ts_evt.received_at.format("%-I:%M:%S %p");
            events.push((
                name.clone(),
                reason_color,
                format!(
                    "{} {} — {}°C {:.1}W",
                    time,
                    evt.reason.to_uppercase(),
                    evt.temp_c,
                    evt.ppt_watts,
                ),
                node.id,
            ));
        }
    }

    // Limit total events shown
    events.truncate(50);

    // -- Fixed header bar --
    let filter_dropdown = pick_list(
        &EventFilter::ALL[..],
        Some(filter),
        Message::SetConsoleFilter,
    )
    .text_size(10)
    .padding([2, 6])
    .style(|_theme: &iced::Theme, _status| pick_list::Style {
        text_color: colors::PUMICE,
        placeholder_color: colors::TEPHRA,
        handle_color: colors::TEPHRA,
        background: colors::OBSIDIAN.into(),
        border: iced::Border {
            color: colors::SCORIA,
            width: 1.0,
            radius: 4.0.into(),
        },
    });

    let event_count = text(format!("{}", events.len()))
        .size(10)
        .color(colors::TEPHRA);

    let header_bar = container(
        row![
            text("Throttle Events").size(13).color(colors::PUMICE),
            Space::new().width(Length::Fill),
            event_count,
            filter_dropdown,
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center),
    )
    .width(Length::Fill)
    .padding([8, 12]);

    // -- Scrollable data rows --
    let mut data_col = column![].spacing(1);

    if events.is_empty() {
        data_col = data_col.push(
            container(
                text("No events").size(10).color(colors::TEPHRA),
            )
            .padding([4, 12]),
        );
    } else {
        for (name, color, detail, node_id) in &events {
            let event_row = container(
                row![
                    text(">").size(10).color(colors::SCORIA),
                    container(node_name_btn(name, 10.0, *color, *node_id))
                        .width(90),
                    text(detail.clone())
                        .size(10)
                        .color(*color)
                        .font(iced::Font::MONOSPACE),
                ]
                .spacing(6)
                .align_y(iced::Alignment::Center),
            )
            .width(Length::Fill)
            .padding([2, 12]);

            data_col = data_col.push(event_row);
        }
    }

    let console_body = scrollable(data_col)
        .anchor_bottom()
        .height(180);

    // Console-style container: dark bg, subtle inner border
    container(
        column![header_bar, console_body].spacing(0),
    )
    .width(Length::Fill)
    .style(|_: &iced::Theme| container::Style {
        background: Some(Color::from_rgb(0.05, 0.04, 0.04).into()),
        border: iced::Border {
            color: colors::SCORIA,
            width: 1.0,
            radius: 8.0.into(),
        },
        ..Default::default()
    })
    .into()
}
