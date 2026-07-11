use iced::border::Radius;
use iced::futures::{Stream, StreamExt};
use iced::keyboard::key::{self, Named};
use iced::widget::operation;
use iced::widget::space::horizontal;
use iced::widget::{
    button, column, container, markdown, row, rule, scrollable, svg, text, text_editor,
};
use iced::{Background, Border, Color, Element, Length, Task, Theme, keyboard};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::ai_config::AIConfig;
use crate::app::Message;
use crate::core::agent_tools::ToolManager;
use crate::core::ai_client::{self, ChatMessage, ChatResponseChunk, ChatResponseMessage};

#[derive(Clone, Debug)]
pub struct AIChat {
    visible: bool,
    input: text_editor::Content,
    error: Option<String>,
    messages: Vec<ChatMsg>,
    config: AIConfig,
    stream_id: Option<Uuid>,
    auto_scroll: bool,
    tool_manager: ToolManager,
}

#[derive(Debug)]
pub struct ChatMsg {
    pub role: Role,
    pub content: String,
    markdown_content: markdown::Content,
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

impl ChatMsg {
    fn view(&self) -> Element<'_, ChatMsgMessage> {
        container(row![
            if let Role::User = self.role {
                horizontal()
            } else {
                iced::widget::Space::new()
            },
            container(
                markdown::view(self.markdown_content.items(), Theme::CatppuccinMocha)
                    
                    .map(ChatMsgMessage::LinkClicked)
                    
            )
            .style(|_theme| container::Style {
                background: Some(Background::Color(Color::TRANSPARENT)),
                ..Default::default()
            })
            .padding([8.0, 12.0])
        ])
        .into()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
    Thinking,
    System,
}

#[derive(Clone, Debug)]
pub enum AIChatMessage {
    TogglePanel,
    EditorAction(text_editor::Action),
    Send,
    MessageAction(ChatMsgMessage),
    ChunkReceived(ChatResponseChunk),
    StreamError(String),
    StreamFinished,
    UserScrolled(scrollable::Viewport),
}

impl Default for AIChat {
    fn default() -> Self {
        Self {
            visible: false,
            input: text_editor::Content::default(),
            error: None,
            messages: Vec::new(),
            config: AIConfig::default(),
            stream_id: None,
            auto_scroll: true,
            tool_manager: ToolManager::without_db(),
        }
    }
}

impl AIChat {
    fn messages_view(&self) -> Element<'_, AIChatMessage> {
        let messages_col = column(
            self.messages
                .iter()
                .map(|msg| msg.view().map(AIChatMessage::MessageAction)),
        );
        scrollable(messages_col)
            .id("chat_messages")
            .on_scroll(AIChatMessage::UserScrolled)
            .height(Length::Fill)
            .into()
    }

    fn actions_view(&self) -> Element<'_, AIChatMessage> {
        container(row![
            horizontal(),
            button(
                svg(svg::Handle::from_memory(include_bytes!(
                    "../resources/send.svg"
                )))
                .height(14)
                .width(14)
            )
            .on_press(AIChatMessage::Send)
            .style(|_theme, _status| button::Style {
                background: Some(iced::Background::Color(Color::TRANSPARENT)),
                ..Default::default()
            })
        ])
        .style(|_theme: &Theme| container::Style {
            background: Some(Background::Color(
                _theme.extended_palette().background.weakest.color,
            )),
            ..Default::default()
        })
        .into()
    }

    fn editor_view(&self) -> Element<'_, AIChatMessage> {
        text_editor(&self.input)
            .placeholder("How many active users do I have?")
            .on_action(AIChatMessage::EditorAction)
            .id("ai_editor")
            .key_binding(|event| match (&event.key, &event.modifiers) {
                (&keyboard::Key::Named(key::Named::Enter), &keyboard::Modifiers::SHIFT) => {
                    text_editor::Binding::from_key_press(text_editor::KeyPress {
                        modifiers: keyboard::Modifiers::NONE,
                        ..event.clone()
                    })
                }
                (&keyboard::Key::Named(key::Named::Enter), _) => {
                    Some(text_editor::Binding::Custom(AIChatMessage::Send))
                }
                _ => text_editor::Binding::from_key_press(event),
            })
            .style(|_theme: &Theme, _status| text_editor::Style {
                background: Background::Color(_theme.extended_palette().background.weakest.color),
                border: Border {
                    color: Color::TRANSPARENT,
                    radius: Radius::new(0),
                    width: 0.0,
                },
                ..text_editor::default(_theme, _status)
            })
            .min_height(80)
            .max_height(200)
            .into()
    }

    pub fn view(&self) -> Element<'_, AIChatMessage> {
        let layout = column![
            container(text("AI Chat").size(14)).padding([4.0, 8.0]),
            rule::horizontal(1.0),
            self.messages_view(),
            rule::horizontal(1.0),
            self.editor_view(),
            self.actions_view()
        ];
        container(layout)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    pub fn update(&mut self, message: AIChatMessage) -> Task<Message> {
        match message {
            AIChatMessage::TogglePanel => {
                self.visible = !self.visible;
                Task::none()
            }
            AIChatMessage::EditorAction(action) => {
                self.input.perform(action);
                Task::none()
            }
            AIChatMessage::Send => {
                if !self.input.text().is_empty() && self.stream_id.is_none() {
                    self.messages
                        .push(ChatMsg::new(Role::User, self.input.text()));
                    self.input.perform(text_editor::Action::SelectAll);
                    self.input
                        .perform(text_editor::Action::Edit(text_editor::Edit::Delete));

                    let config = self.config.clone();
                    let messages: Vec<ChatMessage> =
                        self.messages.iter().map(|m| m.clone().into()).collect();
                    let tm = self.tool_manager.clone();

                    self.stream_id = Some(Uuid::new_v4());

                    Task::future(ai_client::prompt(config, messages, tm)).then(|request_result| {
                        match request_result {
                            Ok(stream) => Task::run(stream, |chat_response_chunk| {
                                let message = match chat_response_chunk {
                                    Ok(chunk) => {
                                        if let ChatResponseChunk::Done = chunk {
                                            Message::AIChat(AIChatMessage::StreamFinished)
                                        } else {
                                            Message::AIChat(AIChatMessage::ChunkReceived(chunk))
                                        }
                                    }
                                    Err(err) => {
                                        Message::AIChat(AIChatMessage::StreamError(err.to_string()))
                                    }
                                };
                                message
                            }),
                            Err(err) => Task::done(Message::AIChat(AIChatMessage::StreamError(
                                err.to_string(),
                            ))),
                        }
                    })
                } else {
                    Task::none()
                }
            }
            AIChatMessage::MessageAction(_) => Task::none(),
            AIChatMessage::ChunkReceived(chunk) => {
                match chunk {
                    ChatResponseChunk::Message(msg) => match msg {
                        ai_client::ChatResponseMessage::Content(delta) => {
                            if let Some(last) = self.messages.last_mut()
                                && let Role::Assistant = last.role
                            {
                                last.content.push_str(&delta);
                                last.markdown_content.push_str(&delta);
                            } else {
                                self.messages.push(ChatMsg::new(Role::Assistant, delta));
                            }
                        }
                        ai_client::ChatResponseMessage::Thinking(delta) => {
                            if let Some(last) = self.messages.last_mut()
                                && let Role::Thinking = last.role
                            {
                                last.markdown_content.push_str(&delta);
                            } else {
                                self.messages.push(ChatMsg::new(Role::Thinking, delta));
                            }
                        }
                    },

                    ChatResponseChunk::Done => {
                        self.stream_id = None;
                    }

                    ChatResponseChunk::ToolCallStarted { call_id, tool_name } => {
                        eprintln!("[pgeru] Tool call started: {tool_name} ({call_id})");
                    }
                    ChatResponseChunk::ToolCallDelta { call_id, args_delta } => {
                        eprintln!("[pgeru] Tool call delta for {call_id}: {args_delta}");
                    }
                    ChatResponseChunk::ToolCallComplete {
                        call_id,
                        tool_name,
                        args,
                    } => {
                        eprintln!("[pgeru] Tool call complete: {tool_name} ({call_id}) args={args}");
                    }
                }

                if self.auto_scroll {
                    operation::snap_to_end(iced::widget::Id::new("chat_messages"))
                } else {
                    Task::none()
                }
            }
            AIChatMessage::StreamError(err) => {
                self.error = Some(err);
                self.stream_id = None;
                Task::none()
            }
            AIChatMessage::StreamFinished => {
                self.stream_id = None;
                Task::none()
            }
            AIChatMessage::UserScrolled(viewport) => {
                let offset = viewport.absolute_offset();
                let content = viewport.content_bounds();
                let visible = viewport.bounds();
                let distance_from_bottom = content.height - visible.height - offset.y;
                self.auto_scroll = distance_from_bottom < 50.0;
                Task::none()
            }
        }
    }

    pub fn streaming(&self) -> bool {
        self.stream_id.is_some()
    }

    pub fn set_config(&mut self, config: AIConfig) {
        self.config = config;
    }

    pub fn set_tool_manager(&mut self, tm: ToolManager) {
        self.tool_manager = tm;
    }
}
