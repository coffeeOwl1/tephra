use iced::widget::{column, container, row, text, Space};
use iced::{Color, Element, Length};

use crate::message::Message;
use crate::node::history::Trend;
use crate::theme::colors;

/// A labeled metric display with a colored left accent bar and optional trend arrow.
pub fn metric_pill<'a>(
    label: &str,
    value: &str,
    subtitle: &str,
    accent_color: Color,
    trend: Option<Trend>,
) -> Element<'a, Message> {
    metric_pill_styled(label, value, subtitle, accent_color, trend, None)
}

/// Metric pill with optional subtitle color override.
pub fn metric_pill_styled<'a>(
    label: &str,
    value: &str,
    subtitle: &str,
    accent_color: Color,
    trend: Option<Trend>,
    subtitle_color: Option<Color>,
) -> Element<'a, Message> {
    let accent_bar = container(Space::new())
        .width(3)
        .height(Length::Fill)
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(accent_color.into()),
            ..Default::default()
        });

    let label_text = text(label.to_string())
        .size(11)
        .color(colors::TEPHRA);

    let value_text = text(value.to_string())
        .size(20)
        .color(accent_color);

    // Trend arrow next to value
    let value_row = if let Some(t) = trend {
        let arrow = text(t.arrow())
            .size(16)
            .color(t.color());
        row![value_text, arrow].spacing(4).align_y(iced::Alignment::Center)
    } else {
        row![value_text].align_y(iced::Alignment::Center)
    };

    let subtitle_text = text(subtitle.to_string())
        .size(11)
        .color(subtitle_color.unwrap_or(colors::TEPHRA));

    let content = column![label_text, value_row, subtitle_text]
        .spacing(2);

    container(
        row![accent_bar, content]
            .spacing(8)
            .height(56),
    )
    .width(Length::FillPortion(1))
    .into()
}

/// Compact version for dashboard cards — single line with optional trend.
pub fn metric_compact<'a>(
    label: &str,
    value: &str,
    unit: &str,
    accent_color: Color,
    trend: Option<Trend>,
) -> Element<'a, Message> {
    let label_text = text(format!("{label}: "))
        .size(12)
        .color(colors::TEPHRA);

    let value_text = text(value.to_string())
        .size(14)
        .color(accent_color);

    let unit_text = text(unit.to_string())
        .size(11)
        .color(colors::TEPHRA);

    let mut r = row![label_text, value_text, unit_text]
        .spacing(3)
        .align_y(iced::Alignment::Center);

    if let Some(t) = trend {
        r = r.push(text(t.arrow()).size(12).color(t.color()));
    }

    r.into()
}
