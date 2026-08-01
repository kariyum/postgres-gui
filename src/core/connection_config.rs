use iced::{
    Color, Element, Length,
    widget::{button, column, container, text},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

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
    EditRequested(ConnectionConfig),
    DuplicateRequested(ConnectionConfig),
    DeleteRequested(ConnectionConfig),
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
            context_button("Edit", Message::EditRequested(self.clone())),
            context_button("Duplicate", Message::DuplicateRequested(self.clone())),
            context_button("Delete", Message::DeleteRequested(self.clone())),
        ])
        .width(150)
        .into()
    }

    pub fn view(&self) -> Element<'_, Message> {
        iced_aw::ContextMenu::new(
            button(column![
                text(&self.name),
                text(self.connection_string()).size(12)
            ])
            .on_press(Message::Connect(self.clone())),
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
            border: iced::Border {
                radius: 0.0.into(),
                width: 0.0,
                color: Color::TRANSPARENT,
            },
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
