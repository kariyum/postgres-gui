use iced::widget::{column, container, markdown, row, space::horizontal, text};
use iced::widget::{space, text_editor};
use iced::{Background, Color, Element, Theme};
use rig_core::providers::openai::ToolCall;
use serde::{Deserialize, Serialize};

use crate::components::tool_call_entry::{self, ToolCallEntry};
use crate::core::agent_client::ChatMessage;

#[derive(Debug)]
pub struct Content {
    pub role: Role,
    pub content: String,
    pub markdown_content: markdown::Content,
}

#[derive(Debug)]
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

    fn view_tool<'a>(&'a self, tool: &'a ToolCallEntry) -> Element<'a, ChatMsgMessage> {
        let error: Element<ChatMsgMessage> = if let Some(ref err) = tool.error {
            text(err).into()
        } else {
            space().into()
        };

        let result: Element<ChatMsgMessage> = if let Some(ref result) = tool.result {
            text(result).into()
        } else {
            space().into()
        };

        let args: Element<ChatMsgMessage> = if let Ok(ref tool_details) = tool.tool_details {
            match tool_details.args {
                tool_call_entry::ToolArgs::ConnectToDatabase(ref args) => {
                    text(format!("Connect to {}", args.database_name)).into()
                }
                tool_call_entry::ToolArgs::DescribeTable(ref args) => text(format!(
                    "@{} describe table {}.{}",
                    args.database_name, args.schema, args.table
                ))
                .into(),
                tool_call_entry::ToolArgs::ExecuteSQL(ref args) => column![
                    text(format!("Execute SQL on {}", args.database_name,)),
                    text(args.sql.as_str())
                ]
                .into(),
                tool_call_entry::ToolArgs::ExplainQuery(ref args) => {
                    text(format!("Explaning query {}", args.sql)).into()
                }
                tool_call_entry::ToolArgs::ListConnections(_) => text("List Connections").into(),
                tool_call_entry::ToolArgs::ListSchemas(ref args) => {
                    text(format!("List Schemas on {}", args.database_name)).into()
                }
                tool_call_entry::ToolArgs::ListTables(ref args) => text(format!(
                    "List Tables {}.{}",
                    args.database_name, args.schema
                ))
                .into(),
                tool_call_entry::ToolArgs::ShowTableStats(ref args) => {
                    text(format!("Show Table Stats {}", args.table)).into()
                }
            }
        } else {
            space().into()
        };

        container(column![args, error, result,])
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
