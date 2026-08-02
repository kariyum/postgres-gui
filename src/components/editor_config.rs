use crate::core::connection_config::ConnectionConfig;
use iced::{
    Element, Length, Theme, alignment,
    widget::{button, container, row, svg, text},
};

#[derive(Debug, Clone)]
pub struct EditorConfig {
    config: ConnectionConfig,
    database: String,
    // database_keeper:
    // query result
    // query filters
    // query state (idle, running, finished ...)
}

#[derive(Debug, Clone)]
pub enum Message {
    Select,
    Close,
}

impl EditorConfig {
    pub fn new(connection_config: ConnectionConfig) -> Self {
        Self {
            database: connection_config.database.clone(),
            config: connection_config,
        }
    }

    pub fn connection_string(&self) -> String {
        self.config.connection_string()
    }

    pub fn view_header(&self) -> Element<'_, Message> {
        let close_btn = button(
            svg(svg::Handle::from_memory(include_bytes!(
                "../resources/x.svg"
            )))
            .height(12)
            .width(12),
        )
        .on_press(Message::Close)
        .height(Length::Fit)
        .width(Length::Fit)
        .padding([2, 2])
        .style(|theme: &Theme, status| {
            let palette = theme.palette();
            button::Style {
                background: if matches!(status, button::Status::Hovered) {
                    Some(palette.background.weaker.color.into())
                } else {
                    None
                },
                ..Default::default()
            }
        });
        button(
            row![text(&self.database), close_btn]
                .align_y(alignment::Alignment::Center)
                .spacing(6),
        )
        .on_press(Message::Select)
        .padding([4, 6])
        .style(|_theme, _status| button::Style {
            border: iced::Border::default().width(0),
            ..button::background(_theme, _status)
        })
        .into()
    }

    pub fn view_editor(&self) -> Element<'_, Message> {
        container("Placeholder")
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }
}
