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
            iced::widget::Space::new().height(Length::Fixed(24.0)),
            column![
                text("📋  Add a connection  →  click  +  in the sidebar")
                    .size(14)
                    .color(theme::TEXT_MUTED),
                text("🔌  Connect  →  click  ▶  on a saved connection")
                    .size(14)
                    .color(theme::TEXT_MUTED),
                text("⌨   Run queries  →  type SQL & press  F5  or click  Run")
                    .size(14)
                    .color(theme::TEXT_MUTED),
            ]
            .spacing(8)
            .align_x(iced::Alignment::Start),
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
