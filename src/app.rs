use iced::color;

use iced::widget::{Column, Row, button, column, container, row, scrollable, svg, text};
use iced::{Color, Element, Length, Padding, Task, Theme};
use sqlx::PgPool;

use crate::components::connection_dialog::{self, ConnectionDialog, DialogMessage};
use crate::components::connection_item::{self, ConnectionItem, ItemMessage};
use crate::core::connection_config::ConnectionConfig;
use crate::db;
use crate::theme;

// ─── Messages ──────────────────────────────────────────────────────────────────

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum Message {
    // Connection item (replaces most old messages)
    ConnectionItemMessage(String, connection_item::ItemMessage),

    // Connection dialog
    AddConnection,
    ConnectionDialogMessage(connection_dialog::DialogMessage),

    // Startup / IO
    ConnectionsLoaded(Vec<ConnectionConfig>),
    ConnectionSaved(Result<(), String>),

    // Connect result (async callback — parent handles directly)
    ConnectCompleted(String, Result<PgPool, String>),

    // Layout
    ToggleSidebar,

    ZoomIn,
    ZoomOut,
    ZoomReset,

    Noop,
}

// ─── Application state ─────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct App {
    pub connection_items: Vec<ConnectionItem>,
    pub active_connection: Option<String>,
    pub dialog: ConnectionDialog,
    pub sidebar_open: bool,
    pub zoom_multiplier: u8,
}

impl Default for App {
    fn default() -> Self {
        Self {
            connection_items: Vec::new(),
            active_connection: None,
            dialog: ConnectionDialog::default(),
            sidebar_open: true,
            zoom_multiplier: 0,
        }
    }
}

// ─── Update ────────────────────────────────────────────────────────────────────

impl App {
    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            // ── Connection item messages ────────────────────────────────────
            Message::ConnectionItemMessage(id, msg) => {
                match msg {
                    ItemMessage::ConnectRequested => {
                        let cs = match self.connection_items.iter().find(|i| i.cfg.id == id) {
                            Some(item) => item.cfg.connection_string(),
                            None => return Task::none(),
                        };
                        if let Some(item) =
                            self.connection_items.iter_mut().find(|i| i.cfg.id == id)
                        {
                            item.connection_status = connection_item::ConnectionStatus::Connecting;
                        }
                        Task::perform(async move { db::connect(&cs).await }, move |result| {
                            Message::ConnectCompleted(id.clone(), result)
                        })
                    }

                    ItemMessage::DisconnectRequested => {
                        if let Some(item) =
                            self.connection_items.iter_mut().find(|i| i.cfg.id == id)
                        {
                            item.pool = None;
                            item.schema_tree =
                                crate::components::schema_tree::SchemaTree::new(Vec::new());
                            item.schema_loading = false;
                            item.result = None;
                            item.error = None;
                            item.connection_status =
                                connection_item::ConnectionStatus::Disconnected;
                        }
                        if self.active_connection.as_deref() == Some(&id) {
                            self.active_connection = self
                                .connection_items
                                .iter()
                                .find(|i| i.pool.is_some())
                                .map(|i| i.cfg.id.clone());
                        }
                        Task::none()
                    }

                    ItemMessage::EditRequested => {
                        let cfg = match self.connection_items.iter().find(|i| i.cfg.id == id) {
                            Some(item) => item.cfg.clone(),
                            None => return Task::none(),
                        };
                        Task::done(Message::ConnectionDialogMessage(DialogMessage::OpenEdit(
                            cfg,
                        )))
                    }

                    ItemMessage::DeleteRequested => {
                        self.connection_items.retain(|i| i.cfg.id != id);
                        if self.active_connection.as_deref() == Some(&id) {
                            self.active_connection = self
                                .connection_items
                                .iter()
                                .find(|i| i.pool.is_some())
                                .map(|i| i.cfg.id.clone());
                        }
                        let configs: Vec<ConnectionConfig> = self
                            .connection_items
                            .iter()
                            .map(|i| i.cfg.clone())
                            .collect();
                        Task::perform(
                            async move {
                                tokio::task::spawn_blocking(move || {
                                    crate::db_config::save_connections(&configs)
                                })
                                .await
                                .unwrap_or(Err("Background task failed".to_string()))
                            },
                            Message::ConnectionSaved,
                        )
                    }

                    ItemMessage::DuplicateRequested => {
                        if let Some(item) = self.connection_items.iter().find(|i| i.cfg.id == id) {
                            let mut new_cfg = item.cfg.clone();
                            new_cfg.id = uuid::Uuid::new_v4().to_string();
                            new_cfg.name = format!("{} (copy)", new_cfg.name);
                            self.connection_items.push(ConnectionItem::new(new_cfg));
                            let configs: Vec<ConnectionConfig> = self
                                .connection_items
                                .iter()
                                .map(|i| i.cfg.clone())
                                .collect();
                            Task::perform(
                                async move {
                                    tokio::task::spawn_blocking(move || {
                                        crate::db_config::save_connections(&configs)
                                    })
                                    .await
                                    .unwrap_or(Err("Background task failed".to_string()))
                                },
                                Message::ConnectionSaved,
                            )
                        } else {
                            Task::none()
                        }
                    }

                    ItemMessage::CopyStringRequested => {
                        if let Some(item) = self.connection_items.iter().find(|i| i.cfg.id == id) {
                            iced::clipboard::write(item.cfg.connection_string())
                        } else {
                            Task::none()
                        }
                    }

                    ItemMessage::RunQuery => {
                        let (sql, pool) =
                            match self.connection_items.iter().find(|i| i.cfg.id == id) {
                                Some(item) => (item.editor.text(), item.pool.clone()),
                                None => return Task::none(),
                            };
                        let pool = match pool {
                            Some(p) => p,
                            None => return Task::none(),
                        };
                        if let Some(item) =
                            self.connection_items.iter_mut().find(|i| i.cfg.id == id)
                        {
                            item.running = true;
                            item.result = None;
                            item.error = None;
                        }
                        let id2 = id.clone();
                        Task::perform(
                            async move { db::execute_query(&pool, &sql).await },
                            move |r| {
                                Message::ConnectionItemMessage(
                                    id2.clone(),
                                    ItemMessage::QueryResult(r),
                                )
                            },
                        )
                    }

                    ItemMessage::Select => {
                        self.active_connection = Some(id.clone());
                        Task::none()
                    }

                    // ─── Internal: delegate to item.update() ───────────────
                    _ => {
                        if let Some(item) =
                            self.connection_items.iter_mut().find(|i| i.cfg.id == id)
                        {
                            item.update(msg)
                                .map(move |m| Message::ConnectionItemMessage(id.clone(), m))
                        } else {
                            Task::none()
                        }
                    }
                }
            }

            // ── Connect result ──────────────────────────────────────────────
            Message::ConnectCompleted(id, result) => {
                if let Some(item) = self.connection_items.iter_mut().find(|i| i.cfg.id == id) {
                    match result {
                        Ok(pool) => {
                            item.pool = Some(pool.clone());
                            item.connection_status = connection_item::ConnectionStatus::Connected;
                            item.schema_loading = true;
                            self.active_connection = Some(id.clone());
                            let id2 = id.clone();
                            Task::perform(
                                async move { db::fetch_schema_tree(&pool).await },
                                move |r| {
                                    Message::ConnectionItemMessage(
                                        id2.clone(),
                                        ItemMessage::SchemaLoaded(r),
                                    )
                                },
                            )
                        }
                        Err(e) => {
                            let short = e[..e.len().min(80)].to_string();
                            item.connection_status =
                                connection_item::ConnectionStatus::Error(format!("Error: {short}"));
                            Task::none()
                        }
                    }
                } else {
                    Task::none()
                }
            }

            // ── Dialog ──────────────────────────────────────────────────────
            Message::AddConnection => {
                Task::done(Message::ConnectionDialogMessage(DialogMessage::OpenNew))
            }

            Message::ConnectionDialogMessage(msg) => {
                if let DialogMessage::DialogSaved(cfg) = &msg {
                    if let Some(existing) = self
                        .connection_items
                        .iter_mut()
                        .find(|i| i.cfg.id == cfg.id)
                    {
                        existing.cfg = cfg.clone();
                    } else {
                        self.connection_items.push(ConnectionItem::new(cfg.clone()));
                    }

                    let configs: Vec<ConnectionConfig> = self
                        .connection_items
                        .iter()
                        .map(|i| i.cfg.clone())
                        .collect();
                    Task::batch([
                        self.dialog
                            .update(DialogMessage::DialogSaved(cfg.clone()))
                            .map(Message::ConnectionDialogMessage),
                        Task::perform(
                            async move {
                                tokio::task::spawn_blocking(move || {
                                    crate::db_config::save_connections(&configs)
                                })
                                .await
                                .unwrap_or(Err("Background task failed".to_string()))
                            },
                            Message::ConnectionSaved,
                        ),
                    ])
                } else {
                    let task = self.dialog.update(msg);
                    task.map(Message::ConnectionDialogMessage)
                }
            }

            Message::ConnectionSaved(Ok(())) => {
                Task::done(Message::ConnectionDialogMessage(DialogMessage::DialogClose))
            }

            Message::ConnectionSaved(Err(e)) => {
                eprintln!("Failed to save connection: {e}");
                Task::none()
            }

            // ── Startup ─────────────────────────────────────────────────────
            Message::ConnectionsLoaded(configs) => {
                for cfg in configs {
                    self.connection_items.push(ConnectionItem::new(cfg));
                }
                Task::none()
            }

            // ── Layout ──────────────────────────────────────────────────────
            Message::ToggleSidebar => {
                self.sidebar_open = !self.sidebar_open;
                Task::none()
            }
            Message::ZoomIn => {
                self.zoom_multiplier += 1;
                Task::none()
            }
            Message::ZoomOut => {
                if self.zoom_multiplier > 0 {
                    self.zoom_multiplier -= 1;
                }
                Task::none()
            }
            Message::ZoomReset => {
                self.zoom_multiplier = 0;
                Task::none()
            }

            Message::Noop => Task::none(),
        }
    }

    // ─── View ─────────────────────────────────────────────────────────────────

    pub fn view(&self) -> Element<'_, Message> {
        let main = self.view_main();

        let layout: Element<Message> = if self.sidebar_open {
            row![self.view_sidebar(), iced::widget::rule::vertical(1), main,].into()
        } else {
            row![
                container(
                    button(text("☰").size(16))
                        .on_press(Message::ToggleSidebar)
                        .padding([6, 8])
                        .style(iced::widget::button::secondary),
                )
                .padding([4, 0])
                .width(Length::Fixed(32.0))
                .height(Length::Fill)
                .align_y(iced::Alignment::Start),
                iced::widget::rule::vertical(1),
                main,
            ]
            .into()
        };

        if let Some(dialog) = self.dialog.view() {
            iced::widget::stack![
                layout,
                container(dialog.map(Message::ConnectionDialogMessage))
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .align_x(iced::Alignment::Center)
                    .align_y(iced::Alignment::Center)
                    .style(|_: &Theme| iced::widget::container::Style {
                        background: Some(Color::from_rgba(0.0, 0.0, 0.0, 0.45).into()),
                        ..Default::default()
                    }),
            ]
            .into()
        } else {
            layout
        }
    }

    // ── Sidebar ───────────────────────────────────────────────────────────────

    fn view_sidebar(&self) -> Element<'_, Message> {
        let header = container(
            row![
                text("Connections").size(13),
                iced::widget::Space::new().width(Length::Fill),
                button(row![
                    svg(svg::Handle::from_memory(include_bytes!(
                        "resources/plus.svg"
                    )))
                    .width(16)
                    .height(16)
                    .style(|_theme, _status| svg::Style {
                        color: Some(color!(255, 255, 255))
                    }),
                ])
                .on_press(Message::AddConnection)
                .style(iced::widget::button::primary),
            ]
            .align_y(iced::Alignment::Center),
        )
        .padding([10, 12]);

        let mut conn_list = Column::new().spacing(4).padding(Padding::from([0, 0]));

        for item in &self.connection_items {
            let id = item.cfg.id.clone();
            let view: Element<'_, connection_item::ItemMessage> = item.view().into();
            let view = view.map(move |msg| Message::ConnectionItemMessage(id.clone(), msg));
            conn_list = conn_list.push(view);
        }

        if self.connection_items.is_empty() {
            conn_list = conn_list.push(
                container(
                    column![
                        text("No connections yet.")
                            .size(13)
                            .color(theme::TEXT_MUTED),
                        text("Click + to add one.")
                            .size(12)
                            .color(theme::TEXT_MUTED),
                    ]
                    .spacing(4)
                    .align_x(iced::Alignment::Center),
                )
                .padding([20, 12])
                .width(Length::Fill),
            );
        }

        container(column![
            header,
            iced::widget::rule::horizontal(1),
            scrollable(conn_list).height(Length::Fill),
        ])
        .width(Length::Fixed(260.0))
        .height(Length::Fill)
        .into()
    }

    // ── Main area ─────────────────────────────────────────────────────────────

    fn view_main(&self) -> Element<'_, Message> {
        let tab_bar = self.view_tab_bar();

        let body: Element<Message> = if let Some(ref active_id) = self.active_connection {
            if let Some(item) = self
                .connection_items
                .iter()
                .find(|i| &i.cfg.id == active_id)
            {
                item.view_editor()
                    .map(move |msg| Message::ConnectionItemMessage(active_id.clone(), msg))
            } else {
                self.view_welcome()
            }
        } else {
            self.view_welcome()
        };

        column![tab_bar, iced::widget::rule::horizontal(1), body,]
            .height(Length::Fill)
            .into()
    }

    fn view_tab_bar(&self) -> Element<'_, Message> {
        let mut tabs_row = Row::new().align_y(iced::Alignment::Center);

        for item in &self.connection_items {
            if item.pool.is_none() {
                continue;
            }

            let is_active = self.active_connection.as_deref() == Some(&item.cfg.id);
            let id = item.cfg.id.clone();
            let name = item.cfg.name.clone();

            let tab_btn = button(
                row![
                    text("🔌").size(12),
                    text(name).size(13),
                    iced::widget::Space::new().width(Length::Fixed(4.0)),
                    button(text("✕").size(10))
                        .on_press(Message::ConnectionItemMessage(
                            id.clone(),
                            ItemMessage::DisconnectRequested,
                        ))
                        .padding([1, 4])
                        .style(iced::widget::button::text),
                ]
                .spacing(4)
                .align_y(iced::Alignment::Center),
            )
            .on_press(Message::ConnectionItemMessage(
                id.clone(),
                ItemMessage::Select,
            ))
            .padding([4, 8])
            .style(move |theme: &Theme, _status| {
                let palette = theme.extended_palette();
                if is_active {
                    button::Style {
                        background: Some(palette.background.base.color.into()),
                        text_color: palette.background.base.text,
                        border: iced::Border {
                            width: 1.0,
                            color: palette.primary.base.color,
                            radius: 6.0.into(),
                        },
                        ..Default::default()
                    }
                } else {
                    button::Style {
                        background: Some(palette.background.weak.color.into()),
                        text_color: palette.background.weak.text,
                        border: iced::Border {
                            width: 1.0,
                            color: palette.background.strong.color,
                            radius: 6.0.into(),
                        },
                        ..Default::default()
                    }
                }
            });

            tabs_row = tabs_row.push(tab_btn);
        }

        if self.connection_items.iter().all(|i| i.pool.is_none()) {
            tabs_row = tabs_row.push(
                text("  Connect to a database to get started")
                    .size(12)
                    .color(theme::TEXT_MUTED),
            );
        }

        container(tabs_row)
            .height(Length::Fixed(38.0))
            .width(Length::Fill)
            .padding([4, 8])
            .style(|theme: &Theme| {
                let palette = theme.extended_palette();
                iced::widget::container::Style {
                    background: Some(palette.background.weak.color.into()),
                    ..Default::default()
                }
            })
            .into()
    }

    fn view_welcome(&self) -> Element<'_, Message> {
        container(
            column![
                text("pgeru").size(48).font(iced::Font {
                    weight: iced::font::Weight::Bold,
                    ..iced::Font::DEFAULT
                }),
                text("PostgreSQL client").size(18).color(theme::TEXT_MUTED),
                iced::widget::Space::new().height(Length::Fixed(24.0)),
                column![
                    text("📋  Add a connection  →  click  +  in the sidebar")
                        .size(14)
                        .color(theme::TEXT_MUTED),
                    text("🔌  Connect  →  click  ▶  on a saved connection")
                        .size(14)
                        .color(theme::TEXT_MUTED),
                    text("⌨   Run queries  →  type SQL & press  F5  or click  Run")
                        .size(14)
                        .color(theme::TEXT_MUTED),
                ]
                .spacing(8)
                .align_x(iced::Alignment::Start),
            ]
            .spacing(6)
            .align_x(iced::Alignment::Center),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(iced::Alignment::Center)
        .align_y(iced::Alignment::Center)
        .into()
    }
}
