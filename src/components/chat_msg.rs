use iced::widget::text;
use iced::widget::{container, markdown, row, space::horizontal};
use iced::{Background, Color, Element, Theme};
use serde::{Deserialize, Serialize};

use crate::core::agent_client::ChatMessage;

#[derive(Debug)]
pub struct ChatMsg {
    pub role: Role,
    pub content: String,
    pub markdown_content: markdown::Content,
}

impl Clone for ChatMsg {
    fn clone(&self) -> Self {
        Self {
            role: self.role.clone(),
            content: self.content.clone(),
            markdown_content: markdown::Content::parse(&self.content),
        }
    }
}

impl ChatMsg {
    pub fn new(role: Role, content: String) -> Self {
        Self {
            markdown_content: markdown::Content::parse(&content),
            role,
            content,
        }
    }

    pub fn view(&self) -> Element<'_, ChatMsgMessage> {
        let content = container(
            markdown::view(self.markdown_content.items(), Theme::CatppuccinMocha)
                .map(ChatMsgMessage::LinkClicked),
        )
        .style(|_theme| container::Style {
            background: Some(if let Role::Tool = self.role {
                Background::Color(Color::from_rgba(0.15, 0.15, 0.25, 0.4))
            } else {
                Background::Color(Color::TRANSPARENT)
            }),
            ..Default::default()
        })
        .padding([8.0, 12.0]);

        container(row![
            if let Role::User = self.role {
                horizontal()
            } else {
                iced::widget::Space::new()
            },
            content,
        ])
        .into()
    }
}

impl Into<ChatMessage> for ChatMsg {
    fn into(self) -> ChatMessage {
        ChatMessage {
            content: self.content,
            role: self.role,
        }
    }
}

#[derive(Clone, Debug)]
pub enum ChatMsgMessage {
    LinkClicked(markdown::Uri),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
    Thinking,
    System,
    Tool
}
