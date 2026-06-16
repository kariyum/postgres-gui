use std::collections::HashMap;

use iced::widget::{
    button, column, container, row, scrollable, text, text_editor, Column, Row,
};
use iced::{Color, Element, Length, Padding, Task, Theme};
use sqlx::PgPool;

use crate::connection_dialog::ConnectionDialog;
use crate::db;
use crate::schema_tree;
use crate::types::{ConnectionConfig, QueryResult, TreeNode, TreeNodeKind};

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
    DialogNameChanged(String),
    DialogHostChanged(String),
    DialogPortChanged(String),
    DialogUserChanged(String),
    DialogPasswordChanged(String),
    DialogDatabaseChanged(String),
    DialogSave,
    DialogCancel,

    // Query editor
    QueryEditorAction(text_editor::Action),
    RunQuery,
    QueryResult(String, Result<QueryResult, String>),

    // Active tab
    SelectConnection(String),

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
            Message::DialogNameChanged(v) => {
                self.dialog.name = v;
                Task::none()
            }
            Message::DialogHostChanged(v) => {
                self.dialog.host = v;
                Task::none()
            }
            Message::DialogPortChanged(v) => {
                self.dialog.port = v;
                Task::none()
            }
            Message::DialogUserChanged(v) => {
                self.dialog.user = v;
                Task::none()
            }
            Message::DialogPasswordChanged(v) => {
                self.dialog.password = v;
                Task::none()
            }
            Message::DialogDatabaseChanged(v) => {
                self.dialog.database = v;
                Task::none()
            }
            Message::DialogCancel => {
                self.dialog.close();
                Task::none()
            }
            Message::DialogSave => match self.dialog.build_config() {
                Err(e) => {
                    self.dialog.error = Some(e);
                    Task::none()
                }
                Ok(cfg) => {
                    if let Some(existing) =
                        self.connections.iter_mut().find(|c| c.id == cfg.id)
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
                        self.connection_status
                            .insert(id, format!("Error: {short}"));
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

            Message::Noop => Task::none(),
        }
    }

    // ─── View ─────────────────────────────────────────────────────────────────

    pub fn view(&self) -> Element<'_, Message> {
        let sidebar = self.view_sidebar();
        let main = self.view_main();

        let layout: Element<Message> = row![
            sidebar,
            iced::widget::rule::vertical(1),
            main,
        ]
        .into();

        // Overlay dialog on top if visible
        if self.dialog.visible {
            let dialog = self.dialog.view();
            iced::widget::stack![
                layout,
                container(dialog)
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
                button(text("+").size(16))
                    .on_press(Message::AddConnection)
                    .padding([2, 10])
                    .style(iced::widget::button::primary),
            ]
            .align_y(iced::Alignment::Center),
        )
        .padding([10, 12]);

        let mut conn_list = Column::new()
            .spacing(4)
            .padding(Padding::from([0, 0])); // bottom padding handled by items

        for cfg in &self.connections {
            let is_connected = self.tabs.iter().any(|t| t.conn_id == cfg.id);
            let is_active = self.active_tab.as_deref() == Some(&cfg.id);

            let status_dot = if is_connected {
                text("●").size(10).color(Color::from_rgb(0.2, 0.8, 0.4))
            } else {
                text("●").size(10).color(Color::from_rgb(0.5, 0.5, 0.55))
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
                            .color(Color::from_rgb(0.55, 0.6, 0.65)),
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
                    container(
                        text(status_msg.as_str())
                            .size(11)
                            .color(Color::from_rgb(0.6, 0.65, 0.5)),
                    )
                    .padding([0, 18]),
                );
            }

            // Schema tree for connected DBs
            if is_connected {
                if let Some(tab) = self.tabs.iter().find(|t| t.conn_id == cfg.id) {
                    if tab.schema_loading {
                        conn_list = conn_list.push(
                            container(
                                text("  Loading schema…")
                                    .size(11)
                                    .color(Color::from_rgb(0.5, 0.55, 0.6)),
                            )
                            .padding([2, 16]),
                        );
                    } else if !tab.schema_tree.is_empty() {
                        conn_list = conn_list.push(
                            container(schema_tree::render_tree(
                                &tab.schema_tree,
                                0,
                                &cfg.id,
                            ))
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
                        text("No connections yet.").size(13).color(Color::from_rgb(0.5, 0.55, 0.6)),
                        text("Click + to add one.").size(12).color(Color::from_rgb(0.45, 0.5, 0.55)),
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

        column![
            tab_bar,
            iced::widget::rule::horizontal(1),
            body,
        ]
        .height(Length::Fill)
        .into()
    }

    fn view_tab_bar(&self) -> Element<'_, Message> {
        let mut tabs_row = Row::new().spacing(2).align_y(iced::Alignment::Center);

        for tab in &self.tabs {
            let name: String = self
                .connections
                .iter()
                .find(|c| c.id == tab.conn_id)
                .map(|c| c.name.clone())
                .unwrap_or_else(|| "Unknown".to_string());

            let is_active = self.active_tab.as_deref() == Some(&tab.conn_id);
            let tab_id = tab.conn_id.clone();

            let tab_btn = button(
                row![
                    text("🔌").size(12),
                    text(name),
                ]
                .spacing(6)
                .align_y(iced::Alignment::Center),
            )
            .on_press(Message::SelectConnection(tab_id))
            .padding([6, 14])
            .style(move |theme: &Theme, _status| {
                let palette = theme.extended_palette();
                if is_active {
                    button::Style {
                        background: Some(palette.background.base.color.into()),
                        text_color: palette.background.base.text,
                        border: iced::Border {
                            color: palette.primary.base.color,
                            width: 0.0,
                            radius: 0.0.into(),
                        },
                        ..Default::default()
                    }
                } else {
                    button::Style {
                        background: Some(palette.background.weak.color.into()),
                        text_color: palette.background.weak.text,
                        border: iced::Border::default(),
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
                    .color(Color::from_rgb(0.5, 0.55, 0.6)),
            );
        }

        container(tabs_row)
            .height(Length::Fixed(38.0))
            .width(Length::Fill)
            .padding([4, 8])
            .into()
    }

    fn view_welcome(&self) -> Element<'_, Message> {
        container(
            column![
                text("pgeru").size(40),
                text("A PostgreSQL client built with Rust + iced")
                    .size(16)
                    .color(Color::from_rgb(0.5, 0.55, 0.6)),
                text("← Add a connection in the sidebar and click ▶ to connect.")
                    .size(14)
                    .color(Color::from_rgb(0.5, 0.55, 0.6)),
            ]
            .spacing(14)
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
            button(text("Running…").size(13))
                .padding([6, 16])
                .style(iced::widget::button::secondary)
        } else {
            button(
                row![text("▶  Run").size(13)].align_y(iced::Alignment::Center),
            )
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
                text("F5 to run").size(11).color(Color::from_rgb(0.5, 0.55, 0.6)),
                iced::widget::Space::new().width(Length::Fill),
                text(conn_info)
                    .size(11)
                    .color(Color::from_rgb(0.4, 0.75, 0.5)),
            ]
            .spacing(14)
            .align_y(iced::Alignment::Center),
        )
        .padding([6, 10]);

        // ── Editor ───────────────────────────────────────────────────────────
        let editor = container(
            text_editor(&tab.editor)
                .on_action(Message::QueryEditorAction)
                .height(Length::Fixed(200.0))
                .font(iced::Font::MONOSPACE)
                .size(14)
                .padding(10),
        )
        .padding([4, 10]);

        // ── Results area ─────────────────────────────────────────────────────
        let results_area: Element<Message> = if tab.running {
            container(
                text("Executing query…")
                    .size(13)
                    .color(Color::from_rgb(0.5, 0.55, 0.6)),
            )
            .padding(16)
            .into()
        } else if let Some(ref err) = tab.error {
            container(
                column![
                    text("Error")
                        .size(14)
                        .color(Color::from_rgb(0.9, 0.3, 0.3)),
                    text(err.as_str())
                        .size(13)
                        .font(iced::Font::MONOSPACE)
                        .color(Color::from_rgb(0.85, 0.4, 0.4)),
                ]
                .spacing(8),
            )
            .padding(16)
            .into()
        } else if let Some(ref qr) = tab.result {
            self.view_results_table(qr)
        } else {
            container(
                text("Run a query to see results here.")
                    .size(13)
                    .color(Color::from_rgb(0.45, 0.5, 0.55)),
            )
            .padding(16)
            .into()
        };

        // ── Status bar ───────────────────────────────────────────────────────
        let status_bar = if let Some(ref qr) = tab.result {
            container(
                text(qr.message.as_str())
                    .size(12)
                    .color(Color::from_rgb(0.4, 0.75, 0.5)),
            )
            .padding([4, 12])
        } else if let Some(ref err) = tab.error {
            let short = &err[..err.len().min(120)];
            container(
                text(format!("Error: {short}"))
                    .size(12)
                    .color(Color::from_rgb(0.9, 0.3, 0.3)),
            )
            .padding([4, 12])
        } else {
            container(text("Ready").size(12).color(Color::from_rgb(0.5, 0.55, 0.6)))
                .padding([4, 12])
        };

        column![
            toolbar,
            iced::widget::rule::horizontal(1),
            editor,
            iced::widget::rule::horizontal(1),
            scrollable(results_area).height(Length::Fill),
            iced::widget::rule::horizontal(1),
            status_bar,
        ]
        .height(Length::Fill)
        .into()
    }

    fn view_results_table<'a>(&'a self, qr: &'a QueryResult) -> Element<'a, Message> {
        if qr.columns.is_empty() {
            return container(
                text(qr.message.as_str())
                    .size(13)
                    .color(Color::from_rgb(0.4, 0.75, 0.5)),
            )
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
                ((header_len.max(max_data) as f32) * 8.5 + 24.0).clamp(80.0, 300.0)
            })
            .collect();

        // Header row
        let header = {
            let mut header_row = Row::new();
            header_row = header_row.push(
                container(text("#").size(12).color(Color::from_rgb(0.5, 0.55, 0.65)))
                    .width(Length::Fixed(48.0))
                    .padding([6, 8]),
            );
            for (col, &w) in qr.columns.iter().zip(col_widths.iter()) {
                header_row = header_row.push(
                    container(
                        text(col.name.as_str()).size(12).font(iced::Font {
                            weight: iced::font::Weight::Bold,
                            ..iced::Font::DEFAULT
                        }),
                    )
                    .width(Length::Fixed(w))
                    .padding([6, 8]),
                );
            }
            container(header_row).style(|theme: &Theme| {
                let palette = theme.extended_palette();
                iced::widget::container::Style {
                    background: Some(palette.background.strong.color.into()),
                    ..Default::default()
                }
            })
        };

        // Data rows — cells are owned Strings moved into text()
        let mut data_col = Column::new();
        for (row_idx, row) in qr.rows.iter().enumerate() {
            let is_even = row_idx % 2 == 0;
            let mut data_row = Row::new();

            // Row number
            data_row = data_row.push(
                container(
                    text(format!("{}", row_idx + 1))
                        .size(11)
                        .font(iced::Font::MONOSPACE)
                        .color(Color::from_rgb(0.5, 0.55, 0.65)),
                )
                .width(Length::Fixed(48.0))
                .padding([5, 8]),
            );

            for (col_idx, &w) in col_widths.iter().enumerate() {
                let cell: String = row.cells.get(col_idx).cloned().unwrap_or_default();
                let is_null = cell == "NULL";
                let cell_color = if is_null {
                    Color::from_rgb(0.45, 0.45, 0.55)
                } else {
                    Color::from_rgb(0.85, 0.88, 0.93)
                };

                data_row = data_row.push(
                    container(
                        text(cell) // String moved in — no borrow
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
                        palette.background.weak.color.into()
                    }),
                    ..Default::default()
                }
            };

            data_col = data_col
                .push(container(data_row).style(row_bg_style).width(Length::Shrink));
        }

        // Horizontally scrollable table body, then vertically scrollable
        let table_body = scrollable(data_col)
            .direction(scrollable::Direction::Horizontal(
                scrollable::Scrollbar::default(),
            ));

        column![header, scrollable(table_body)].into()
    }
}
