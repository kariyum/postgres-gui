use std::{rc::Rc, sync::Arc};

use crate::core::agent_tools::{DatabaseKeeperMessage, ToolError};
use crate::db;
use crate::types::{QueryResult, Schema};
use crate::{components::connection_config::ConnectionConfig, widgets};

use anyhow::Context;
use iced::futures::SinkExt;
use iced::futures::channel::mpsc::Sender;
use iced::{Alignment, padding};
use iced::widget::{operation, space};
use iced::{
    Background, Color, Element, Length, Task, Theme,
    alignment::{self, Horizontal::Left},
    theme,
    widget::{Column, button, column, container, pane_grid, row, rule, svg, text, text_editor},
};
use tokio::sync::oneshot;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EditorId(Uuid);

impl EditorId {
    pub fn id(&self) -> String {
        self.0.to_string()
    }
}

#[derive(Debug, Clone)]
pub struct EditorConfig {
    id: EditorId,
    config: ConnectionConfig,
    database: String,
    editor: widgets::sql_editor::Content,
    schema: Option<Arc<Schema>>,
    panes: pane_grid::State<PaneKind>,
    editor_pane: pane_grid::Pane,
    table_pane: Option<pane_grid::Pane>,
    result: Option<Arc<QueryResult>>,
    error: Option<String>,
    running: bool,
    database_keeper: Sender<DatabaseKeeperMessage>,
}

#[derive(Debug, Clone)]
enum PaneKind {
    Editor,
    Table,
}

#[derive(Debug, Clone)]
pub enum Message {
    Select,
    Close,
    EditorAction(widgets::sql_editor::Action),
    SchemaReady(Arc<Schema>),
    Noop,
    ToolbarActions(ToolbarActions),
    Resized(pane_grid::ResizeEvent),
    QueryCompleted {
        result: Result<Arc<QueryResult>, String>,
    },
    PoolReady {
        pool: Result<sqlx::Pool<sqlx::Postgres>, crate::core::agent_tools::ToolError>,
        query: String,
    },
}

#[derive(Debug, Clone)]
pub enum ToolbarActions {
    RunQuery,
    FormatQuery,
}

impl EditorConfig {
    pub fn new(connection_config: ConnectionConfig, tx: Sender<DatabaseKeeperMessage>) -> Self {
        let (pane, editor_pane) = pane_grid::State::new(PaneKind::Editor);
        Self {
            id: EditorId(Uuid::new_v4()),
            database: connection_config.database.clone(),
            config: connection_config,
            editor: widgets::sql_editor::Content::new(),
            schema: None,
            panes: pane,
            editor_pane,
            database_keeper: tx,
            table_pane: None,
            result: None,
            error: None,
            running: false,
        }
    }

    pub fn update(&mut self, msg: Message) -> Task<Message> {
        match msg {
            Message::Select => Task::none(),
            Message::Close => Task::none(),
            Message::EditorAction(action) => {
                self.apply_editor_action(action);
                Task::none()
            }
            Message::SchemaReady(schema) => {
                self.schema = Some(schema);
                Task::none()
            }
            Message::Noop => Task::none(),
            Message::ToolbarActions(toolbar_actions) => {
                tracing::info!("Got toolbar action {:?}", toolbar_actions);
                self.perform_toolbar_action(toolbar_actions)
            }
            Message::Resized(resize_event) => {
                self.panes.resize(resize_event.split, resize_event.ratio);
                Task::none()
            }
            Message::QueryCompleted { result } => {
                self.running = false;
                match result {
                    Ok(query_result) => self.result = Some(query_result),
                    Err(err) => self.error = Some(err),
                }
                Task::none()
            }
            Message::PoolReady { pool, query } => match pool {
                Ok(pool) => Task::perform(
                    async move { db::execute_query(&pool, &query).await },
                    |result| Message::QueryCompleted {
                        result: result.map(|r| Arc::new(r)),
                    },
                ),
                Err(err) => {
                    self.error = Some(err.0);
                    Task::none()
                }
            },
        }
    }

    fn perform_toolbar_action(&mut self, msg: ToolbarActions) -> Task<Message> {
        match msg {
            ToolbarActions::RunQuery => {
                if self.table_pane.is_none() {
                    if let Some((pane, _split)) = self.panes.split(
                        pane_grid::Axis::Horizontal,
                        self.editor_pane,
                        PaneKind::Table,
                    ) {
                        self.table_pane = Some(pane);
                    }
                }

                self.running = true;
                self.result = None;
                self.error = None;

                let (tx, rx) = oneshot::channel();
                let mut database_keeper_tx = self.database_keeper.clone();
                let database = self.config.name.clone();
                let query = self.editor.text();
                Task::perform(
                    async move {
                        if let Err(err) = database_keeper_tx
                            .send(DatabaseKeeperMessage::GetPool {
                                database_name: database,
                                respond: tx,
                            })
                            .await
                        {
                            tracing::error!("Failed to GetPool {err}");
                            Err(ToolError(String::from("Failed to GetPool {err}")))
                        } else {
                            rx.await
                                .context("Oneshot channel was dropped")
                                .map_err(|err| ToolError(err.to_string()))
                                .flatten()
                        }
                    },
                    |pool| Message::PoolReady { pool, query },
                )
            }
            ToolbarActions::FormatQuery => {
                let current = self.editor.text();
                let formatted = crate::core::sql_format::format_sql(&current);
                if formatted != current {
                    self.editor.set_completion(None);
                    self.editor
                        .perform(widgets::sql_editor::Action::SelectAll);
                    self.editor
                        .perform(widgets::sql_editor::Action::Paste(formatted));
                }
                Task::none()
            }
        }
    }

    pub fn set_query(&mut self, query: &str) {
        self.editor = widgets::sql_editor::Content::with_text(query);
        self.editor.perform(widgets::sql_editor::Action::Move {
            motion: widgets::sql_editor::Motion::DocumentEnd,
            select: false,
        });
    }

    pub fn id(&self) -> EditorId {
        self.id
    }

    /// Applies an editor action, then opens/updates/closes the suggestion popup.
    fn apply_editor_action(&mut self, action: widgets::sql_editor::Action) {
        use widgets::sql_editor::Action as A;

        enum Popup {
            Keep,
            Dismiss,
            Refresh { force: bool },
        }

        let popup = match &action {
            A::Type(text) => {
                if text.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '.') {
                    Popup::Refresh { force: false }
                } else {
                    Popup::Dismiss
                }
            }
            A::Backspace | A::DeleteWord { .. } => {
                if self.editor.completion().is_some() {
                    Popup::Refresh { force: false }
                } else {
                    Popup::Keep
                }
            }
            A::RequestCompletion => Popup::Refresh { force: true },
            A::CompletionMove(_)
            | A::CompletionAccept
            | A::CompletionPick(_)
            | A::CompletionDismiss => Popup::Keep,
            _ => Popup::Dismiss,
        };

        self.editor.perform(action);

        match popup {
            Popup::Keep => {}
            Popup::Dismiss => self.editor.set_completion(None),
            Popup::Refresh { force } => {
                let completion =
                    crate::core::sql_completion::suggest(self.schema.as_deref(), &self.editor, force);
                self.editor.set_completion(completion);
            }
        }
    }

    /// Fetches the schema used for suggestions (connects if needed).
    pub fn load_schema(&self) -> Task<Message> {
        let mut keeper = self.database_keeper.clone();
        let name = self.config.name.clone();
        Task::perform(
            async move {
                crate::core::database_keeper::get_schema(&mut keeper, &name)
                    .await
                    .ok()
            },
            |schema| match schema {
                Some(schema) => Message::SchemaReady(Arc::new(schema)),
                None => Message::Noop,
            },
        )
    }

    /// Uses `schema` for suggestions if it belongs to this editor's connection.
    pub fn set_schema(&mut self, connection: &str, schema: Arc<Schema>) {
        if self.config.name == connection {
            self.schema = Some(schema);
        }
    }

    pub fn connection_string(&self) -> String {
        self.config.connection_string()
    }

    pub fn view_header(&self, is_focused: bool) -> Element<'_, Message> {
        let close_btn = button(
            svg(svg::Handle::from_memory(include_bytes!(
                "../resources/x.svg"
            )))
            .height(14)
            .width(14),
        )
        .on_press(Message::Close)
        .height(Length::Fit)
        .width(Length::Fit)
        .padding([2, 2])
        .style(|theme: &Theme, status| {
            let palette = theme.palette();
            button::Style {
                background: if matches!(status, button::Status::Hovered) {
                    Some(palette.background.weaker.color.into())
                } else {
                    None
                },
                ..Default::default()
            }
        });
        button(
            row![text(&self.database), close_btn]
                .align_y(alignment::Alignment::Center)
                .spacing(6),
        )
        .on_press(Message::Select)
        .padding([4, 6])
        .height(Length::Fill)
        .style(move |theme, status| button::Style {
            border: iced::Border::default().width(0),
            background: if matches!(status, button::Status::Active) && !is_focused {
                Some(theme.palette().background.weakest.color.into())
            } else {
                button::background(theme, status).background
            },
            ..button::background(theme, status)
        })
        .into()
    }

    pub fn view(&self) -> Element<'_, Message> {
        pane_grid::PaneGrid::new(&self.panes, |_pane, state, _is_maximized| match state {
            PaneKind::Editor => pane_grid::Content::new(self.view_editor()),
            PaneKind::Table => pane_grid::Content::new(self.view_table()),
        })
        .width(Length::Fill)
        .height(Length::Fill)
        .spacing(0)
        .on_resize(5, |event| Message::Resized(event))
        .into()
    }

    fn view_editor(&self) -> Element<'_, Message> {
        container(column![
            self.view_toolbar(),
            rule::horizontal(1),
            self.view_query_editor()
        ])
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    }

    fn view_toolbar(&self) -> Element<'_, Message> {
        let run_btn = button(
            row![
                svg(svg::Handle::from_memory(include_bytes!(
                    "../resources/play.svg"
                )))
                .height(12)
                .width(12),
                text("Run").size(12)
            ]
            .spacing(12)
            .align_y(Alignment::Center),
        )
        .on_press(Message::ToolbarActions(ToolbarActions::RunQuery))
        .style(button::subtle);

        let format_btn = button(
            row![
                svg(svg::Handle::from_memory(include_bytes!(
                    "../resources/format.svg"
                )))
                .height(12)
                .width(12),
                text("Format").size(12)
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        )
        .on_press(Message::ToolbarActions(ToolbarActions::FormatQuery))
        .style(button::subtle);

        container(
            row![run_btn, format_btn]
                .spacing(4)
                .align_y(iced::Alignment::Center),
        )
        .padding([6, 4])
        .into()
    }

    fn view_query_editor(&self) -> Element<'_, Message> {
        container(
            widgets::sql_editor::SqlEditor::new(&self.editor, Message::EditorAction)
                .height(Length::Fill)
                .font(iced::Font::MONOSPACE)
                .size(14)
                .id(self.id.0.to_string()),
        )
        .style(|theme: &Theme| {
            let palette = theme.palette();
            iced::widget::container::Style {
                background: Some(palette.background.weakest.color.into()),
                border: iced::Border {
                    color: Color::TRANSPARENT,
                    width: 0.0,
                    radius: iced::border::Radius::new(0),
                },
                text_color: Some(Color::from_rgb(1.0, 0.0, 0.0)),
                ..Default::default()
            }
        })
        .into()
    }

    fn view_table(&self) -> Element<'_, Message> {
        let content: Element<'_, Message> = if let Some(ref result) = self.result {
            if result.columns.is_empty() {
                container(text(&result.message).size(13).color(crate::theme::SUCCESS))
                    .padding(16)
                    .into()
            } else {
                widgets::table::Table::new(&result.columns, &result.rows).into()
            }
        } else if let Some(ref error) = self.error {
            container(text(error).size(13).color(crate::theme::DANGER))
                .padding(16)
                .into()
        } else if self.running {
            text("Running query...")
                .size(13)
                .style(text::secondary)
                .into()
        } else {
            container(
                text("Run a query to see results here.")
                    .size(13)
                    .style(text::secondary),
            )
            .padding(16)
            .into()
        };

        container(column![rule::horizontal(1), content])
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    pub fn focus(&self) -> Task<Message> {
        operation::focus(self.id().0.to_string())
    }
}
