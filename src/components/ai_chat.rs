use std::collections::HashMap;

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
use crate::core::agent_tools::{ToolManager, needs_approval};
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
    pending_tool_calls: HashMap<String, (String, String)>,
    tool_call_entries: Vec<ToolCallEntry>,
}

#[derive(Clone, Debug)]
struct ToolCallEntry {
    call_id: String,
    tool_name: String,
    args: String,
    result: Option<String>,
    error: Option<String>,
    status: ToolCallStatus,
}

#[derive(Clone, Debug)]
enum ToolCallStatus {
    PendingApproval,
    Running,
    Completed,
    Failed,
    Rejected,
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

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
    Thinking,
    System,
    Tool,
}

impl ToolCallEntry {
    fn icon(&self) -> &'static str {
        match self.tool_name.as_str() {
            "execute_sql" => "\u{1F5C4}\u{FE0F}",
            "list_schemas" | "list_tables" => "\u{1F4CB}",
            "describe_table" => "\u{1F50D}",
            "explain_query" => "\u{1F4CA}",
            "show_table_stats" => "\u{1F4C8}",
            _ => "\u{1F527}",
        }
    }

    fn status_label(&self) -> &'static str {
        match &self.status {
            ToolCallStatus::PendingApproval => "\u{26A0}\u{FE0F} Needs approval",
            ToolCallStatus::Running => "\u{23F3} Running...",
            ToolCallStatus::Completed => "\u{2705} Done",
            ToolCallStatus::Failed => "\u{274C} Failed",
            ToolCallStatus::Rejected => "\u{1F6AB} Rejected",
        }
    }

    fn view(&self) -> Element<'_, AIChatMessage> {
        let mut children: Vec<Element<'_, AIChatMessage>> = vec![
            row![
                text(format!("{} {}", self.icon(), self.tool_name)).size(13),
                horizontal(),
                text(self.status_label()).size(11).color(Color::from_rgba(
                    0.7, 0.7, 0.9, 1.0,
                )),
            ]
            .spacing(8)
            .into(),
            container(text(&self.args).size(11))
                .padding([4, 6])
                .style(|_: &Theme| container::Style {
                    background: Some(Background::Color(Color::from_rgba(
                        0.0, 0.0, 0.0, 0.2,
                    ))),
                    border: Border {
                        color: Color::from_rgba(0.5, 0.5, 0.8, 0.2),
                        width: 1.0,
                        radius: Radius::new(4.0),
                    },
                    ..Default::default()
                })
                .into(),
        ];

        if let Some(result) = &self.result {
            children.push(
                container(text(result).size(11))
                    .padding([4, 6])
                    .style(|_: &Theme| container::Style {
                        background: Some(Background::Color(Color::from_rgba(
                            0.0, 0.2, 0.0, 0.15,
                        ))),
                        border: Border {
                            color: Color::from_rgba(0.3, 0.8, 0.3, 0.3),
                            width: 1.0,
                            radius: Radius::new(4.0),
                        },
                        ..Default::default()
                    })
                    .into(),
            );
        }

        if let Some(error) = &self.error {
            children.push(
                container(text(error).size(11).color(Color::from_rgb(1.0, 0.3, 0.3)))
                    .padding([4, 6])
                    .style(|_: &Theme| container::Style {
                        background: Some(Background::Color(Color::from_rgba(
                            0.3, 0.0, 0.0, 0.15,
                        ))),
                        border: Border {
                            color: Color::from_rgba(1.0, 0.3, 0.3, 0.3),
                            width: 1.0,
                            radius: Radius::new(4.0),
                        },
                        ..Default::default()
                    })
                    .into(),
            );
        }

        if let ToolCallStatus::PendingApproval = &self.status {
            children.push(
                row![
                    button(
                        text("Approve")
                            .size(12)
                            .color(Color::from_rgb(0.2, 0.8, 0.2))
                    )
                    .on_press(AIChatMessage::ApproveToolCall(self.call_id.clone()))
                    .style(|_theme, _status| button::Style {
                        background: Some(Background::Color(Color::from_rgba(
                            0.0, 0.3, 0.0, 0.3,
                        ))),
                        border: Border {
                            color: Color::from_rgba(0.2, 0.8, 0.2, 0.5),
                            width: 1.0,
                            radius: Radius::new(4.0),
                        },
                        ..Default::default()
                    }),
                    button(
                        text("Reject")
                            .size(12)
                            .color(Color::from_rgb(1.0, 0.3, 0.3))
                    )
                    .on_press(AIChatMessage::RejectToolCall(self.call_id.clone()))
                    .style(|_theme, _status| button::Style {
                        background: Some(Background::Color(Color::from_rgba(
                            0.3, 0.0, 0.0, 0.3,
                        ))),
                        border: Border {
                            color: Color::from_rgba(1.0, 0.3, 0.3, 0.5),
                            width: 1.0,
                            radius: Radius::new(4.0),
                        },
                        ..Default::default()
                    }),
                ]
                .spacing(8)
                .into(),
            );
        }

        container(column(children).spacing(4))
            .padding(8)
            .style(|_: &Theme| container::Style {
                background: Some(Background::Color(Color::from_rgba(
                    0.15, 0.15, 0.25, 0.6,
                ))),
                border: Border {
                    color: Color::from_rgba(0.5, 0.5, 0.9, 0.3),
                    width: 1.0,
                    radius: Radius::new(6.0),
                },
                ..Default::default()
            })
            .max_width(500)
            .into()
    }
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
    ApproveToolCall(String),
    RejectToolCall(String),
    ToolExecutionResult {
        call_id: String,
        result: Result<String, String>,
    },
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
            pending_tool_calls: HashMap::new(),
            tool_call_entries: Vec::new(),
        }
    }
}

impl AIChat {
    fn messages_view(&self) -> Element<'_, AIChatMessage> {
        let msg_els: Vec<Element<'_, AIChatMessage>> = self
            .messages
            .iter()
            .map(|msg| msg.view().map(AIChatMessage::MessageAction))
            .collect();

        let tool_els: Vec<Element<'_, AIChatMessage>> = self
            .tool_call_entries
            .iter()
            .map(|entry| entry.view())
            .collect();

        let all: Vec<Element<'_, AIChatMessage>> =
            msg_els.into_iter().chain(tool_els).collect();

        scrollable(column(all))
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
                    let input = self.input.text();
                    eprintln!("[pgeru:ai] Send: input_len={}, stream_id=None", input.len());

                    self.messages.push(ChatMsg::new(Role::User, input));
                    self.input.perform(text_editor::Action::SelectAll);
                    self.input
                        .perform(text_editor::Action::Edit(text_editor::Edit::Delete));

                    let prev_entries = self.tool_call_entries.len();
                    self.tool_call_entries.clear();
                    self.pending_tool_calls.clear();
                    eprintln!(
                        "[pgeru:ai] Send: cleared {} tool_call_entries and pending_tool_calls",
                        prev_entries
                    );

                    let config = self.config.clone();
                    let messages: Vec<ChatMessage> =
                        self.messages.iter().map(|m| m.clone().into()).collect();
                    let tm = self.tool_manager.clone();

                    eprintln!(
                        "[pgeru:ai] Send: {} messages, model={}",
                        messages.len(),
                        config.model,
                    );

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
                    eprintln!(
                        "[pgeru:ai] Send: skipped (input_empty={}, stream_id_is_some={})",
                        self.input.text().is_empty(),
                        self.stream_id.is_some()
                    );
                    Task::none()
                }
            }
            AIChatMessage::MessageAction(_) => Task::none(),
            AIChatMessage::ChunkReceived(chunk) => {
                let mut task = Task::none();

                match chunk {
                    ChatResponseChunk::Message(msg) => {
                        // Flush pending tool calls before text/thinking content:
                        // the model has stopped calling tools and is now speaking.
                        task = task.chain(self.flush_pending_tool_calls());

                        match msg {
                            ai_client::ChatResponseMessage::Content(delta) => {
                                if let Some(last) = self.messages.last_mut()
                                    && let Role::Assistant = last.role
                                {
                                    let prev = last.content.len();
                                    last.content.push_str(&delta);
                                    last.markdown_content.push_str(&delta);
                                    eprintln!(
                                        "[pgeru:ai] chunk Content: delta_len={}, total_len={}",
                                        delta.len(),
                                        last.content.len()
                                    );
                                } else {
                                    eprintln!(
                                        "[pgeru:ai] chunk Content (new msg): delta_len={}",
                                        delta.len()
                                    );
                                    self.messages.push(ChatMsg::new(Role::Assistant, delta));
                                }
                            }
                            ai_client::ChatResponseMessage::Thinking(delta) => {
                                if let Some(last) = self.messages.last_mut()
                                    && let Role::Thinking = last.role
                                {
                                    last.markdown_content.push_str(&delta);
                                } else {
                                    eprintln!(
                                        "[pgeru:ai] chunk Thinking: delta_len={}",
                                        delta.len()
                                    );
                                    self.messages.push(ChatMsg::new(Role::Thinking, delta));
                                }
                            }
                        }
                    }

                    ChatResponseChunk::ToolCallStarted {
                        call_id,
                        tool_name,
                        initial_args,
                    } => {
                        eprintln!(
                            "[pgeru:ai] ToolCallStarted: call_id={}, tool_name={}, initial_args_len={}",
                            call_id,
                            tool_name,
                            initial_args.len()
                        );
                        self.pending_tool_calls
                            .insert(call_id, (tool_name, initial_args));
                    }
                    ChatResponseChunk::ToolCallDelta { call_id, args_delta } => {
                        if let Some((_, args)) = self.pending_tool_calls.get_mut(&call_id) {
                            let prev = args.len();
                            args.push_str(&args_delta);
                            eprintln!(
                                "[pgeru:ai] ToolCallDelta: call_id={}, delta_len={}, total_len={}",
                                call_id,
                                args_delta.len(),
                                args.len()
                            );
                        } else {
                            eprintln!(
                                "[pgeru:ai] ToolCallDelta: call_id={} NOT FOUND in pending",
                                call_id
                            );
                        }
                    }
                    ChatResponseChunk::ToolCallComplete {
                        call_id,
                        tool_name,
                        args,
                    } => {
                        self.pending_tool_calls.remove(&call_id);

                        let needs_approval = needs_approval(&tool_name, &args);
                        eprintln!(
                            "[pgeru:ai] ToolCallComplete: call_id={}, tool_name={}, args_len={}, needs_approval={}",
                            call_id,
                            tool_name,
                            args.len(),
                            needs_approval,
                        );
                        let status = if needs_approval {
                            ToolCallStatus::PendingApproval
                        } else {
                            ToolCallStatus::Running
                        };

                        self.tool_call_entries.push(ToolCallEntry {
                            call_id: call_id.clone(),
                            tool_name: tool_name.clone(),
                            args: args.clone(),
                            result: None,
                            error: None,
                            status,
                        });

                        if !needs_approval {
                            eprintln!(
                                "[pgeru:ai] ToolCallComplete: auto-executing {} (call_id={})",
                                tool_name, call_id
                            );
                            let tm = self.tool_manager.clone();
                            task = Task::perform(
                                async move { tm.execute(&tool_name, &args).await },
                                move |result| {
                                    Message::AIChat(AIChatMessage::ToolExecutionResult {
                                        call_id,
                                        result: result.map_err(|e| e.0),
                                    })
                                },
                            );
                        } else {
                            eprintln!(
                                "[pgeru:ai] ToolCallComplete: needs approval for {} (call_id={})",
                                tool_name, call_id
                            );
                        }
                    }
                    ChatResponseChunk::Done => {
                        eprintln!("[pgeru:ai] chunk Done (unexpected in ChunkReceived)");
                    }
                }

                if self.auto_scroll {
                    task = task.chain(operation::snap_to_end(iced::widget::Id::new(
                        "chat_messages",
                    )));
                }

                task
            }
            AIChatMessage::ApproveToolCall(call_id) => {
                eprintln!("[pgeru:ai] ApproveToolCall: call_id={}", call_id);
                if let Some(entry) = self
                    .tool_call_entries
                    .iter_mut()
                    .find(|e| e.call_id == call_id)
                {
                    entry.status = ToolCallStatus::Running;
                    let tool_name = entry.tool_name.clone();
                    let args = entry.args.clone();
                    let tm = self.tool_manager.clone();
                    eprintln!(
                        "[pgeru:ai] ApproveToolCall: executing {} (call_id={})",
                        tool_name, call_id
                    );
                    Task::perform(
                        async move { tm.execute(&tool_name, &args).await },
                        move |result| {
                            Message::AIChat(AIChatMessage::ToolExecutionResult {
                                call_id,
                                result: result.map_err(|e| e.0),
                            })
                        },
                    )
                } else {
                    eprintln!(
                        "[pgeru:ai] ApproveToolCall: call_id={} NOT FOUND in entries",
                        call_id
                    );
                    Task::none()
                }
            }
            AIChatMessage::RejectToolCall(call_id) => {
                eprintln!("[pgeru:ai] RejectToolCall: call_id={}", call_id);
                if let Some(entry) = self
                    .tool_call_entries
                    .iter_mut()
                    .find(|e| e.call_id == call_id)
                {
                    entry.status = ToolCallStatus::Rejected;
                    eprintln!(
                        "[pgeru:ai] RejectToolCall: rejected {} (call_id={})",
                        entry.tool_name, call_id
                    );
                } else {
                    eprintln!(
                        "[pgeru:ai] RejectToolCall: call_id={} NOT FOUND in entries",
                        call_id
                    );
                }
                self.maybe_re_prompt()
            }
            AIChatMessage::ToolExecutionResult { call_id, result } => {
                match &result {
                    Ok(data) => eprintln!(
                        "[pgeru:ai] ToolExecutionResult: call_id={}, ok, data_len={}",
                        call_id,
                        data.len()
                    ),
                    Err(err) => eprintln!(
                        "[pgeru:ai] ToolExecutionResult: call_id={}, error={}",
                        call_id, err
                    ),
                }
                if let Some(entry) = self
                    .tool_call_entries
                    .iter_mut()
                    .find(|e| e.call_id == call_id)
                {
                    match result {
                        Ok(data) => {
                            entry.result = Some(data);
                            entry.status = ToolCallStatus::Completed;
                        }
                        Err(err) => {
                            entry.error = Some(err);
                            entry.status = ToolCallStatus::Failed;
                        }
                    }
                } else {
                    eprintln!(
                        "[pgeru:ai] ToolExecutionResult: call_id={} NOT FOUND in entries",
                        call_id
                    );
                }
                self.maybe_re_prompt()
            }
            AIChatMessage::StreamError(err) => {
                eprintln!("[pgeru:ai] StreamError: {}", err);
                self.error = Some(err);
                self.stream_id = None;
                Task::none()
            }
            AIChatMessage::StreamFinished => {
                eprintln!("[pgeru:ai] StreamFinished");
                self.stream_id = None;
                let flush = self.flush_pending_tool_calls();
                self.maybe_re_prompt().chain(flush)
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

    fn all_tool_calls_complete(&self) -> bool {
        if self.tool_call_entries.is_empty() {
            return false;
        }
        self.tool_call_entries.iter().all(|e| {
            matches!(
                e.status,
                ToolCallStatus::Completed | ToolCallStatus::Failed | ToolCallStatus::Rejected
            )
        }        )
    }

    /// Drain all entries from `pending_tool_calls` and turn them into
    /// `ToolCallEntry` items. Non-destructive calls are auto-executed.
    fn flush_pending_tool_calls(&mut self) -> Task<Message> {
        if self.pending_tool_calls.is_empty() {
            return Task::none();
        }

        let count = self.pending_tool_calls.len();
        eprintln!("[pgeru:ai] flush_pending_tool_calls: flushing {} pending call(s)", count);

        let pending: HashMap<String, (String, String)> =
            self.pending_tool_calls.drain().collect();
        let mut exec_tasks: Vec<Task<Message>> = Vec::new();

        for (call_id, (tool_name, args)) in pending {
            if args.is_empty() {
                eprintln!(
                    "[pgeru:ai] flush_pending: skipping {tool_name} ({call_id}) with empty args"
                );
                continue;
            }

            let needs_approval = needs_approval(&tool_name, &args);
            let status = if needs_approval {
                ToolCallStatus::PendingApproval
            } else {
                ToolCallStatus::Running
            };

            eprintln!(
                "[pgeru:ai] flush_pending: {} ({call_id}) needs_approval={}",
                tool_name, needs_approval
            );

            self.tool_call_entries.push(ToolCallEntry {
                call_id: call_id.clone(),
                tool_name: tool_name.clone(),
                args: args.clone(),
                result: None,
                error: None,
                status,
            });

            if !needs_approval {
                let tm = self.tool_manager.clone();
                exec_tasks.push(Task::perform(
                    async move { tm.execute(&tool_name, &args).await },
                    move |result| {
                        Message::AIChat(AIChatMessage::ToolExecutionResult {
                            call_id,
                            result: result.map_err(|e| e.0),
                        })
                    },
                ));
            }
        }

        let mut combined = Task::none();
        for t in exec_tasks {
            combined = combined.chain(t);
        }
        combined
    }

    fn maybe_re_prompt(&mut self) -> Task<Message> {
        if self.stream_id.is_some() {
            eprintln!("[pgeru:ai] maybe_re_prompt: skipped (stream active)");
            return Task::none();
        }
        if !self.all_tool_calls_complete() {
            let pending: Vec<&str> = self
                .tool_call_entries
                .iter()
                .filter(|e| {
                    !matches!(
                        e.status,
                        ToolCallStatus::Completed
                            | ToolCallStatus::Failed
                            | ToolCallStatus::Rejected
                    )
                })
                .map(|e| e.tool_name.as_str())
                .collect();
            eprintln!(
                "[pgeru:ai] maybe_re_prompt: skipped (pending tool calls: {:?})",
                pending
            );
            return Task::none();
        }

        let entry_count = self.tool_call_entries.len();
        eprintln!(
            "[pgeru:ai] maybe_re_prompt: injecting {} tool result(s) and re-prompting",
            entry_count
        );

        for entry in &self.tool_call_entries {
            let content = match &entry.result {
                Some(r) => {
                    format!(
                        "Tool '{}' was called with args: {}\n\nResult:\n{}",
                        entry.tool_name, entry.args, r
                    )
                }
                None => match &entry.error {
                    Some(e) => {
                        format!(
                            "Tool '{}' was called with args: {}\n\nError:\n{}",
                            entry.tool_name, entry.args, e
                        )
                    }
                    None => {
                        format!(
                            "Tool '{}' was called with args: {}\n\nThe call was rejected by the user.",
                            entry.tool_name, entry.args
                        )
                    }
                },
            };
            eprintln!(
                "[pgeru:ai] maybe_re_prompt: injecting tool msg for {} (len={})",
                entry.tool_name,
                content.len()
            );
            self.messages.push(ChatMsg::new(Role::Tool, content));
        }

        self.tool_call_entries.clear();

        let config = self.config.clone();
        let messages: Vec<ChatMessage> =
            self.messages.iter().map(|m| m.clone().into()).collect();
        let tm = self.tool_manager.clone();

        eprintln!(
            "[pgeru:ai] maybe_re_prompt: starting new stream with {} messages",
            messages.len()
        );

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
    }
}
