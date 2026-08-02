use iced::{
    Element, Length, Theme,
    widget::{button, column, container, text},
};
use serde::{Deserialize, Serialize};
use tracing::info;
use uuid::Uuid;

use crate::theme;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionConfig {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: String,
    pub database: String,
}

#[derive(Clone, Debug)]
pub enum Message {
    Connect(ConnectionConfig),
    Edit(ConnectionConfig),
    Duplicate(ConnectionConfig),
    Delete(ConnectionConfig),
}

impl ConnectionConfig {
    pub fn new(
        name: String,
        host: String,
        port: u16,
        user: String,
        password: String,
        database: String,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name,
            host,
            port,
            user,
            password,
            database,
        }
    }

    pub fn connection_string(&self) -> String {
        format!(
            "postgres://{}:{}@{}:{}/{}",
            self.user, self.password, self.host, self.port, self.database
        )
    }

    fn view_context_menu(&self) -> Element<'_, Message> {
        container(column![
            context_button("Connect", Message::Connect(self.clone())),
            context_button("Edit", Message::Edit(self.clone())),
            context_button("Duplicate", Message::Duplicate(self.clone())),
            context_button("Delete", Message::Delete(self.clone())),
        ])
        .width(150)
        .into()
    }

    pub fn view(&self) -> Element<'_, Message> {
        iced_aw::ContextMenu::new(
            button(
                column![
                    text(&self.name).size(14).font(iced::Font {
                        weight: iced::font::Weight::Bold,
                        ..iced::Font::DEFAULT
                    }),
                    text(format!("{}/{}", self.user, self.database))
                        .size(11)
                        .color(theme::TEXT_MUTED)
                ]
                .spacing(2),
            )
            .on_press(Message::Connect(self.clone()))
            .padding([8, 12])
            .width(Length::Fill)
            .style(|theme: &Theme, status| {
                let palette = theme.palette();
                button::Style {
                    background: Some(if matches!(status, button::Status::Hovered) {
                        palette.background.weak.color.into()
                    } else {
                        palette.background.base.color.into()
                    }),
                    border: iced::Border::default()
                        .rounded(4)
                        .color(palette.background.weak.color)
                        .width(1.0),
                    text_color: palette.background.base.text,
                    ..Default::default()
                }
            }),
            || self.view_context_menu(),
        )
        .into()
    }
}

fn context_button(
    title: &str,
    msg: Message,
) -> button::Button<'_, Message, iced::Theme, iced::Renderer> {
    button(text(title).size(13))
        .padding([6, 12])
        .on_press(msg)
        .width(Length::Fill)
        .style(|_theme, _status| button::Style {
            ..button::subtle(_theme, _status)
        })
}

impl Default for ConnectionConfig {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name: String::from("Connection Name"),
            host: String::from("localhost"),
            port: 5432,
            user: String::from("postgres"),
            password: String::new(),
            database: String::from("postgres"),
        }
    }
}
