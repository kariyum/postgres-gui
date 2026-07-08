use iced::border::Radius;
use iced::futures::{Stream, StreamExt};
use iced::keyboard::key::{self, Named};
use iced::widget::space::horizontal;
use iced::widget::{button, column, container, row, rule, scrollable, svg, text, text_editor};
use iced::{Background, Border, Color, Element, Length, Task, Theme, keyboard};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::ai_config::AIConfig;
use crate::app::Message;
use crate::core::ai_client::{self, ChatMessage, ChatResponseChunk};

#[derive(Clone, Default, Debug)]
pub struct AIChat {
    visible: bool,
    input: text_editor::Content,
    error: Option<String>,
    messages: Vec<ChatMsg>,
    config: AIConfig,
    stream_id: Option<Uuid>,
}

#[derive(Clone, Debug)]
pub struct ChatMsg {
    pub role: Role,
    pub content: String,
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
pub enum ChatMsgMessage {}

impl ChatMsg {
    fn view(&self) -> Element<'_, ChatMsgMessage> {
        container(row![
            if let Role::User = self.role {
                horizontal()
            } else {
                iced::widget::Space::new()
            },
            container(text(self.content.to_string()))
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
}

impl AIChat {
    fn messages_view(&self) -> Element<'_, AIChatMessage> {
        let messages_col = column(
            self.messages
                .iter()
                .map(|msg| msg.view().map(AIChatMessage::MessageAction)),
        );
        scrollable(messages_col).height(Length::Fill).into()
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
        layout.into()
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
                    self.messages.push(ChatMsg {
                        role: Role::User,
                        content: self.input.text(),
                    });
                    self.input.perform(text_editor::Action::SelectAll);
                    self.input
                        .perform(text_editor::Action::Edit(text_editor::Edit::Delete));

                    let config = self.config.clone();
                    let messages: Vec<ChatMessage> =
                        self.messages.iter().map(|m| m.clone().into()).collect();

                    self.stream_id = Some(Uuid::new_v4());

                    Task::future(ai_client::prompt(config, messages)).then(|request_result| {
                        match request_result {
                            Ok(stream) => Task::run(stream, |chat_response_chunk| {
                                let message = match chat_response_chunk {
                                    Ok(chunk) => {
                                        if chunk.done {
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
                if let Some(last) = self.messages.last_mut() {
                    if let Role::Assistant = last.role {
                        last.content.push_str(&chunk.message.content);
                    } else {
                        self.messages.push(ChatMsg {
                            role: Role::Assistant,
                            content: chunk.message.content,
                        });
                    }
                } else {
                    self.messages.push(ChatMsg {
                        role: Role::Assistant,
                        content: chunk.message.content,
                    });
                }

                if chunk.done {
                    self.stream_id = None;
                }

                Task::none()
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
        }
    }

    pub fn streaming(&self) -> bool {
        self.stream_id.is_some()
    }

    pub fn set_config(&mut self, config: AIConfig) {
        self.config = config;
    }
}
