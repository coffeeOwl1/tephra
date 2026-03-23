use iced::widget::{button, column, container, row, text, text_input, Space};
use iced::{Element, Length};

use crate::message::Message;
use crate::theme::colors;

/// Modal overlay for adding a node by IP:port.
pub fn add_node_dialog<'a>(input: &str, error: Option<&'a str>) -> Element<'a, Message> {
    let title = text("Add Tephra Server")
        .size(18)
        .color(colors::PUMICE);

    let hint = text("Enter IP address or IP:port (default port: 9867)")
        .size(12)
        .color(colors::TEPHRA);

    let input_field = text_input("192.168.1.100:9867", input)
        .on_input(Message::AddDialogInput)
        .on_submit(Message::AddDialogSubmit)
        .padding(10)
        .size(14);

    let cancel_btn = button(
        text("Cancel")
            .size(13)
            .color(colors::TEPHRA),
    )
    .on_press(Message::CloseAddDialog)
    .padding([8, 20])
    .style(|_theme: &iced::Theme, _status| button::Style {
        background: None,
        border: iced::Border {
            color: colors::SCORIA,
            width: 1.0,
            radius: 6.0.into(),
        },
        text_color: colors::TEPHRA,
        ..Default::default()
    });

    let connect_btn = button(
        text("Connect")
            .size(13)
            .color(colors::OBSIDIAN),
    )
    .on_press(Message::AddDialogSubmit)
    .padding([8, 20])
    .style(|_theme: &iced::Theme, status| {
        let bg = match status {
            button::Status::Hovered => colors::SANDSTONE,
            _ => colors::EMBER,
        };
        button::Style {
            background: Some(bg.into()),
            border: iced::Border {
                radius: 6.0.into(),
                ..Default::default()
            },
            text_color: colors::OBSIDIAN,
            ..Default::default()
        }
    });

    let buttons = row![cancel_btn, Space::new().width(Length::Fill), connect_btn]
        .align_y(iced::Alignment::Center);

    let mut dialog_col = column![title, hint, input_field].spacing(12);
    if let Some(err) = error {
        dialog_col = dialog_col.push(
            text(err.to_string())
                .size(12)
                .color(colors::MAGMA),
        );
    }
    dialog_col = dialog_col.push(buttons);

    let dialog = container(
        dialog_col
            .padding(24)
            .width(400),
    )
    .style(|_theme: &iced::Theme| container::Style {
        background: Some(colors::BASALT.into()),
        border: iced::Border {
            color: colors::SCORIA,
            width: 1.0,
            radius: 12.0.into(),
        },
        ..Default::default()
    });

    // Center the dialog in the window with a dark backdrop
    container(
        container(dialog)
            .width(Length::Shrink)
            .center_x(Length::Fill),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .center_y(Length::Fill)
    .style(|_theme: &iced::Theme| container::Style {
        background: Some(iced::Color::from_rgba(0.0, 0.0, 0.0, 0.6).into()),
        ..Default::default()
    })
    .into()
}
