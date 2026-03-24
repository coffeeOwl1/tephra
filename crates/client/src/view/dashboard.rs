use iced::widget::{button, column, container, row, scrollable, text, Space};
use iced::{Element, Length};

use crate::app::App;
use crate::message::Message;
use crate::theme::colors;
use crate::view::node_card::node_card;

/// Shared global header used across all views.
/// `show_compare` controls whether the Compare button is visible.
pub fn global_header(app: &App, show_compare: bool) -> Element<'_, Message> {
    let title = button(
        text("tephra")
            .size(28)
            .color(colors::EMBER),
    )
    .on_press(Message::NavigateDashboard)
    .padding([0, 0])
    .style(|_theme: &iced::Theme, _status| button::Style {
        background: None,
        ..Default::default()
    });

    let node_count = text(format!(
        "{} node{}",
        app.nodes.len(),
        if app.nodes.len() == 1 { "" } else { "s" }
    ))
    .size(13)
    .color(colors::TEPHRA);

    let mut header = row![
        title,
        Space::new().width(12),
        node_count,
        Space::new().width(Length::Fill),
    ]
    .align_y(iced::Alignment::Center);

    if show_compare {
        let compare_btn = button(
            text("Compare")
                .size(13)
                .color(colors::MINERAL),
        )
        .on_press(Message::NavigateCompare)
        .padding([6, 16])
        .style(|_theme: &iced::Theme, status| {
            let border_color = match status {
                button::Status::Hovered => colors::MINERAL,
                _ => colors::SCORIA,
            };
            button::Style {
                background: None,
                border: iced::Border {
                    color: border_color,
                    width: 1.0,
                    radius: 6.0.into(),
                },
                text_color: colors::MINERAL,
                ..Default::default()
            }
        });
        header = header.push(compare_btn);
        header = header.push(Space::new().width(8));
    }

    let settings_btn = button(
        text("\u{2699}").size(16).color(colors::TEPHRA),
    )
    .on_press(Message::NavigateSettings)
    .padding([6, 12])
    .style(|_theme: &iced::Theme, status| {
        let border_color = match status {
            button::Status::Hovered => colors::TEPHRA,
            _ => colors::SCORIA,
        };
        button::Style {
            background: None,
            border: iced::Border {
                color: border_color,
                width: 1.0,
                radius: 6.0.into(),
            },
            text_color: colors::TEPHRA,
            ..Default::default()
        }
    });
    header = header.push(settings_btn);
    header = header.push(Space::new().width(8));

    let on_dashboard = matches!(app.current_view, crate::app::View::Dashboard);
    if on_dashboard {
        // Invisible placeholder so buttons don't shift
        header = header.push(Space::new().width(36));
    } else {
        let close_btn = button(
            text("\u{2715}").size(14).color(colors::TEPHRA),
        )
        .on_press(Message::NavigateDashboard)
        .padding([6, 12])
        .style(|_theme: &iced::Theme, status| {
            let border_color = match status {
                button::Status::Hovered => colors::TEPHRA,
                _ => colors::SCORIA,
            };
            button::Style {
                background: None,
                border: iced::Border {
                    color: border_color,
                    width: 1.0,
                    radius: 6.0.into(),
                },
                text_color: colors::TEPHRA,
                ..Default::default()
            }
        });
        header = header.push(close_btn);
    }

    header.into()
}

/// The dashboard view: header + grid of node cards.
pub fn view(app: &App) -> Element<'_, Message> {
    let header = global_header(app, true);

    // -- Node cards grid --
    // Use a simple responsive column layout:
    // arrange cards in rows of 2-3 depending on count
    let cards: Vec<Element<'_, Message>> = app
        .node_order
        .iter()
        .filter_map(|id| app.nodes.get(id))
        .map(|node| node_card(node))
        .collect();

    let grid = build_grid(cards);

    // -- Empty state --
    let content = if app.nodes.is_empty() {
        column![
            header,
            Space::new().height(120),
            container(
                column![
                    text("No nodes connected")
                        .size(18)
                        .color(colors::TEPHRA),
                    Space::new().height(8),
                    text("Open Settings (\u{2699}) to add a node")
                        .size(13)
                        .color(colors::TEPHRA),
                ]
                .align_x(iced::Alignment::Center),
            )
            .width(Length::Fill)
            .align_x(iced::alignment::Horizontal::Center),
        ]
        .spacing(16)
    } else {
        column![header, Space::new().height(16), grid].spacing(0)
    };

    scrollable(content.padding(24)).into()
}

/// Arrange card elements in a wrapping flow layout.
/// Cards are 300px wide — rows wrap naturally with spacing.
fn build_grid(cards: Vec<Element<'_, Message>>) -> Element<'_, Message> {
    if cards.is_empty() {
        return column![].into();
    }

    // Use iced's wrap layout: just put all cards in a single row
    // that wraps. Since iced doesn't have a native wrap widget,
    // use rows of 3 (fits in 1200px window with padding).
    let cols = 3;
    let mut rows = column![].spacing(12);
    let mut current_row: Vec<Element<'_, Message>> = Vec::new();

    for card in cards {
        current_row.push(card);
        if current_row.len() == cols {
            let r: Vec<Element<'_, Message>> = current_row.drain(..).collect();
            rows = rows.push(row(r).spacing(12));
        }
    }

    if !current_row.is_empty() {
        let r: Vec<Element<'_, Message>> = current_row.drain(..).collect();
        rows = rows.push(row(r).spacing(12));
    }

    rows.into()
}
