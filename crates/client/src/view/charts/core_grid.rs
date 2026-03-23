use iced::widget::{column, container, row, text};
use iced::{Element, Length};

use crate::message::Message;
use crate::net::api_types::CoreSnapshot;
use crate::theme::colors;

/// Per-core grid built from iced widgets with busiest core highlighting.
pub fn core_grid(cores: &[CoreSnapshot]) -> Element<'_, Message> {
    if cores.is_empty() {
        return text("No core data").size(12).color(colors::TEPHRA).into();
    }

    // Find the busiest core (highest utilization, only if >20%)
    let busiest_idx = cores
        .iter()
        .enumerate()
        .filter(|(_, c)| c.util_pct > 20.0)
        .max_by(|(_, a), (_, b)| a.util_pct.partial_cmp(&b.util_pct).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(i, _)| i);

    let cols = (cores.len() as f64).sqrt().ceil() as usize;
    let mut grid_rows: Vec<Element<'_, Message>> = Vec::new();
    let mut current_row: Vec<Element<'_, Message>> = Vec::new();

    for (i, core) in cores.iter().enumerate() {
        let util = core.util_pct.clamp(0.0, 100.0) as f32 / 100.0;
        let bg_color = util_to_color(util);
        let bg_alpha = 0.08 + util * 0.5;
        let is_busiest = busiest_idx == Some(i);

        // Core label: ★ for busiest core
        let label = if is_busiest {
            format!("★ C{i}")
        } else {
            format!("C{i}")
        };

        let label_color = if is_busiest { colors::EMBER } else { colors::TEPHRA };

        // Utilization text color by threshold
        let util_text_color = if core.util_pct >= 85.0 {
            colors::ERUPTION
        } else if core.util_pct >= 50.0 {
            colors::EMBER
        } else if core.util_pct >= 20.0 {
            colors::LAVA
        } else {
            colors::TEPHRA
        };

        let border_color = if is_busiest { colors::EMBER } else { colors::SCORIA };
        let border_width = if is_busiest { 2.0 } else { 1.0 };

        let cell = container(
            column![
                text(label).size(10).color(label_color),
                text(format!("{}", core.freq_mhz))
                    .size(14)
                    .color(colors::PUMICE),
                text(format!("{:.0}%", core.util_pct))
                    .size(10)
                    .color(util_text_color),
            ]
            .spacing(2)
            .align_x(iced::Alignment::Center)
            .padding(6),
        )
        .width(Length::Fill)
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(colors::with_alpha(bg_color, bg_alpha).into()),
            border: iced::Border {
                color: border_color,
                width: border_width,
                radius: 4.0.into(),
            },
            ..Default::default()
        });

        current_row.push(cell.into());

        if current_row.len() == cols || i == cores.len() - 1 {
            while current_row.len() < cols {
                current_row.push(
                    container(text(""))
                        .width(Length::Fill)
                        .into(),
                );
            }
            let r: Vec<Element<'_, Message>> = current_row.drain(..).collect();
            grid_rows.push(row(r).spacing(4).into());
        }
    }

    column(grid_rows).spacing(4).into()
}

fn util_to_color(util: f32) -> iced::Color {
    if util <= 0.5 {
        let t = util / 0.5;
        lerp_color(colors::MINERAL, colors::EMBER, t)
    } else {
        let t = (util - 0.5) / 0.5;
        lerp_color(colors::EMBER, colors::ERUPTION, t)
    }
}

fn lerp_color(a: iced::Color, b: iced::Color, t: f32) -> iced::Color {
    iced::Color::from_rgba(
        a.r + (b.r - a.r) * t,
        a.g + (b.g - a.g) * t,
        a.b + (b.b - a.b) * t,
        1.0,
    )
}
