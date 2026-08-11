use iced::widget::{column, container, markdown, row, space::horizontal, text};
use iced::widget::{space, text_editor};
use iced::{Background, Color, Element, Theme};
use serde::{Deserialize, Serialize};

use crate::components::tool_call_entry::{self, ToolCallEntry};
use crate::core::agent_client::ChatMessage;

#[derive(Debug)]
pub struct Content {
    pub role: Role,
    pub content: String,
    pub markdown_content: markdown::Content,
}

impl Serialize for Content {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("Content", 2)?;
        state.serialize_field("role", &self.role)?;
        state.serialize_field("content", &self.content)?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for Content {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct ContentHelper {
            role: Role,
            content: String,
        }

        let helper = ContentHelper::deserialize(deserializer)?;
        Ok(Content {
            role: helper.role,
            content: helper.content.clone(),
            markdown_content: markdown::Content::parse(&helper.content),
        })
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub enum ChatMsg {
    Content(Content),
    Tool(ToolCallEntry),
}

impl Clone for ChatMsg {
    fn clone(&self) -> Self {
        match self {
            ChatMsg::Content(Content {
                role,
                content,
                markdown_content: _,
            }) => ChatMsg::Content(Content {
                role: role.clone(),
                content: content.clone(),
                markdown_content: markdown::Content::parse(&content),
            }),
            ChatMsg::Tool(tool_call_entry) => ChatMsg::Tool(tool_call_entry.clone()),
        }
    }
}

impl ChatMsg {
    pub fn new_content(role: Role, content: String) -> ChatMsg {
        ChatMsg::Content(Content {
            markdown_content: markdown::Content::parse(&content),
            role,
            content,
        })
    }

    pub fn new_tool(tool_call_entry: ToolCallEntry) -> ChatMsg {
        ChatMsg::Tool(tool_call_entry)
    }

    pub fn view(&self) -> Element<'_, ChatMsgMessage> {
        match self {
            ChatMsg::Content(content) => self.view_content(content),
            ChatMsg::Tool(tool) => tool.view().map(|_| ChatMsgMessage::ToolAction).into(),
        }
    }

    fn view_content<'a>(&'a self, content: &'a Content) -> Element<'a, ChatMsgMessage> {
        let body = container(
            markdown::view(content.markdown_content.items(), Theme::CatppuccinMocha)
                .map(ChatMsgMessage::LinkClicked),
        )
        .style(|_theme| container::Style {
            background: Some(if let Role::Tool = content.role {
                Background::Color(Color::from_rgba(0.15, 0.15, 0.25, 0.4))
            } else {
                Background::Color(Color::TRANSPARENT)
            }),
            ..Default::default()
        })
        .padding([8.0, 12.0]);

        container(row![
            if let Role::User = content.role {
                horizontal()
            } else {
                iced::widget::Space::new()
            },
            body,
        ])
        .into()
    }
}

impl Into<ChatMessage> for ChatMsg {
    fn into(self) -> ChatMessage {
        match self {
            ChatMsg::Content(content) => ChatMessage {
                content: content.content,
                role: content.role,
            },
            ChatMsg::Tool(tool) => ChatMessage {
                content: format!(
                    "Tool '{}' was called with args: {}\n\nResult:\n{:?} Error: \n{:?}",
                    tool.tool_name, tool.args, tool.result, tool.error
                ),
                role: Role::Tool,
            },
        }
    }
}

#[derive(Clone, Debug)]
pub enum ChatMsgMessage {
    LinkClicked(markdown::Uri),
    ToolAction,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
    Thinking,
    System,
    Tool,
}
