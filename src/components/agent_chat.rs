use iced::futures::Stream;
use std::collections::HashMap;
use std::{format, matches};
use tracing::error;
use tracing::info;
use uuid::Uuid;

use iced::keyboard::key::{self};
use iced::widget::operation;
use iced::widget::space::{self, horizontal};
use iced::widget::{
    button, column, container, pick_list, row, rule, scrollable, svg, text, text_editor,
};
use iced::{Background, Border, Color, Element, Length, Task, Theme, keyboard};

use crate::components::chat_msg::{ChatMsg, ChatMsgMessage, Role};
use crate::components::tool_call_entry::{ToolCallEntry, ToolCallStatus};
use crate::core::agent_client::{self, ChatMessage, ChatResponseChunk};
use crate::core::agent_tools::{Tools, needs_approval};
use crate::core::configured_provider::ConfiguredProvider;
use crate::core::connection_config::ConnectionConfig;
use crate::core::database_keeper::DatabaseKeeper;

#[derive(Clone, Debug)]
pub enum AgentChatMessage {
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
    ModelSelected(String),
    ModelChanged(ConfiguredProvider),
    ResetChat,
}

#[derive(Clone, Debug)]
pub struct AgentChat {
    visible: bool,
    input: text_editor::Content,
    error: Option<String>,
    messages: Vec<ChatMsg>,
    config: ConfiguredProvider,
    stream_id: Option<Uuid>,
    auto_scroll: bool,
    tool_manager: Tools,
    pending_tool_calls: HashMap<String, (String, String)>,
    tool_call_entries: Vec<ToolCallEntry>,
    chosen_model: Option<String>,
}

impl AgentChat {
    pub fn new(
        config: ConfiguredProvider,
        configs: Vec<ConnectionConfig>,
        pools: HashMap<String, sqlx::PgPool>,
    ) -> Self {
        let chosen_model = config.default_model.clone();
        let (tx, rx) = tokio::sync::mpsc::channel(1000);
        let mut actor = DatabaseKeeper::new(configs, pools, rx);
        tokio::spawn(async move { actor.run().await });
        Self {
            visible: false,
            input: text_editor::Content::default(),
            error: None,
            messages: Vec::new(),
            config,
            stream_id: None,
            auto_scroll: true,
            tool_manager: Tools::new(tx),
            pending_tool_calls: HashMap::new(),
            tool_call_entries: Vec::new(),
            chosen_model,
        }
    }

    fn messages_view(&self) -> Element<'_, AgentChatMessage> {
        let msg_els: Vec<Element<'_, AgentChatMessage>> = self
            .messages
            .iter()
            .map(|msg| msg.view().map(AgentChatMessage::MessageAction))
            .collect();

        let tool_els: Vec<Element<'_, AgentChatMessage>> = self
            .tool_call_entries
            .iter()
            .map(|entry| entry.view())
            .collect();

        let all: Vec<Element<'_, AgentChatMessage>> = msg_els.into_iter().chain(tool_els).collect();

        scrollable(column(all))
            .id("chat_messages")
            .on_scroll(AgentChatMessage::UserScrolled)
            .height(Length::Fill)
            .into()
    }

    fn actions_view(&self) -> Element<'_, AgentChatMessage> {
        let default_model = self
            .chosen_model
            .clone()
            .or_else(|| self.config.default_model.clone());

        let model_picker: Element<'_, AgentChatMessage> = pick_list(
            default_model,
            self.config.available_models.clone(),
            |s: &String| s.clone(),
        )
        .on_select(AgentChatMessage::ModelSelected)
        .placeholder("Select model")
        .text_size(12)
        .menu_height(150.0)
        .width(Length::Shrink)
        .style(|theme: &iced::Theme, status| {
            let palette = theme.palette();
            let bg = match status {
                pick_list::Status::Hovered | pick_list::Status::Opened { .. } => {
                    iced::Background::Color(palette.background.weak.color)
                }
                pick_list::Status::Active | pick_list::Status::Disabled => {
                    iced::Background::Color(palette.background.weakest.color)
                }
            };
            pick_list::Style {
                text_color: palette.background.weak.text,
                placeholder_color: palette.secondary.base.color,
                handle_color: palette.background.weak.text,
                background: bg,
                border: iced::Border {
                    radius: 0.0.into(),
                    width: 0.0,
                    color: iced::Color::TRANSPARENT,
                },
            }
        })
        .into();

        container(row![
            horizontal(),
            model_picker,
            button(
                svg(svg::Handle::from_memory(include_bytes!(
                    "../resources/rotate.svg"
                )))
                .height(14)
                .width(14)
            )
            .on_press(AgentChatMessage::ResetChat)
            .style(|_theme, _status| button::Style {
                background: Some(iced::Background::Color(Color::TRANSPARENT)),
                ..Default::default()
            }),
            button(
                svg(svg::Handle::from_memory(include_bytes!(
                    "../resources/send.svg"
                )))
                .height(14)
                .width(14)
            )
            .on_press(AgentChatMessage::Send)
            .style(|_theme, _status| button::Style {
                background: Some(iced::Background::Color(Color::TRANSPARENT)),
                ..Default::default()
            })
        ])
        .style(|_theme: &Theme| container::Style {
            background: Some(Background::Color(
                _theme.palette().background.weak.color,
            )),
            ..Default::default()
        })
        .into()
    }

    fn editor_view(&self) -> Element<'_, AgentChatMessage> {
        text_editor(&self.input)
            .placeholder("How many active users do I have?")
            .on_action(AgentChatMessage::EditorAction)
            .id("ai_editor")
            .key_binding(|event| match (&event.key, &event.modifiers) {
                (&keyboard::Key::Named(key::Named::Enter), &keyboard::Modifiers::SHIFT) => {
                    text_editor::Binding::from_key_press(text_editor::KeyPress {
                        modifiers: keyboard::Modifiers::NONE,
                        ..event.clone()
                    })
                }
                (&keyboard::Key::Named(key::Named::Enter), _) => {
                    Some(text_editor::Binding::Custom(AgentChatMessage::Send))
                }
                _ => text_editor::Binding::from_key_press(event),
            })
            .style(|_theme: &Theme, _status| text_editor::Style {
                background: Background::Color(_theme.palette().background.weak.color),
                border: Border {
                    color: Color::TRANSPARENT,
                    radius: iced::border::Radius::new(0),
                    width: 0.0,
                },
                ..text_editor::default(_theme, _status)
            })
            .height(Length::Fixed(120.0))
            .into()
    }

    pub fn view(&self) -> Element<'_, AgentChatMessage> {
        let layout = column![
            container(text("AI Chat").size(14)).padding([4.0, 8.0]),
            rule::horizontal(1.0),
            self.messages_view(),
            self.error_view(),
            rule::horizontal(1.0),
            self.editor_view(),
            self.actions_view()
        ];
        container(layout)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    pub fn update(&mut self, message: AgentChatMessage) -> Task<AgentChatMessage> {
        match message {
            AgentChatMessage::TogglePanel => {
                self.visible = !self.visible;
                Task::none()
            }
            AgentChatMessage::EditorAction(action) => {
                self.input.perform(action);
                Task::none()
            }
            AgentChatMessage::Send => {
                operation::snap_to_end(iced::widget::Id::new("chat_messages")).chain(
                    if !self.input.text().is_empty()
                        && self.stream_id.is_none()
                        && let Some(model) = self
                            .chosen_model
                            .clone()
                            .or(self.config.default_model.clone())
                    {
                        let input = self.input.text();
                        info!("Send: input_len={}, stream_id=None", input.len());

                        self.messages.push(ChatMsg::new(Role::User, input));
                        self.input.perform(text_editor::Action::SelectAll);
                        self.input
                            .perform(text_editor::Action::Edit(text_editor::Edit::Delete));

                        self.tool_call_entries.clear();
                        self.pending_tool_calls.clear();

                        let messages: Vec<ChatMessage> =
                            self.messages.iter().map(|msg| msg.clone().into()).collect();
                        let tm = self.tool_manager.clone();

                        self.stream_id = Some(Uuid::new_v4());
                        self.error = None;

                        self.prompt_agent(messages, tm, model)
                    } else {
                        Task::none()
                    },
                )
            }
            AgentChatMessage::MessageAction(_) => Task::none(),
            AgentChatMessage::ChunkReceived(chunk) => {
                let mut task = Task::none();

                match chunk {
                    ChatResponseChunk::Message(msg) => {
                        // Flush pending tool calls before text/thinking content:
                        // the model has stopped calling tools and is now speaking.
                        task = task.chain(self.flush_pending_tool_calls());

                        match msg {
                            agent_client::ChatResponseMessage::Content(delta) => {
                                if let Some(last) = self.messages.last_mut()
                                    && let Role::Assistant = last.role
                                {
                                    let prev = last.content.len();
                                    last.content.push_str(&delta);
                                    last.markdown_content.push_str(&delta);
                                    info!(
                                        "chunk Content: delta_len={}, total_len={}",
                                        delta.len(),
                                        last.content.len()
                                    );
                                } else {
                                    info!("chunk Content (new msg): delta_len={}", delta.len());
                                    self.messages.push(ChatMsg::new(Role::Assistant, delta));
                                }
                            }
                            agent_client::ChatResponseMessage::Thinking(delta) => {
                                if let Some(last) = self.messages.last_mut()
                                    && let Role::Thinking = last.role
                                {
                                    last.markdown_content.push_str(&delta);
                                } else {
                                    info!("chunk Thinking: delta_len={}", delta.len());
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
                        info!(
                            "ToolCallStarted: call_id={}, tool_name={}, initial_args_len={}",
                            call_id,
                            tool_name,
                            initial_args.len()
                        );
                        self.pending_tool_calls
                            .insert(call_id, (tool_name, initial_args));
                    }
                    ChatResponseChunk::ToolCallDelta {
                        call_id,
                        args_delta,
                    } => {
                        if let Some((_, args)) = self.pending_tool_calls.get_mut(&call_id) {
                            let prev = args.len();
                            args.push_str(&args_delta);
                            info!(
                                "ToolCallDelta: call_id={}, delta_len={}, total_len={}",
                                call_id,
                                args_delta.len(),
                                args.len()
                            );
                        } else {
                            info!("ToolCallDelta: call_id={} NOT FOUND in pending", call_id);
                        }
                    }
                    ChatResponseChunk::ToolCallComplete {
                        call_id,
                        tool_name,
                        args,
                    } => {
                        self.pending_tool_calls.remove(&call_id);

                        let needs_approval = needs_approval(&tool_name, &args);
                        info!(
                            "ToolCallComplete: call_id={}, tool_name={}, args_len={}, needs_approval={}",
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
                            info!(
                                "ToolCallComplete: auto-executing {} (call_id={})",
                                tool_name, call_id
                            );
                            let tm = self.tool_manager.clone();
                            task = Task::perform(
                                async move { tm.execute(&tool_name, &args).await },
                                move |result| AgentChatMessage::ToolExecutionResult {
                                    call_id,
                                    result: result.map_err(|e| e.0),
                                },
                            );
                        } else {
                            info!(
                                "ToolCallComplete: needs approval for {} (call_id={})",
                                tool_name, call_id
                            );
                        }
                    }
                    ChatResponseChunk::Done => {
                        info!("chunk Done (unexpected in ChunkReceived)");
                    }
                }

                if self.auto_scroll {
                    task = task.chain(operation::snap_to_end(iced::widget::Id::new(
                        "chat_messages",
                    )));
                }

                task
            }
            AgentChatMessage::ApproveToolCall(call_id) => {
                info!("ApproveToolCall: call_id={}", call_id);
                if let Some(entry) = self
                    .tool_call_entries
                    .iter_mut()
                    .find(|e| e.call_id == call_id)
                {
                    entry.status = ToolCallStatus::Running;
                    let tool_name = entry.tool_name.clone();
                    let args = entry.args.clone();
                    let tm = self.tool_manager.clone();
                    info!(
                        "ApproveToolCall: executing {} (call_id={})",
                        tool_name, call_id
                    );
                    Task::perform(
                        async move { tm.execute(&tool_name, &args).await },
                        move |result| AgentChatMessage::ToolExecutionResult {
                            call_id,
                            result: result.map_err(|e| e.0),
                        },
                    )
                } else {
                    info!("ApproveToolCall: call_id={} NOT FOUND in entries", call_id);
                    Task::none()
                }
            }
            AgentChatMessage::RejectToolCall(call_id) => {
                info!("RejectToolCall: call_id={}", call_id);
                if let Some(entry) = self
                    .tool_call_entries
                    .iter_mut()
                    .find(|e| e.call_id == call_id)
                {
                    entry.status = ToolCallStatus::Rejected;
                    info!(
                        "RejectToolCall: rejected {} (call_id={})",
                        entry.tool_name, call_id
                    );
                } else {
                    info!("RejectToolCall: call_id={} NOT FOUND in entries", call_id);
                }
                self.maybe_re_prompt()
            }
            AgentChatMessage::ToolExecutionResult { call_id, result } => {
                match &result {
                    Ok(data) => info!(
                        "ToolExecutionResult: call_id={}, ok, data_len={}",
                        call_id,
                        data.len()
                    ),
                    Err(err) => info!("ToolExecutionResult: call_id={}, error={}", call_id, err),
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
                    info!(
                        "ToolExecutionResult: call_id={} NOT FOUND in entries",
                        call_id
                    );
                }
                self.maybe_re_prompt()
            }
            AgentChatMessage::StreamError(err) => {
                error!("StreamError: {}", err);
                self.error = Some(err);
                self.stream_id = None;
                Task::none()
            }
            AgentChatMessage::StreamFinished => {
                info!("StreamFinished");
                self.stream_id = None;
                let flush = self.flush_pending_tool_calls();
                self.maybe_re_prompt().chain(flush)
            }
            AgentChatMessage::UserScrolled(viewport) => {
                let offset = viewport.absolute_offset();
                let content = viewport.content_bounds();
                let visible = viewport.bounds();
                let distance_from_bottom = content.height - visible.height - offset.y;
                self.auto_scroll = distance_from_bottom < 50.0;
                Task::none()
            }
            AgentChatMessage::ModelSelected(model) => {
                self.chosen_model = Some(model.clone());
                self.config.default_model = Some(model);
                Task::done(AgentChatMessage::ModelChanged(self.config.clone()))
            }
            AgentChatMessage::ModelChanged(_) => Task::none(),
            AgentChatMessage::ResetChat => {
                if !self.streaming() {
                    self.messages.clear();
                    self.error = None;
                }
                Task::none()
            }
        }
    }

    fn prompt_agent(
        &mut self,
        messages: Vec<ChatMessage>,
        tm: Tools,
        model: String,
    ) -> Task<AgentChatMessage> {
        Task::future(agent_client::prompt(
            self.config.clone(),
            model,
            messages,
            tm,
        ))
        .then(|request_result| match request_result {
            Ok(stream) => consume_stream(stream),
            Err(err) => {
                info!("Request failed with {err}");
                Task::done(AgentChatMessage::StreamError(err.to_string()))
            }
        })
    }

    pub fn streaming(&self) -> bool {
        self.stream_id.is_some()
    }

    pub fn update_connections(
        &self,
        configs: Vec<ConnectionConfig>,
        pools: std::collections::HashMap<String, sqlx::PgPool>,
    ) {
        self.tool_manager.update_connections(configs, pools);
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
        })
    }

    /// Drain all entries from `pending_tool_calls` and turn them into
    /// `ToolCallEntry` items. Non-destructive calls are auto-executed.
    fn flush_pending_tool_calls(&mut self) -> Task<AgentChatMessage> {
        if self.pending_tool_calls.is_empty() {
            return Task::none();
        }

        let count = self.pending_tool_calls.len();
        info!(
            "flush_pending_tool_calls: flushing {} pending call(s)",
            count
        );

        let pending: HashMap<String, (String, String)> = self.pending_tool_calls.drain().collect();
        let mut exec_tasks: Vec<Task<AgentChatMessage>> = Vec::new();

        for (call_id, (tool_name, args)) in pending {
            if args.is_empty() {
                info!("flush_pending: skipping {tool_name} ({call_id}) with empty args");
                continue;
            }

            let needs_approval = needs_approval(&tool_name, &args);
            let status = if needs_approval {
                ToolCallStatus::PendingApproval
            } else {
                ToolCallStatus::Running
            };

            info!(
                "flush_pending: {} ({call_id}) needs_approval={}",
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
                    move |result| AgentChatMessage::ToolExecutionResult {
                        call_id,
                        result: result.map_err(|e| e.0),
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

    fn maybe_re_prompt(&mut self) -> Task<AgentChatMessage> {
        if self.stream_id.is_some() {
            info!("maybe_re_prompt: skipped (stream active)");
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
            info!(
                "maybe_re_prompt: skipped (pending tool calls: {:?})",
                pending
            );
            return Task::none();
        }

        let entry_count = self.tool_call_entries.len();
        info!(
            "maybe_re_prompt: injecting {} tool result(s) and re-prompting",
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
            info!(
                "maybe_re_prompt: injecting tool msg for {} (len={})",
                entry.tool_name,
                content.len()
            );
            self.messages.push(ChatMsg::new(Role::Tool, content));
        }

        self.tool_call_entries.clear();

        let messages: Vec<ChatMessage> = self.messages.iter().map(|m| m.clone().into()).collect();
        let tm = self.tool_manager.clone();

        info!(
            "maybe_re_prompt: starting new stream with {} messages",
            messages.len()
        );

        self.stream_id = Some(Uuid::new_v4());

        let model = self
            .chosen_model
            .clone()
            .or(self.config.default_model.clone());

        if let Some(model) = model {
            self.prompt_agent(messages, tm, model)
        } else {
            Task::none()
        }
    }

    fn error_view(&self) -> Element<'_, AgentChatMessage> {
        match &self.error {
            Some(err) => container(text(err).size(14)).padding([4, 8]).into(),
            None => iced::widget::space().into(),
        }
    }
}

fn consume_stream<S: Stream<Item = Result<ChatResponseChunk, anyhow::Error>> + Send + 'static>(
    stream: S,
) -> Task<AgentChatMessage> {
    Task::run(stream, |chat_response_chunk| match chat_response_chunk {
        Ok(ChatResponseChunk::Done) => AgentChatMessage::StreamFinished,
        Ok(chunk) => AgentChatMessage::ChunkReceived(chunk),
        Err(err) => AgentChatMessage::StreamError(err.to_string()),
    })
}
