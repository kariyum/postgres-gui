use iced::widget::{column, container, text};
use iced::{Element, Length};

use crate::theme;

pub fn view<Message: 'static>() -> Element<'static, Message> {
    container(
        column![
            text("pgeru").size(48).font(iced::Font {
                weight: iced::font::Weight::Bold,
                ..iced::Font::DEFAULT
            }),
            text("PostgreSQL client").size(18).color(theme::TEXT_MUTED),
        ]
        .spacing(6)
        .align_x(iced::Alignment::Center),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .align_x(iced::Alignment::Center)
    .align_y(iced::Alignment::Center)
    .into()
}
