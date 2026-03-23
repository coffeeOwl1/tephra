use iced::widget::{button, column, container, row, scrollable, text, Space};
use iced::{Element, Length};

use crate::app::App;
use crate::message::Message;
use crate::theme::colors;
use crate::view::node_card::node_card;

/// The dashboard view: header + grid of node cards.
pub fn view(app: &App) -> Element<'_, Message> {
    // -- Header --
    let title = text("tephra")
        .size(28)
        .color(colors::EMBER);

    let node_count = text(format!(
        "{} node{}",
        app.nodes.len(),
        if app.nodes.len() == 1 { "" } else { "s" }
    ))
    .size(13)
    .color(colors::TEPHRA);

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

    let header = row![
        title,
        Space::new().width(12),
        node_count,
        Space::new().width(Length::Fill),
        compare_btn,
        Space::new().width(8),
        add_btn,
    ]
    .align_y(iced::Alignment::Center);

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
                    text("Click \"+ Add Node\" to connect to a tephra-server")
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
