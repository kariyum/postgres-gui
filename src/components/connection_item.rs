use std::fmt::Display;

use iced::widget::{
    Column, Row, button, column, container, row, scrollable, svg, text, text_editor,
};
use iced::{Background, Border, Color, Element, Length, Task, Theme, color};

use crate::components::schema_tree::{self, SchemaTree, TreeMessage};
use crate::core::connection_config::ConnectionConfig;
use crate::theme;
use crate::types::{QueryResult, TreeNode};

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum ConnectionStatus {
    Disconnected,
    Connecting,
    Connected,
    Error(String),
}

impl Display for ConnectionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConnectionStatus::Disconnected => write!(f, "Disconnected"),
            ConnectionStatus::Connecting => write!(f, "Connecting..."),
            ConnectionStatus::Connected => write!(f, "Connected"),
            ConnectionStatus::Error(str) => write!(f, "{}", str),
        }
    }
}

#[derive(Debug)]
pub struct ConnectionItem {
    pub cfg: ConnectionConfig,
    pub pool: Option<sqlx::PgPool>,
    pub editor: text_editor::Content,
    pub result: Option<QueryResult>,
    pub error: Option<String>,
    pub running: bool,
    pub schema_tree: SchemaTree,
    pub schema_loading: bool,
    pub connection_status: ConnectionStatus,
    pub actions_open: bool,
}

#[derive(Debug, Clone)]
pub enum ItemMessage {
    ConnectRequested,
    DisconnectRequested,
    EditRequested,
    DeleteRequested,
    DuplicateRequested,
    CopyStringRequested,
    RunQuery,
    Select,

    UpdateConfig(ConnectionConfig),

    EditorAction(text_editor::Action),
    SchemaTreeMessage(schema_tree::TreeMessage),
    SchemaLoaded(Result<Vec<TreeNode>, String>),
    QueryResult(Result<QueryResult, String>),
    ConnectSucceeded(sqlx::PgPool),
    ConnectFailed(String),
}

impl ConnectionItem {
    pub fn new(cfg: ConnectionConfig) -> Self {
        Self {
            editor: text_editor::Content::with_text("SELECT 1;"),
            pool: None,
            result: None,
            error: None,
            running: false,
            schema_tree: SchemaTree::new(Vec::new()),
            schema_loading: false,
            connection_status: ConnectionStatus::Disconnected,
            actions_open: false,
            cfg,
        }
    }
}

impl ConnectionItem {
    pub fn update(&mut self, message: ItemMessage) -> Task<ItemMessage> {
        match message {
            ItemMessage::ConnectRequested => {
                self.connection_status = ConnectionStatus::Connecting;
                Task::none()
            }
            ItemMessage::DisconnectRequested => {
                self.pool = None;
                self.schema_tree = SchemaTree::new(Vec::new());
                self.schema_loading = false;
                self.result = None;
                self.error = None;
                self.connection_status = ConnectionStatus::Disconnected;
                Task::none()
            }
            ItemMessage::RunQuery => {
                self.running = true;
                self.result = None;
                self.error = None;
                Task::none()
            }

            ItemMessage::UpdateConfig(cfg) => {
                self.cfg = cfg;
                Task::none()
            }

            ItemMessage::ConnectSucceeded(pool) => {
                self.pool = Some(pool);
                self.connection_status = ConnectionStatus::Connected;
                self.schema_loading = true;
                Task::none()
            }
            ItemMessage::ConnectFailed(err) => {
                let short = err[..err.len().min(80)].to_string();
                self.connection_status = ConnectionStatus::Error(format!("Error: {short}"));
                Task::none()
            }

            ItemMessage::SchemaTreeMessage(msg) => match msg {
                TreeMessage::SelectTable(schema, table) => {
                    let sql = format!("SELECT * FROM \"{schema}\".\"{table}\" LIMIT 100;");
                    self.editor = text_editor::Content::with_text(&sql);
                    self.schema_tree
                        .update(TreeMessage::SelectTable(schema, table))
                        .map(ItemMessage::SchemaTreeMessage)
                }
                _ => self
                    .schema_tree
                    .update(msg)
                    .map(ItemMessage::SchemaTreeMessage),
            },
            ItemMessage::SchemaLoaded(result) => {
                self.schema_loading = false;
                match result {
                    Ok(nodes) => self.schema_tree = SchemaTree::new(nodes),
                    Err(e) => self.error = Some(format!("Schema load error: {e}")),
                }
                Task::none()
            }

            // ── Query editor and results ─────────────────────────────────
            ItemMessage::EditorAction(action) => {
                self.editor.perform(action);
                Task::none()
            }
            ItemMessage::QueryResult(result) => {
                self.running = false;
                match result {
                    Ok(qr) => self.result = Some(qr),
                    Err(e) => self.error = Some(e),
                }
                Task::none()
            }

            ItemMessage::EditRequested
            | ItemMessage::DeleteRequested
            | ItemMessage::DuplicateRequested
            | ItemMessage::CopyStringRequested
            | ItemMessage::Select => Task::none(),
        }
    }
}

impl ConnectionItem {
    pub fn view(&self) -> Column<'_, ItemMessage> {
        let is_connected = self.pool.is_some();

        let conn_row = button(
            row![
                text(format!("/{} - {}", self.cfg.database, self.cfg.name)).size(13),
                iced::widget::Space::new().width(Length::Fill),
            ]
            .spacing(6),
        )
        .on_press(ItemMessage::Select)
        .width(Length::Fill)
        .style(move |theme: &Theme, status| {
            let palette = theme.extended_palette();
            if self.connection_status == ConnectionStatus::Connected {
                button::Style {
                    background: Some(color!(0x155c2b, 0.2).into()),
                    text_color: palette.primary.weak.text,
                    ..Default::default()
                }
            } else {
                match status {
                    button::Status::Hovered | button::Status::Pressed => button::Style {
                        background: Some(palette.background.strong.color.into()),
                        text_color: palette.background.base.text,
                        ..Default::default()
                    },
                    _ => button::Style {
                        background: None,
                        text_color: palette.background.base.text,
                        border: iced::Border::default(),
                        ..Default::default()
                    },
                }
            }
        });

        let context_menu = move || {
            let mut menu = column![];

            if is_connected {
                menu = menu.push(
                    button(text("Disconnect").size(13))
                        .on_press(ItemMessage::DisconnectRequested)
                        .padding([6, 12])
                        .width(Length::Fill)
                        .style(|_theme, _status| button::Style {
                            border: iced::Border {
                                radius: 0.0.into(),
                                width: 0.0,
                                color: Color::TRANSPARENT,
                            },
                            ..button::subtle(_theme, _status)
                        }),
                );
            } else {
                menu = menu.push(
                    button(text("Connect").size(13))
                        .on_press(ItemMessage::ConnectRequested)
                        .padding([6, 12])
                        .width(Length::Fill)
                        .style(|_theme, _status| button::Style {
                            border: iced::Border {
                                radius: 0.0.into(),
                                width: 0.0,
                                color: Color::TRANSPARENT,
                            },
                            ..button::subtle(_theme, _status)
                        }),
                );
            }

            menu = menu
                .push(
                    button(text("Edit").size(13))
                        .on_press(ItemMessage::EditRequested)
                        .padding([6, 12])
                        .width(Length::Fill)
                        .style(|_theme, _status| button::Style {
                            border: iced::Border {
                                radius: 0.0.into(),
                                width: 0.0,
                                color: Color::TRANSPARENT,
                            },
                            ..button::subtle(_theme, _status)
                        }),
                )
                .push(
                    button(text("Duplicate").size(13))
                        .on_press(ItemMessage::DuplicateRequested)
                        .padding([6, 12])
                        .width(Length::Fill)
                        .style(|_theme, _status| button::Style {
                            border: iced::Border {
                                radius: 0.0.into(),
                                width: 0.0,
                                color: Color::TRANSPARENT,
                            },
                            ..button::subtle(_theme, _status)
                        }),
                )
                .push(
                    button(text("Copy Connection String").size(13))
                        .on_press(ItemMessage::CopyStringRequested)
                        .padding([6, 12])
                        .width(Length::Fill)
                        .style(|_theme, _status| button::Style {
                            border: iced::Border {
                                radius: 0.0.into(),
                                width: 0.0,
                                color: Color::TRANSPARENT,
                            },
                            ..button::subtle(_theme, _status)
                        }),
                )
                .push(
                    button(text("Delete").size(13))
                        .on_press(ItemMessage::DeleteRequested)
                        .padding([6, 12])
                        .width(Length::Fill)
                        .style(|_theme, _status| button::Style {
                            border: iced::Border {
                                radius: 0.0.into(),
                                width: 0.0,
                                color: Color::TRANSPARENT,
                            },
                            ..button::subtle(_theme, _status)
                        }),
                );

            container(menu).width(150).into()
        };

        let wrapped: Element<ItemMessage> =
            iced_aw::ContextMenu::new(conn_row, context_menu).into();

        let mut col = Column::new();
        col = col.push(container(wrapped));

        if self.connection_status != ConnectionStatus::Disconnected {
            col = col.push(
                container(
                    text(self.connection_status.to_string())
                        .size(11)
                        .color(theme::TEXT_MUTED),
                )
                .padding([0, 18]),
            );
        }

        // Schema tree for connected DBs
        if is_connected {
            if self.schema_loading {
                col = col.push(
                    container(text("Loading schema…").size(11).color(theme::TEXT_MUTED))
                        .padding([2, 16]),
                );
            } else if !self.schema_tree.is_empty() {
                let tree = iced::Element::from(self.schema_tree.view())
                    .map(ItemMessage::SchemaTreeMessage);
                col = col.push(container(tree).padding([0, 6]));
            }
        }

        col
    }

    pub fn view_editor(&self) -> Element<'_, ItemMessage> {
        let run_btn = if self.running {
            button(
                row![text("⏳").size(13), text(" Running…").size(13),]
                    .align_y(iced::Alignment::Center),
            )
            .padding([6, 16])
            .style(iced::widget::button::secondary)
        } else {
            button(
                row![
                    svg(svg::Handle::from_memory(include_bytes!(
                        "../resources/play.svg"
                    )))
                    .height(12)
                    .width(12),
                    text("Run").size(13)
                ]
                .spacing(6)
                .align_y(iced::Alignment::Center),
            )
            .on_press(ItemMessage::RunQuery)
            .padding([6, 16])
            .style(iced::widget::button::primary)
        };

        let conn_info = format!(
            "{}@{}:{}/{}",
            self.cfg.user, self.cfg.host, self.cfg.port, self.cfg.database
        );

        let toolbar = container(
            row![
                run_btn,
                iced::widget::Space::new().width(Length::Fill),
                text(conn_info).size(11).color(theme::TEXT_MUTED),
            ]
            .spacing(8)
            .align_y(iced::Alignment::Center),
        )
        .padding([6, 10]);

        let editor = container(
            text_editor(&self.editor)
                .on_action(ItemMessage::EditorAction)
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
                    placeholder: Color::WHITE,
                    selection: Color::WHITE,
                    value: Color::WHITE,
                }),
        )
        .style(|theme: &Theme| {
            let palette = theme.extended_palette();
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
        });

        let results_area: Element<ItemMessage> = if self.running {
            container(
                row![
                    text("⏳").size(14),
                    text(" Executing query…").size(13).color(theme::TEXT_MUTED),
                ]
                .align_y(iced::Alignment::Center),
            )
            .padding(16)
            .into()
        } else if let Some(ref err) = self.error {
            container(
                column![
                    row![
                        text("⚠ ").size(14).color(theme::DANGER),
                        text("Error")
                            .size(14)
                            .color(theme::DANGER)
                            .font(iced::Font {
                                weight: iced::font::Weight::Bold,
                                ..iced::Font::DEFAULT
                            }),
                    ]
                    .align_y(iced::Alignment::Center),
                    text(err.as_str())
                        .size(13)
                        .font(iced::Font::MONOSPACE)
                        .color(theme::DANGER),
                ]
                .spacing(8),
            )
            .padding(16)
            .into()
        } else if let Some(ref qr) = self.result {
            self.view_results_table(qr)
        } else {
            container(
                column![
                    text("Run a query to see results here.")
                        .size(14)
                        .color(theme::TEXT_MUTED),
                    text("Type SQL above and press F5 or click Run")
                        .size(12)
                        .color(theme::TEXT_MUTED),
                ]
                .spacing(6)
                .align_x(iced::Alignment::Center),
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(iced::Alignment::Center)
            .align_y(iced::Alignment::Center)
            .into()
        };

        let has_result = self.result.is_some();

        // ── Status bar ────────────────────────────────────────────────────
        let status_bar = if let Some(ref qr) = self.result {
            let count_info = if qr.columns.is_empty() {
                format!("{}", qr.message)
            } else {
                format!(
                    "{} row(s)  |  {} column(s)",
                    qr.rows.len(),
                    qr.columns.len()
                )
            };
            container(
                row![
                    text(count_info).size(12).color(theme::SUCCESS),
                    iced::widget::Space::new().width(Length::Fill),
                    text(qr.message.as_str()).size(12).color(theme::TEXT_MUTED),
                ]
                .align_y(iced::Alignment::Center),
            )
            .padding([4, 12])
        } else if let Some(ref err) = self.error {
            let short = &err[..err.len().min(120)];
            container(
                row![
                    text("Error:").size(12).color(theme::DANGER),
                    iced::widget::Space::new().width(Length::Fixed(4.0)),
                    text(format!("{short}")).size(12).color(theme::DANGER),
                ]
                .align_y(iced::Alignment::Center),
            )
            .padding([4, 12])
        } else {
            container(
                row![
                    text("Ready").size(12).color(theme::TEXT_MUTED),
                    iced::widget::Space::new().width(Length::Fill),
                    text("Ctrl+Enter to run").size(11).color(theme::TEXT_MUTED),
                ]
                .align_y(iced::Alignment::Center),
            )
            .padding([4, 12])
        };

        column![
            toolbar,
            iced::widget::rule::horizontal(1),
            editor,
            iced::widget::rule::horizontal(1),
            scrollable(results_area).height(if has_result {
                Length::FillPortion(2)
            } else {
                Length::Fill
            }),
            iced::widget::rule::horizontal(1),
            status_bar,
        ]
        .height(Length::Fill)
        .into()
    }

    fn view_results_table<'a>(&self, qr: &'a QueryResult) -> Element<'a, ItemMessage> {
        if qr.columns.is_empty() {
            return container(text(qr.message.as_str()).size(13).color(theme::SUCCESS))
                .padding(16)
                .into();
        }

        let col_widths: Vec<f32> = qr
            .columns
            .iter()
            .enumerate()
            .map(|(i, col)| {
                let header_len = col.name.len();
                let max_data = qr
                    .rows
                    .iter()
                    .map(|r| r.cells.get(i).map(|s| s.len()).unwrap_or(0))
                    .max()
                    .unwrap_or(0);
                ((header_len.max(max_data) as f32) * 8.5 + 24.0).clamp(80.0, 350.0)
            })
            .collect();

        let total_width: f32 = 48.0 + col_widths.iter().sum::<f32>();

        let header = {
            let mut header_row = Row::new().width(Length::Fixed(total_width));
            header_row = header_row.push(
                container(text("#").size(12).color(theme::TEXT_MUTED))
                    .width(Length::Fixed(48.0))
                    .padding([7, 8]),
            );
            for (col, &w) in qr.columns.iter().zip(col_widths.iter()) {
                header_row = header_row.push(
                    container(text(col.name.as_str()).size(12).font(iced::Font {
                        weight: iced::font::Weight::Bold,
                        ..iced::Font::DEFAULT
                    }))
                    .width(Length::Fixed(w))
                    .padding([7, 8]),
                );
            }
            container(header_row)
                .width(Length::Fill)
                .style(|theme: &Theme| {
                    let palette = theme.extended_palette();
                    iced::widget::container::Style {
                        background: Some(palette.background.strong.color.into()),
                        ..Default::default()
                    }
                })
        };

        let mut data_col = Column::new().width(Length::Fixed(total_width));
        for (row_idx, row) in qr.rows.iter().enumerate() {
            let is_even = row_idx % 2 == 0;
            let mut data_row = Row::new();

            data_row = data_row.push(
                container(
                    text(format!("{}", row_idx + 1))
                        .size(11)
                        .font(iced::Font::MONOSPACE)
                        .color(theme::TEXT_MUTED),
                )
                .width(Length::Fixed(48.0))
                .padding([5, 8]),
            );

            for (col_idx, &w) in col_widths.iter().enumerate() {
                let cell: String = row.cells.get(col_idx).cloned().unwrap_or_default();
                let is_null = cell == "NULL";
                let cell_color = if is_null {
                    theme::TEXT_MUTED
                } else {
                    theme::TEXT
                };

                data_row = data_row.push(
                    container(
                        text(cell)
                            .size(12)
                            .font(iced::Font::MONOSPACE)
                            .color(cell_color),
                    )
                    .width(Length::Fixed(w))
                    .padding([5, 8]),
                );
            }

            let row_bg_style = move |theme: &Theme| {
                let palette = theme.extended_palette();
                iced::widget::container::Style {
                    background: Some(if is_even {
                        palette.background.base.color.into()
                    } else {
                        iced::Background::Color(Color::from_rgba(1.0, 1.0, 1.0, 0.02))
                    }),
                    ..Default::default()
                }
            };

            data_col = data_col.push(container(data_row).style(row_bg_style).width(Length::Fill));
        }

        let table_body = scrollable(data_col).direction(scrollable::Direction::Horizontal(
            scrollable::Scrollbar::default(),
        ));

        column![header, scrollable(table_body).height(Length::Fill),].into()
    }
}
