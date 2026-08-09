use iced::widget::{container, markdown, row, space::horizontal, text};
use iced::{Background, Color, Element, Theme};
use serde::{Deserialize, Serialize};

use crate::core::agent_client::ChatMessage;

#[derive(Debug)]
pub struct Content {
    pub role: Role,
    pub content: String,
    pub markdown_content: markdown::Content,
}

#[derive(Debug)]
pub struct Tool {
    pub args: String,
    pub tool_name: String,
    pub result: String,
}

#[derive(Debug)]
pub enum ChatMsg {
    Content(Content),
    Tool(Tool),
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
            ChatMsg::Tool(Tool {
                args,
                tool_name,
                result,
            }) => ChatMsg::Tool(Tool {
                args: args.clone(),
                tool_name: tool_name.clone(),
                result: result.clone(),
            }),
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

    pub fn new_tool(args: String, tool_name: String, result: String) -> ChatMsg {
        ChatMsg::Tool(Tool {
            args,
            tool_name,
            result,
        })
    }

    pub fn view(&self) -> Element<'_, ChatMsgMessage> {
        match self {
            ChatMsg::Content(content) => self.view_content(content),
            ChatMsg::Tool(tool) => self.view_tool(tool),
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

    fn view_tool<'a>(&'a self, tool: &'a Tool) -> Element<'a, ChatMsgMessage> {
        container(text(tool.tool_name.as_str()))
            .padding([8, 12])
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
                    "Tool '{}' was called with args: {}\n\nResult:\n{}",
                    tool.tool_name, tool.args, tool.result
                ),
                role: Role::Tool,
            },
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
    Tool,
}
