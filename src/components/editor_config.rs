use crate::components::connection_config::ConnectionConfig;
use crate::db;
use crate::types::QueryResult;
use iced::{
    Background, Color, Element, Length, Task, Theme,
    alignment::{self, Horizontal::Left},
    theme,
    widget::{Column, button, column, container, pane_grid, row, rule, svg, text, text_editor},
};

#[derive(Debug, Clone)]
pub struct EditorConfig {
    config: ConnectionConfig,
    database: String,
    editor: text_editor::Content,
    panes: pane_grid::State<PaneKind>,
    editor_pane: pane_grid::Pane,
    table_pane: Option<pane_grid::Pane>,
    pool: Option<sqlx::PgPool>,
    result: Option<QueryResult>,
    error: Option<String>,
    running: bool,
    // database_keeper:
    // query filters
    // query state (idle, running, finished ...)
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
    EditorAction(text_editor::Action),
    ToolbarActions(ToolbarActions),
    Resized(pane_grid::ResizeEvent),
    QueryCompleted {
        pool: Option<sqlx::PgPool>,
        result: Result<QueryResult, String>,
    },
}

#[derive(Debug, Clone)]
pub enum ToolbarActions {
    RunQuery,
}

impl EditorConfig {
    pub fn new(connection_config: ConnectionConfig) -> Self {
        let (pane, editor_pane) = pane_grid::State::new(PaneKind::Editor);
        Self {
            database: connection_config.database.clone(),
            config: connection_config,
            editor: text_editor::Content::new(),
            panes: pane,
            editor_pane,
            table_pane: None,
            pool: None,
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
                self.editor.perform(action);
                Task::none()
            }
            Message::ToolbarActions(toolbar_actions) => {
                tracing::info!("Got toolbar action {:?}", toolbar_actions);
                self.perform_toolbar_action(toolbar_actions)
            }
            Message::Resized(resize_event) => {
                self.panes.resize(resize_event.split, resize_event.ratio);
                Task::none()
            }
            Message::QueryCompleted { pool, result } => {
                self.pool = pool;
                self.running = false;
                match result {
                    Ok(query_result) => self.result = Some(query_result),
                    Err(err) => self.error = Some(err),
                }
                Task::none()
            }
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

                let connection_string = self.config.connection_string();
                let sql = self.editor.text();
                let pool = self.pool.clone();

                Task::perform(
                    async move {
                        let pool = match pool {
                            Some(pool) => pool,
                            None => match db::connect(&connection_string).await {
                                Ok(pool) => pool,
                                Err(err) => return (None, Err(err)),
                            },
                        };

                        let result = db::execute_query(&pool, &sql).await;
                        (Some(pool), result)
                    },
                    |(pool, result)| Message::QueryCompleted { pool, result },
                )
            }
        }
    }

    pub fn connection_string(&self) -> String {
        self.config.connection_string()
    }

    pub fn view_header(&self) -> Element<'_, Message> {
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
        .style(|_theme, _status| button::Style {
            border: iced::Border::default().width(0),
            ..button::background(_theme, _status)
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
            svg(svg::Handle::from_memory(include_bytes!(
                "../resources/play.svg"
            )))
            .height(12)
            .width(12),
        )
        .on_press(Message::ToolbarActions(ToolbarActions::RunQuery));

        container(
            row![
                run_btn,
                iced::widget::Space::new().width(Length::Fill),
                text(self.connection_string())
                    .size(11)
                    .color(crate::theme::TEXT_MUTED),
            ]
            .align_y(iced::Alignment::Center),
        )
        .padding([6, 4])
        .into()
    }

    fn view_query_editor(&self) -> Element<'_, Message> {
        container(
            text_editor(&self.editor)
                .on_action(Message::EditorAction)
                .highlight("sql", iced::highlighter::Theme::Base16Eighties)
                .height(Length::FillPortion(1))
                .font(iced::Font::MONOSPACE)
                .size(14)
                .style(|_theme, _status| text_editor::Style {
                    background: Background::Color(Color::TRANSPARENT),
                    border: iced::Border {
                        color: Color::TRANSPARENT,
                        width: 0.0,
                        radius: iced::border::Radius::new(0),
                    },
                    ..text_editor::default(_theme, _status)
                }),
        )
        .style(|theme: &Theme| {
            let palette = theme.palette();
            iced::widget::container::Style {
                background: Some(palette.background.base.color.into()),
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
        let content: Element<'_, Message> = match (&self.result, &self.error, self.running) {
            (Some(result), _, _) if result.columns.is_empty() => container(
                text(&result.message)
                    .size(13)
                    .color(crate::theme::SUCCESS),
            )
            .padding(16)
            .into(),
            (Some(result), _, _) => {
                let columns = result
                    .columns
                    .iter()
                    .map(|column| column.name.clone())
                    .collect::<Vec<_>>();
                let rows = result
                    .rows
                    .iter()
                    .map(|row| row.cells.clone())
                    .collect::<Vec<_>>();

                crate::widgets::table::Table::new(columns, rows).into()
            }
            (None, Some(error), _) => {
                container(text(error).size(13).color(crate::theme::DANGER)).padding(16).into()
            }
            (None, None, true) => container(
                text("Running query...")
                    .size(13)
                    .color(crate::theme::TEXT_MUTED),
            )
            .padding(16)
            .into(),
            (None, None, false) => container(
                text("Run a query to see results here.")
                    .size(13)
                    .color(crate::theme::TEXT_MUTED),
            )
            .padding(16)
            .into(),
        };

        container(column![rule::horizontal(1), content])
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }
}
