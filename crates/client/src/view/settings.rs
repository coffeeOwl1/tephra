use iced::widget::{button, column, container, row, text, scrollable, text_input, Space};
use iced::{Element, Length};

use crate::alerts::{AlertDefaults, AlertOverrides};
use crate::app::App;
use crate::message::Message;
use crate::node::NodeState;
use crate::theme::colors;
use crate::view::dashboard::global_header;

// ── Global Settings View ────────────────────────────────────────────────

pub fn view(app: &App) -> Element<'_, Message> {
    let header = global_header(app, true);

    let title = text("Settings").size(20).color(colors::PUMICE);
    let subtitle = text("Esc = Dashboard").size(10).color(colors::TEPHRA);

    // ── Nodes section ──
    let nodes_title = text("Nodes")
        .size(15)
        .color(colors::EMBER);

    let add_btn = button(
        text("+ Add Node")
            .size(13)
            .color(colors::EMBER),
    )
    .on_press(Message::OpenAddDialog)
    .padding([6, 16])
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
                radius: 6.0.into(),
            },
            text_color: colors::EMBER,
            ..Default::default()
        }
    });

    // ── Alert Defaults section ──
    let section_title = text("Alert Defaults")
        .size(15)
        .color(colors::EMBER);
    let section_desc = text("These thresholds apply to all nodes unless overridden.")
        .size(12)
        .color(colors::TEPHRA);

    let defaults = &app.alert_defaults;

    // Column header for the alert table
    let col_header = row![
        text("Alert type").size(11).color(colors::TEPHRA),
        Space::new().width(Length::Fill),
        container(text("Enabled").size(11).color(colors::TEPHRA)).width(60),
        container(text("Persist").size(11).color(colors::TEPHRA)).width(60),
    ]
    .width(560)
    .align_y(iced::Alignment::Center);

    let temp_row = temp_ceiling_row("Temperature ceiling", defaults.temp_ceiling, {
        let d = defaults.clone();
        move |new_val| {
            let mut updated = d.clone();
            updated.temp_ceiling = new_val;
            Message::UpdateAlertDefaults(updated)
        }
    });

    let temp_persist = persist_toggle(defaults.temp_persistent, {
        let d = defaults.clone();
        move |on| {
            let mut updated = d.clone();
            updated.temp_persistent = on;
            Message::UpdateAlertDefaults(updated)
        }
    });

    let temp_full_row: Element<'_, Message> = row![
        temp_row,
        Space::new().width(Length::Fill),
        temp_persist,
    ]
    .align_y(iced::Alignment::Center)
    .width(560)
    .into();

    let throttle_thermal_row = alert_toggle_row(
        "Thermal throttle",
        defaults.throttle_thermal,
        defaults.throttle_thermal_persistent,
        defaults,
        |d, v| d.throttle_thermal = v,
        |d, v| d.throttle_thermal_persistent = v,
    );

    let throttle_power_row = alert_toggle_row(
        "Power limit throttle",
        defaults.throttle_power,
        defaults.throttle_power_persistent,
        defaults,
        |d, v| d.throttle_power = v,
        |d, v| d.throttle_power_persistent = v,
    );

    let workload_row = alert_toggle_row(
        "Workload complete",
        defaults.workload_complete,
        defaults.workload_persistent,
        defaults,
        |d, v| d.workload_complete = v,
        |d, v| d.workload_persistent = v,
    );

    let connection_row = alert_toggle_row(
        "Connection lost",
        defaults.connection_lost,
        defaults.connection_persistent,
        defaults,
        |d, v| d.connection_lost = v,
        |d, v| d.connection_persistent = v,
    );

    // Notification timeout section
    let timeout_title = text("Notification Timeout")
        .size(15)
        .color(colors::EMBER);

    let timeout_row = timeout_stepper_row(
        "Auto-dismiss after",
        defaults.notification_timeout_secs,
        {
            let d = defaults.clone();
            move |new_val| {
                let mut updated = d.clone();
                updated.notification_timeout_secs = new_val;
                Message::UpdateAlertDefaults(updated)
            }
        },
    );

    let timeout_note = text("Persistent notifications ignore this and stay until dismissed.")
        .size(11)
        .color(colors::TEPHRA);

    let test_btn = button(
        text("Send test notification").size(13).color(colors::PUMICE),
    )
    .on_press(Message::SendTestNotification)
    .padding([8, 16])
    .style(|_theme: &iced::Theme, status| {
        let border_color = match status {
            button::Status::Hovered => colors::MINERAL,
            _ => colors::SCORIA,
        };
        button::Style {
            background: Some(colors::with_alpha(colors::MINERAL, 0.1).into()),
            border: iced::Border {
                color: border_color,
                width: 1.0,
                radius: 6.0.into(),
            },
            text_color: colors::PUMICE,
            ..Default::default()
        }
    });

    let content = column![
        title,
        subtitle,
        Space::new().height(20),
        nodes_title,
        add_btn,
        Space::new().height(16),
        section_title,
        section_desc,
        Space::new().height(12),
        col_header,
        temp_full_row,
        throttle_thermal_row,
        throttle_power_row,
        workload_row,
        connection_row,
        Space::new().height(16),
        timeout_title,
        Space::new().height(8),
        timeout_row,
        timeout_note,
        Space::new().height(16),
        test_btn,
    ]
    .spacing(8)
    .max_width(600);

    scrollable(
        column![header, Space::new().height(16), content]
            .padding(24)
            .width(Length::Fill),
    )
    .into()
}

// ── Per-Node Alerts Tab ─────────────────────────────────────────────────

pub fn alerts_tab<'a>(
    node: &'a NodeState,
    defaults: &'a AlertDefaults,
) -> Element<'a, Message> {
    let id = node.id;
    let o = &node.alert_overrides;

    let section_title = text("Alert Overrides")
        .size(15)
        .color(colors::EMBER);
    let section_desc = text("Customize alert thresholds for this node. Unset fields use global defaults.")
        .size(12)
        .color(colors::TEPHRA);

    // Display name
    let name_label = text("Display name").size(13).color(colors::PUMICE);
    let name_input = text_input(
        "hostname",
        node.custom_name.as_deref().unwrap_or(""),
    )
    .on_input(move |val| {
        let name = if val.is_empty() { None } else { Some(val) };
        Message::SetDisplayName(id, name)
    })
    .size(13)
    .padding([6, 10])
    .width(250)
    .style(|_theme: &iced::Theme, _status| text_input::Style {
        background: colors::BASALT.into(),
        border: iced::Border {
            color: colors::SCORIA,
            width: 1.0,
            radius: 4.0.into(),
        },
        icon: colors::TEPHRA,
        placeholder: colors::TEPHRA,
        value: colors::PUMICE,
        selection: colors::with_alpha(colors::EMBER, 0.3),
    });
    let name_row = row![name_label, Space::new().width(12), name_input]
        .align_y(iced::Alignment::Center);

    // Temperature ceiling override
    let temp_row = override_temp_row(id, "Temperature ceiling", defaults.temp_ceiling, o);

    // Throttle overrides
    let throttle_thermal_row = override_toggle_row(id, "Thermal throttle", defaults.throttle_thermal, o.clone(), |ov| ov.throttle_thermal, |ov, val| { ov.throttle_thermal = val; });
    let throttle_power_row = override_toggle_row(id, "Power limit throttle", defaults.throttle_power, o.clone(), |ov| ov.throttle_power, |ov, val| { ov.throttle_power = val; });

    // Workload complete override
    let workload_row = override_toggle_row(id, "Workload complete", defaults.workload_complete, o.clone(), |ov| ov.workload_complete, |ov, val| { ov.workload_complete = val; });

    // Connection lost override
    let connection_row = override_toggle_row(id, "Connection lost", defaults.connection_lost, o.clone(), |ov| ov.connection_lost, |ov, val| { ov.connection_lost = val; });

    // Reset all button
    let has_overrides = o.has_any();
    let mut reset_btn = button(
        text("Reset all to defaults").size(12).color(if has_overrides { colors::PUMICE } else { colors::TEPHRA }),
    )
    .padding([6, 12])
    .style(move |_theme: &iced::Theme, status| {
        let border_color = if !has_overrides {
            colors::SCORIA
        } else {
            match status {
                button::Status::Hovered => colors::MAGMA,
                _ => colors::SCORIA,
            }
        };
        button::Style {
            background: None,
            border: iced::Border {
                color: border_color,
                width: 1.0,
                radius: 6.0.into(),
            },
            text_color: if has_overrides { colors::PUMICE } else { colors::TEPHRA },
            ..Default::default()
        }
    });
    if has_overrides {
        reset_btn = reset_btn.on_press(Message::ClearNodeAlertOverrides(id));
    }

    let content = column![
        section_title,
        section_desc,
        Space::new().height(12),
        name_row,
        Space::new().height(8),
        temp_row,
        throttle_thermal_row,
        throttle_power_row,
        workload_row,
        connection_row,
        Space::new().height(12),
        reset_btn,
    ]
    .spacing(8)
    .padding(16);

    container(content)
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

// ── Shared Widgets ──────────────────────────────────────────────────────

/// Temperature ceiling row with [−] value [+] buttons.
fn temp_ceiling_row<F>(label: &str, value: i32, on_change: F) -> Element<'static, Message>
where
    F: Fn(i32) -> Message + Clone + 'static,
{
    let label_text = text(label.to_string()).size(13).color(colors::PUMICE);

    let minus = {
        let f = on_change.clone();
        let new_val = (value - 1).max(50);
        button(text("\u{2212}").size(14).color(colors::PUMICE))
            .on_press(f(new_val))
            .padding([4, 10])
            .style(stepper_style)
    };

    let val_display = text(format!("{value}\u{00b0}C"))
        .size(14)
        .color(colors::EMBER);

    let plus = {
        let new_val = (value + 1).min(105);
        button(text("+").size(14).color(colors::PUMICE))
            .on_press(on_change(new_val))
            .padding([4, 10])
            .style(stepper_style)
    };

    row![
        label_text,
        Space::new().width(Length::Fill),
        minus,
        container(val_display).padding([0, 8]),
        plus,
    ]
    .align_y(iced::Alignment::Center)
    .width(500)
    .into()
}

/// Alert toggle row with enabled (On/Off) + persistent toggle for global settings.
fn alert_toggle_row(
    label: &str,
    enabled: bool,
    persistent: bool,
    defaults: &AlertDefaults,
    set_enabled: fn(&mut AlertDefaults, bool),
    set_persistent: fn(&mut AlertDefaults, bool),
) -> Element<'static, Message> {
    let label_text = text(label.to_string()).size(13).color(colors::PUMICE);

    let on_btn = {
        let is_active = enabled;
        let mut d = defaults.clone();
        set_enabled(&mut d, true);
        button(text("On").size(12).color(if is_active { colors::GEOTHERMAL } else { colors::TEPHRA }))
            .on_press(Message::UpdateAlertDefaults(d))
            .padding([4, 12])
            .style(move |_theme: &iced::Theme, _status| toggle_btn_style(is_active))
    };

    let off_btn = {
        let is_active = !enabled;
        let mut d = defaults.clone();
        set_enabled(&mut d, false);
        button(text("Off").size(12).color(if is_active { colors::MAGMA } else { colors::TEPHRA }))
            .on_press(Message::UpdateAlertDefaults(d))
            .padding([4, 12])
            .style(move |_theme: &iced::Theme, _status| toggle_btn_style(is_active))
    };

    let persist = persist_toggle(persistent, {
        let d = defaults.clone();
        move |on| {
            let mut updated = d.clone();
            set_persistent(&mut updated, on);
            Message::UpdateAlertDefaults(updated)
        }
    });

    row![
        label_text,
        Space::new().width(Length::Fill),
        on_btn,
        off_btn,
        Space::new().width(8),
        persist,
    ]
    .spacing(2)
    .align_y(iced::Alignment::Center)
    .width(560)
    .into()
}

/// Small persist/auto toggle button.
fn persist_toggle<F>(persistent: bool, on_change: F) -> Element<'static, Message>
where
    F: Fn(bool) -> Message + 'static,
{
    let label = if persistent { "Persist" } else { "Auto" };
    let color = if persistent { colors::LAVA } else { colors::TEPHRA };
    button(text(label).size(11).color(color))
        .on_press(on_change(!persistent))
        .padding([4, 8])
        .style(move |_theme: &iced::Theme, _status| {
            let bg = if persistent {
                Some(colors::with_alpha(colors::LAVA, 0.1).into())
            } else {
                None
            };
            button::Style {
                background: bg,
                border: iced::Border {
                    color: if persistent { colors::LAVA } else { colors::SCORIA },
                    width: if persistent { 0.0 } else { 1.0 },
                    radius: 4.0.into(),
                },
                text_color: color,
                ..Default::default()
            }
        })
        .into()
}

/// Timeout stepper row: label, [−] value [+]
fn timeout_stepper_row<F>(label: &str, value: u32, on_change: F) -> Element<'static, Message>
where
    F: Fn(u32) -> Message + Clone + 'static,
{
    let label_text = text(label.to_string()).size(13).color(colors::PUMICE);

    let minus = {
        let f = on_change.clone();
        let new_val = value.saturating_sub(1).max(1);
        button(text("\u{2212}").size(14).color(colors::PUMICE))
            .on_press(f(new_val))
            .padding([4, 10])
            .style(stepper_style)
    };

    let val_display = text(format!("{value}s"))
        .size(14)
        .color(colors::EMBER);

    let plus = {
        let new_val = (value + 1).min(60);
        button(text("+").size(14).color(colors::PUMICE))
            .on_press(on_change(new_val))
            .padding([4, 10])
            .style(stepper_style)
    };

    row![
        label_text,
        Space::new().width(Length::Fill),
        minus,
        container(val_display).padding([0, 8]),
        plus,
    ]
    .align_y(iced::Alignment::Center)
    .width(500)
    .into()
}

/// Per-node temperature override row: shows effective value, allows customize/reset.
fn override_temp_row(
    node_id: crate::node::NodeId,
    label: &str,
    default_val: i32,
    overrides: &AlertOverrides,
) -> Element<'static, Message> {
    let label_text = text(label.to_string()).size(13).color(colors::PUMICE);

    if let Some(custom_val) = overrides.temp_ceiling {
        // Has override — show stepper + reset
        let full_overrides = overrides.clone();
        let minus = {
            let mut ov = full_overrides.clone();
            let new_val = (custom_val - 1).max(50);
            ov.temp_ceiling = Some(new_val);
            button(text("\u{2212}").size(14).color(colors::PUMICE))
                .on_press(Message::SetNodeAlertOverride(node_id, ov))
                .padding([4, 10])
                .style(stepper_style)
        };

        let val_display = text(format!("{custom_val}\u{00b0}C"))
            .size(14)
            .color(colors::EMBER);

        let plus = {
            let mut ov = full_overrides.clone();
            let new_val = (custom_val + 1).min(105);
            ov.temp_ceiling = Some(new_val);
            button(text("+").size(14).color(colors::PUMICE))
                .on_press(Message::SetNodeAlertOverride(node_id, ov))
                .padding([4, 10])
                .style(stepper_style)
        };

        let reset = {
            let mut ov = full_overrides;
            ov.temp_ceiling = None;
            button(text("Reset").size(11).color(colors::TEPHRA))
                .on_press(Message::SetNodeAlertOverride(node_id, ov))
                .padding([4, 8])
                .style(reset_btn_style)
        };

        row![
            label_text,
            Space::new().width(Length::Fill),
            minus,
            container(val_display).padding([0, 8]),
            plus,
            Space::new().width(8),
            reset,
        ]
        .align_y(iced::Alignment::Center)
        .width(500)
        .into()
    } else {
        // Using default — show value + customize button
        let default_text = text(format!("{default_val}\u{00b0}C (default)"))
            .size(13)
            .color(colors::TEPHRA);

        let customize = {
            let mut ov = overrides.clone();
            ov.temp_ceiling = Some(default_val);
            button(text("Customize").size(11).color(colors::MINERAL))
                .on_press(Message::SetNodeAlertOverride(node_id, ov))
                .padding([4, 8])
                .style(customize_btn_style)
        };

        row![
            label_text,
            Space::new().width(Length::Fill),
            default_text,
            Space::new().width(8),
            customize,
        ]
        .align_y(iced::Alignment::Center)
        .width(500)
        .into()
    }
}

/// Per-node boolean override row. `get_field` reads the field, `set_field` writes it.
fn override_toggle_row(
    node_id: crate::node::NodeId,
    label: &str,
    default_val: bool,
    overrides: AlertOverrides,
    get_field: fn(&AlertOverrides) -> Option<bool>,
    set_field: fn(&mut AlertOverrides, Option<bool>),
) -> Element<'static, Message> {
    let label_text = text(label.to_string()).size(13).color(colors::PUMICE);
    let id = node_id;
    let override_val = get_field(&overrides);

    if let Some(custom_val) = override_val {
        // Has override — show toggle + reset
        let on_btn = {
            let is_active = custom_val;
            let mut ov = overrides.clone();
            set_field(&mut ov, Some(true));
            button(text("On").size(12).color(if is_active { colors::GEOTHERMAL } else { colors::TEPHRA }))
                .on_press(Message::SetNodeAlertOverride(id, ov))
                .padding([4, 12])
                .style(move |_theme: &iced::Theme, _status| toggle_btn_style(is_active))
        };

        let off_btn = {
            let is_active = !custom_val;
            let mut ov = overrides.clone();
            set_field(&mut ov, Some(false));
            button(text("Off").size(12).color(if is_active { colors::MAGMA } else { colors::TEPHRA }))
                .on_press(Message::SetNodeAlertOverride(id, ov))
                .padding([4, 12])
                .style(move |_theme: &iced::Theme, _status| toggle_btn_style(is_active))
        };

        let reset = {
            let mut ov = overrides;
            set_field(&mut ov, None);
            button(text("Reset").size(11).color(colors::TEPHRA))
                .on_press(Message::SetNodeAlertOverride(id, ov))
                .padding([4, 8])
                .style(reset_btn_style)
        };

        row![
            label_text,
            Space::new().width(Length::Fill),
            on_btn,
            off_btn,
            Space::new().width(8),
            reset,
        ]
        .spacing(2)
        .align_y(iced::Alignment::Center)
        .width(500)
        .into()
    } else {
        // Using default
        let val_str = if default_val { "On" } else { "Off" };
        let default_text = text(format!("{val_str} (default)"))
            .size(13)
            .color(colors::TEPHRA);

        let customize = {
            let mut ov = overrides;
            set_field(&mut ov, Some(default_val));
            button(text("Customize").size(11).color(colors::MINERAL))
                .on_press(Message::SetNodeAlertOverride(id, ov))
                .padding([4, 8])
                .style(customize_btn_style)
        };

        row![
            label_text,
            Space::new().width(Length::Fill),
            default_text,
            Space::new().width(8),
            customize,
        ]
        .align_y(iced::Alignment::Center)
        .width(500)
        .into()
    }
}

// ── Button Styles ───────────────────────────────────────────────────────

fn stepper_style(_theme: &iced::Theme, status: button::Status) -> button::Style {
    let border_color = match status {
        button::Status::Hovered => colors::EMBER,
        _ => colors::SCORIA,
    };
    button::Style {
        background: Some(colors::SCORIA.into()),
        border: iced::Border {
            color: border_color,
            width: 1.0,
            radius: 4.0.into(),
        },
        text_color: colors::PUMICE,
        ..Default::default()
    }
}

fn toggle_btn_style(is_active: bool) -> button::Style {
    let (bg, border_color) = if is_active {
        (
            Some(colors::with_alpha(colors::EMBER, 0.1).into()),
            colors::EMBER,
        )
    } else {
        (None, colors::SCORIA)
    };
    button::Style {
        background: bg,
        border: iced::Border {
            color: border_color,
            width: if is_active { 0.0 } else { 1.0 },
            radius: 4.0.into(),
        },
        text_color: colors::TEPHRA,
        ..Default::default()
    }
}

fn customize_btn_style(_theme: &iced::Theme, status: button::Status) -> button::Style {
    let border_color = match status {
        button::Status::Hovered => colors::MINERAL,
        _ => colors::SCORIA,
    };
    button::Style {
        background: None,
        border: iced::Border {
            color: border_color,
            width: 1.0,
            radius: 4.0.into(),
        },
        text_color: colors::MINERAL,
        ..Default::default()
    }
}

fn reset_btn_style(_theme: &iced::Theme, status: button::Status) -> button::Style {
    let border_color = match status {
        button::Status::Hovered => colors::TEPHRA,
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
}
