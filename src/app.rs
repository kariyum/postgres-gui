use iced::color;
use std::collections::HashMap;

use iced::widget::{
    Column, Row, button, column, container, row, scrollable, svg, text, text_editor,
};
use iced::{Color, Element, Length, Padding, Task, Theme};
use sqlx::PgPool;

use crate::components::connection_dialog::{self, ConnectionDialog, DialogMessage};
use crate::core::connection_config::ConnectionConfig;
use crate::db;
use crate::schema_tree;
use crate::theme;
use crate::types::{QueryResult, TreeNode, TreeNodeKind};

// Newtype wrapper so PgPool (which isn't Debug) can be carried in a Message.
#[derive(Clone)]
pub struct PoolMsg(pub PgPool);

impl std::fmt::Debug for PoolMsg {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PgPool")
    }
}

// ─── Messages ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum Message {
    // Connection sidebar
    AddConnection,
    EditConnection(String),
    DeleteConnection(String),
    ConnectTo(String),
    Disconnect(String),
    /// Carries the pool on success so we can store it in state.
    ConnectionResult(String, Result<PoolMsg, String>),

    // Schema tree toggle
    ToggleConnectionTree(String),
    ToggleSchemaNode(String, String),
    ToggleTableGroup(String, String),
    SelectTable(String, String, String),
    SchemaLoaded(String, Result<Vec<TreeNode>, String>),

    // Connection dialog
    ConnectionDialogMessage(connection_dialog::DialogMessage),

    // Query editor
    QueryEditorAction(text_editor::Action),
    RunQuery,
    QueryResult(String, Result<QueryResult, String>),

    // Active tab
    SelectConnection(String),

    // Layout
    ToggleSidebar,

    ZoomIn,
    ZoomOut,
    ZoomReset,

    Noop,
}

// ─── Tab state (one per connected DB) ──────────────────────────────────────────

#[derive(Debug)]
pub struct Tab {
    pub conn_id: String,
    pub pool: PgPool,
    pub editor: text_editor::Content,
    pub result: Option<QueryResult>,
    pub error: Option<String>,
    pub running: bool,
    pub schema_tree: Vec<TreeNode>,
    pub schema_loading: bool,
}

impl Tab {
    pub fn new(conn_id: String, pool: PgPool) -> Self {
        Self {
            conn_id,
            pool,
            editor: text_editor::Content::with_text("SELECT 1;"),
            result: None,
            error: None,
            running: false,
            schema_tree: Vec::new(),
            schema_loading: true, // starts loading immediately
        }
    }
}

// ─── Application state ─────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct App {
    pub connections: Vec<ConnectionConfig>,
    pub pools: HashMap<String, PgPool>,
    pub tabs: Vec<Tab>,
    pub active_tab: Option<String>,
    pub dialog: ConnectionDialog,
    pub connection_status: HashMap<String, String>,
    pub sidebar_open: bool,
    pub zoom_multiplier: u8,
}

impl Default for App {
    fn default() -> Self {
        Self {
            connections: crate::db_config::load_connections(),
            pools: HashMap::new(),
            tabs: Vec::new(),
            active_tab: None,
            dialog: ConnectionDialog::default(),
            connection_status: HashMap::new(),
            sidebar_open: true,
            zoom_multiplier: 0,
        }
    }
}

// ─── Update ────────────────────────────────────────────────────────────────────

impl App {
    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            // ── Dialog ───────────────────────────────────────────────────────
            Message::AddConnection => {
                self.dialog.open_new();
                Task::none()
            }
            Message::EditConnection(id) => {
                if let Some(cfg) = self.connections.iter().find(|c| c.id == id) {
                    self.dialog.open_edit(cfg);
                }
                Task::none()
            }
            Message::DeleteConnection(id) => {
                self.connections.retain(|c| c.id != id);
                let _ = crate::db_config::save_connections(&self.connections);
                self.pools.remove(&id);
                self.tabs.retain(|t| t.conn_id != id);
                if self.active_tab.as_deref() == Some(&id) {
                    self.active_tab = self.tabs.first().map(|t| t.conn_id.clone());
                }
                Task::none()
            }
            Message::ConnectionDialogMessage(msg) => match msg {
                DialogMessage::DialogSave => match self.dialog.build_config() {
                    Err(e) => {
                        self.dialog.error = Some(e);
                        Task::none()
                    }
                    Ok(cfg) => {
                        if let Some(existing) = self.connections.iter_mut().find(|c| c.id == cfg.id)
                        {
                            *existing = cfg;
                        } else {
                            self.connections.push(cfg);
                        }
                        let _ = crate::db_config::save_connections(&self.connections);
                        self.dialog.close();
                        Task::none()
                    }
                },
                _ => self
                    .dialog
                    .update(msg)
                    .map(Message::ConnectionDialogMessage),
            },
            // ── Connect ──────────────────────────────────────────────────────
            Message::ConnectTo(id) => {
                let cfg = match self.connections.iter().find(|c| c.id == id) {
                    Some(c) => c.clone(),
                    None => return Task::none(),
                };
                self.connection_status
                    .insert(id.clone(), "Connecting…".to_string());
                let cs = cfg.connection_string();
                let id2 = id.clone();
                Task::perform(
                    async move { db::connect(&cs).await.map(PoolMsg) },
                    move |result| Message::ConnectionResult(id2.clone(), result),
                )
            }

            Message::ConnectionResult(id, result) => {
                self.connection_status.remove(&id);
                match result {
                    Ok(pool_msg) => {
                        let pool = pool_msg.0;
                        if !self.tabs.iter().any(|t| t.conn_id == id) {
                            self.tabs.push(Tab::new(id.clone(), pool.clone()));
                        }
                        self.pools.insert(id.clone(), pool.clone());
                        self.active_tab = Some(id.clone());

                        // Load schema tree
                        let id2 = id.clone();
                        Task::perform(
                            async move { db::fetch_schema_tree(&pool).await },
                            move |r| Message::SchemaLoaded(id2.clone(), r),
                        )
                    }
                    Err(e) => {
                        let short = e[..e.len().min(80)].to_string();
                        self.connection_status.insert(id, format!("Error: {short}"));
                        Task::none()
                    }
                }
            }

            Message::Disconnect(id) => {
                self.pools.remove(&id);
                self.tabs.retain(|t| t.conn_id != id);
                if self.active_tab.as_deref() == Some(&id) {
                    self.active_tab = self.tabs.first().map(|t| t.conn_id.clone());
                }
                self.connection_status.remove(&id);
                Task::none()
            }

            // ── Schema tree ──────────────────────────────────────────────────
            Message::ToggleConnectionTree(id) => {
                self.active_tab = Some(id);
                Task::none()
            }

            Message::SchemaLoaded(id, result) => {
                if let Some(tab) = self.tabs.iter_mut().find(|t| t.conn_id == id) {
                    tab.schema_loading = false;
                    match result {
                        Ok(nodes) => tab.schema_tree = nodes,
                        Err(e) => tab.error = Some(format!("Schema load error: {e}")),
                    }
                }
                Task::none()
            }

            Message::ToggleSchemaNode(conn_id, schema_name) => {
                if let Some(tab) = self.tabs.iter_mut().find(|t| t.conn_id == conn_id) {
                    for node in &mut tab.schema_tree {
                        if node.label == schema_name && node.kind == TreeNodeKind::Schema {
                            node.expanded = !node.expanded;
                        }
                    }
                }
                Task::none()
            }

            Message::ToggleTableGroup(conn_id, schema_name) => {
                if let Some(tab) = self.tabs.iter_mut().find(|t| t.conn_id == conn_id) {
                    for schema in &mut tab.schema_tree {
                        if schema.label == schema_name {
                            for child in &mut schema.children {
                                if child.kind == TreeNodeKind::TableGroup {
                                    child.expanded = !child.expanded;
                                }
                            }
                        }
                    }
                }
                Task::none()
            }

            Message::SelectTable(conn_id, schema, table) => {
                if let Some(tab) = self.tabs.iter_mut().find(|t| t.conn_id == conn_id) {
                    let sql = format!("SELECT * FROM \"{schema}\".\"{table}\" LIMIT 100;");
                    tab.editor = text_editor::Content::with_text(&sql);
                }
                self.active_tab = Some(conn_id);
                Task::none()
            }

            Message::SelectConnection(id) => {
                self.active_tab = Some(id);
                Task::none()
            }

            Message::ToggleSidebar => {
                self.sidebar_open = !self.sidebar_open;
                Task::none()
            }

            // ── Query editor ─────────────────────────────────────────────────
            Message::QueryEditorAction(action) => {
                if let Some(id) = self.active_tab.clone() {
                    if let Some(tab) = self.tabs.iter_mut().find(|t| t.conn_id == id) {
                        tab.editor.perform(action);
                    }
                }
                Task::none()
            }

            Message::RunQuery => {
                let id = match self.active_tab.clone() {
                    Some(id) => id,
                    None => return Task::none(),
                };
                let tab = match self.tabs.iter_mut().find(|t| t.conn_id == id) {
                    Some(t) => t,
                    None => return Task::none(),
                };

                let sql = tab.editor.text();
                let pool = tab.pool.clone();
                tab.running = true;
                tab.result = None;
                tab.error = None;

                let id2 = id.clone();
                Task::perform(
                    async move { db::execute_query(&pool, &sql).await },
                    move |r| Message::QueryResult(id2.clone(), r),
                )
            }

            Message::QueryResult(id, result) => {
                if let Some(tab) = self.tabs.iter_mut().find(|t| t.conn_id == id) {
                    tab.running = false;
                    match result {
                        Ok(qr) => tab.result = Some(qr),
                        Err(e) => tab.error = Some(e),
                    }
                }
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

        // Overlay dialog on top if visible
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
                .padding([5, 10])
                .style(iced::widget::button::primary),
                // button(text("❮").size(12))
                //     .on_press(Message::ToggleSidebar)
                //     .padding([2, 6])
                //     .style(iced::widget::button::secondary),
            ]
            .align_y(iced::Alignment::Center),
        )
        .padding([10, 12]);

        let mut conn_list = Column::new().spacing(4).padding(Padding::from([0, 0])); // bottom padding handled by items

        for cfg in &self.connections {
            let is_connected = self.tabs.iter().any(|t| t.conn_id == cfg.id);
            let is_active = self.active_tab.as_deref() == Some(&cfg.id);

            let status_dot = if is_connected {
                text("●").size(10).color(theme::SUCCESS)
            } else {
                text("●").size(10).color(theme::TEXT_MUTED)
            };

            let connect_btn = if is_connected {
                button(text("⏏").size(12))
                    .on_press(Message::Disconnect(cfg.id.clone()))
                    .padding([3, 7])
                    .style(iced::widget::button::secondary)
            } else {
                button(text("▶").size(12))
                    .on_press(Message::ConnectTo(cfg.id.clone()))
                    .padding([3, 7])
                    .style(iced::widget::button::primary)
            };

            let cfg_id_1 = cfg.id.clone();
            let cfg_id_2 = cfg.id.clone();

            let conn_row = button(
                row![
                    status_dot,
                    column![
                        text(cfg.name.as_str()).size(13),
                        text(format!("{}:{}/{}", cfg.host, cfg.port, cfg.database))
                            .size(10)
                            .color(theme::TEXT_MUTED),
                    ]
                    .spacing(1),
                    iced::widget::Space::new().width(Length::Fill),
                    connect_btn,
                    button(text("✎").size(12))
                        .on_press(Message::EditConnection(cfg.id.clone()))
                        .padding([3, 7])
                        .style(iced::widget::button::secondary),
                    button(text("✕").size(12))
                        .on_press(Message::DeleteConnection(cfg.id.clone()))
                        .padding([3, 7])
                        .style(iced::widget::button::danger),
                ]
                .spacing(6)
                .align_y(iced::Alignment::Center),
            )
            .on_press(Message::SelectConnection(cfg_id_1))
            .width(Length::Fill)
            .padding([8, 10])
            .style(move |theme: &Theme, status| {
                let palette = theme.extended_palette();
                if is_active {
                    button::Style {
                        background: Some(palette.primary.weak.color.into()),
                        text_color: palette.primary.weak.text,
                        border: iced::Border {
                            radius: 6.0.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    }
                } else {
                    match status {
                        button::Status::Hovered | button::Status::Pressed => button::Style {
                            background: Some(palette.background.strong.color.into()),
                            text_color: palette.background.base.text,
                            border: iced::Border {
                                radius: 6.0.into(),
                                ..Default::default()
                            },
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

            conn_list = conn_list.push(container(conn_row).padding([0, 6]));

            // Connection status message (e.g. "Connecting…" or "Error: …")
            if let Some(status_msg) = self.connection_status.get(&cfg_id_2) {
                conn_list = conn_list.push(
                    container(text(status_msg.as_str()).size(11).color(theme::TEXT_MUTED))
                        .padding([0, 18]),
                );
            }

            // Schema tree for connected DBs
            if is_connected {
                if let Some(tab) = self.tabs.iter().find(|t| t.conn_id == cfg.id) {
                    if tab.schema_loading {
                        conn_list = conn_list.push(
                            container(text("  Loading schema…").size(11).color(theme::TEXT_MUTED))
                                .padding([2, 16]),
                        );
                    } else if !tab.schema_tree.is_empty() {
                        conn_list = conn_list.push(
                            container(schema_tree::render_tree(&tab.schema_tree, 0, &cfg.id))
                                .padding([0, 6]),
                        );
                    }
                }
            }
        }

        if self.connections.is_empty() {
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

        let body: Element<Message> = if let Some(ref active_id) = self.active_tab {
            if let Some(tab) = self.tabs.iter().find(|t| &t.conn_id == active_id) {
                self.view_editor_panel(tab)
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

        for tab in &self.tabs {
            let name: String = self
                .connections
                .iter()
                .find(|c| c.id == tab.conn_id)
                .map(|c| c.name.clone())
                .unwrap_or_else(|| "Unknown".to_string());

            let is_active = self.active_tab.as_deref() == Some(&tab.conn_id);
            let tab_id = tab.conn_id.clone();
            let close_id = tab.conn_id.clone();

            let tab_btn = button(
                row![
                    text("🔌").size(12),
                    text(name).size(13),
                    iced::widget::Space::new().width(Length::Fixed(4.0)),
                    button(text("✕").size(10))
                        .on_press(Message::Disconnect(close_id))
                        .padding([1, 4])
                        .style(iced::widget::button::text),
                ]
                .spacing(4)
                .align_y(iced::Alignment::Center),
            )
            .on_press(Message::SelectConnection(tab_id))
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

        if self.tabs.is_empty() {
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

    fn view_editor_panel<'a>(&'a self, tab: &'a Tab) -> Element<'a, Message> {
        // ── Toolbar ──────────────────────────────────────────────────────────
        let run_btn = if tab.running {
            button(
                row![text("⏳").size(13), text(" Running…").size(13),]
                    .align_y(iced::Alignment::Center),
            )
            .padding([6, 16])
            .style(iced::widget::button::secondary)
        } else {
            button(row![text("▶  Run").size(13)].align_y(iced::Alignment::Center))
                .on_press(Message::RunQuery)
                .padding([6, 16])
                .style(iced::widget::button::primary)
        };

        let conn_info = self
            .connections
            .iter()
            .find(|c| c.id == tab.conn_id)
            .map(|c| format!("{}@{}:{}/{}", c.user, c.host, c.port, c.database))
            .unwrap_or_default();

        let toolbar = container(
            row![
                run_btn,
                text("F5").size(11).color(theme::TEXT_MUTED),
                iced::widget::Space::new().width(Length::Fill),
                iced::widget::Space::new().width(Length::Fill),
                text(conn_info).size(11).color(theme::SUCCESS),
            ]
            .spacing(8)
            .align_y(iced::Alignment::Center),
        )
        .padding([6, 10]);

        // ── Editor ───────────────────────────────────────────────────────────
        let editor = container(
            text_editor(&tab.editor)
                .on_action(Message::QueryEditorAction)
                .height(Length::FillPortion(1))
                .font(iced::Font::MONOSPACE)
                .size(14)
                .padding(10),
        )
        .padding([4, 10])
        .style(|theme: &Theme| {
            let palette = theme.extended_palette();
            iced::widget::container::Style {
                background: Some(palette.background.base.color.into()),
                ..Default::default()
            }
        });

        // ── Results area ─────────────────────────────────────────────────────
        let results_area: Element<Message> = if tab.running {
            container(
                row![
                    text("⏳").size(14),
                    text(" Executing query…").size(13).color(theme::TEXT_MUTED),
                ]
                .align_y(iced::Alignment::Center),
            )
            .padding(16)
            .into()
        } else if let Some(ref err) = tab.error {
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
        } else if let Some(ref qr) = tab.result {
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

        let has_result = tab.result.is_some();

        // ── Status bar ───────────────────────────────────────────────────────
        let status_bar = if let Some(ref qr) = tab.result {
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
        } else if let Some(ref err) = tab.error {
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

    fn view_results_table<'a>(&'a self, qr: &'a QueryResult) -> Element<'a, Message> {
        if qr.columns.is_empty() {
            return container(text(qr.message.as_str()).size(13).color(theme::SUCCESS))
                .padding(16)
                .into();
        }

        // Calculate column widths from header + data
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

        // Total width for horizontal scrolling
        let total_width: f32 = 48.0 + col_widths.iter().sum::<f32>();

        // Header row (sticky — rendered outside the scrollable body)
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

        // Data rows
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

        // Horizontally scrollable body
        let table_body = scrollable(data_col).direction(scrollable::Direction::Horizontal(
            scrollable::Scrollbar::default(),
        ));

        column![header, scrollable(table_body).height(Length::Fill),].into()
    }
}
