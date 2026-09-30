use iced::futures::channel::mpsc::Sender;
use iced::widget::{
    Column, button, column, container, row, rule, scrollable, space, text, text_input,
};
use iced::{Alignment, Color, Element, Length, Task, Theme};

use crate::core::database_keeper::{self, DatabaseKeeperMessage};
use crate::types::Schema;
use crate::widgets::palette::{Palette, PaletteMessage};

#[derive(Debug, Clone)]
pub struct SchemaExplorer {
    visible: bool,
    stage: Stage,
    database_keeper: Option<Sender<DatabaseKeeperMessage>>,
}

#[derive(Debug, Clone)]
enum Stage {
    SelectConnection {
        query: String,
        names: Vec<String>,
        filtered: Vec<String>,
    },
    Browse {
        connection: String,
        entries: Vec<SchemaEntry>,
        search: String,
        loading: bool,
        error: Option<String>,
    },
}

#[derive(Debug, Clone)]
struct SchemaEntry {
    full_name: String,
    detail: String,
}

#[derive(Debug, Clone)]
pub enum SchemaExplorerMessage {
    ConnectionPalette(PaletteMessage),
    SearchChanged(String),
    SchemaLoaded {
        connection: String,
        result: Result<Schema, String>,
    },
    Close,
}

impl Default for SchemaExplorer {
    fn default() -> Self {
        Self {
            visible: false,
            stage: Stage::SelectConnection {
                query: String::new(),
                names: Vec::new(),
                filtered: Vec::new(),
            },
            database_keeper: None,
        }
    }
}

impl SchemaExplorer {
    pub fn open(
        &mut self,
        names: Vec<String>,
        database_keeper: Option<Sender<DatabaseKeeperMessage>>,
    ) {
        self.visible = true;
        self.database_keeper = database_keeper;
        self.stage = Stage::SelectConnection {
            query: String::new(),
            filtered: names.clone(),
            names,
        };
    }

    pub fn close(&mut self) {
        self.visible = false;
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    pub fn is_selecting_connection(&self) -> bool {
        matches!(self.stage, Stage::SelectConnection { .. })
    }

    pub fn update(&mut self, message: SchemaExplorerMessage) -> Task<SchemaExplorerMessage> {
        match message {
            SchemaExplorerMessage::ConnectionPalette(PaletteMessage::InputChanged(query)) => {
                if let Stage::SelectConnection {
                    query: ref mut query_field,
                    ref names,
                    ref mut filtered,
                } = self.stage
                {
                    *query_field = query.clone();
                    *filtered = filter_connections(names, &query);
                }
                Task::none()
            }
            SchemaExplorerMessage::ConnectionPalette(PaletteMessage::Activated(index)) => {
                let name = match &self.stage {
                    Stage::SelectConnection { filtered, .. } => filtered.get(index).cloned(),
                    _ => None,
                };
                match name {
                    Some(name) => self.select_connection(name),
                    None => Task::none(),
                }
            }
            SchemaExplorerMessage::SearchChanged(search) => {
                if let Stage::Browse {
                    search: ref mut search_field,
                    ..
                } = self.stage
                {
                    *search_field = search;
                }
                Task::none()
            }
            SchemaExplorerMessage::SchemaLoaded { connection, result } => {
                self.handle_schema_loaded(connection, result);
                Task::none()
            }
            SchemaExplorerMessage::Close => {
                self.close();
                Task::none()
            }
        }
    }

    fn select_connection(&mut self, name: String) -> Task<SchemaExplorerMessage> {
        let Some(mut actor) = self.database_keeper.clone() else {
            self.stage = Stage::Browse {
                connection: name,
                entries: Vec::new(),
                search: String::new(),
                loading: false,
                error: Some("Database service not ready".into()),
            };
            return Task::none();
        };

        self.stage = Stage::Browse {
            connection: name.clone(),
            entries: Vec::new(),
            search: String::new(),
            loading: true,
            error: None,
        };

        let connection = name.clone();
        Task::perform(
            async move {
                database_keeper::get_schema(&mut actor, &name)
                    .await
                    .map_err(|e| e.to_string())
            },
            move |result| SchemaExplorerMessage::SchemaLoaded { connection, result },
        )
    }

    fn handle_schema_loaded(&mut self, connection: String, result: Result<Schema, String>) {
        match result {
            Ok(schema) => {
                let entries = flatten(&schema);
                if let Stage::Browse {
                    connection: c,
                    entries: e,
                    loading,
                    error,
                    ..
                } = &mut self.stage
                {
                    if *c == connection {
                        *e = entries;
                        *loading = false;
                        *error = None;
                    }
                }
            }
            Err(e) => {
                if let Stage::Browse {
                    connection: c,
                    loading,
                    error,
                    ..
                } = &mut self.stage
                {
                    if *c == connection {
                        if e == "Schema is still loading" {
                            *loading = true;
                        } else {
                            *loading = false;
                            *error = Some(e);
                        }
                    }
                }
            }
        }
    }

    pub fn view(&self) -> Option<Element<'_, SchemaExplorerMessage>> {
        if !self.visible {
            return None;
        }

        match &self.stage {
            Stage::SelectConnection {
                query, filtered, ..
            } => {
                let labels: Vec<&str> = filtered.iter().map(|s| s.as_str()).collect();
                let palette: Element<'_, PaletteMessage> =
                    Palette::new(query.as_str(), labels).into();
                Some(palette.map(SchemaExplorerMessage::ConnectionPalette))
            }
            Stage::Browse { .. } => Some(self.view_browse()),
        }
    }

    fn view_browse(&self) -> Element<'_, SchemaExplorerMessage> {
        let Stage::Browse {
            connection,
            entries,
            search,
            loading,
            error,
        } = &self.stage
        else {
            return text("").into();
        };

        let title = format!("Explore Schema: {connection}");

        let filtered: Vec<&SchemaEntry> = if search.is_empty() {
            entries.iter().collect()
        } else {
            let needle = search.to_lowercase();
            entries
                .iter()
                .filter(|e| {
                    e.full_name.to_lowercase().contains(&needle)
                        || e.detail.to_lowercase().contains(&needle)
                })
                .collect()
        };

        let list: Element<'_, SchemaExplorerMessage> = if *loading {
            text("Loading schema...")
                .size(13)
                .style(text::secondary)
                .into()
        } else if let Some(err) = error {
            text(err.as_str())
                .size(13)
                .color(crate::theme::DANGER)
                .into()
        } else if filtered.is_empty() {
            text("No results.").size(13).style(text::secondary).into()
        } else {
            Column::from_vec(filtered.into_iter().map(entry_row).collect())
                .spacing(2)
                .into()
        };

        let search_input = text_input("Search tables and columns...", search.as_str())
            .on_input(SchemaExplorerMessage::SearchChanged)
            .padding(8)
            .size(14)
            .width(Length::Fill);

        let close_btn = button(text("Close").size(14))
            .on_press(SchemaExplorerMessage::Close)
            .padding([8, 18])
            .style(iced::widget::button::secondary);

        let mut form = column![
            text(title).size(18),
            rule::horizontal(1),
            search_input,
            rule::horizontal(1),
            scrollable(list).height(Length::Fill)
        ]
        .spacing(14)
        .padding(24)
        .width(Length::Fixed(520.0))
        .height(Length::Fixed(480.0));

        form = form.push(container(row![space::horizontal(), close_btn].spacing(10)));

        container(form)
            .style(|theme: &Theme| {
                let palette = theme.palette();
                container::Style {
                    background: Some(palette.background.base.color.into()),
                    border: iced::Border {
                        color: palette.background.strong.color,
                        width: 1.0,
                        radius: 10.0.into(),
                    },
                    shadow: iced::Shadow {
                        color: Color::from_rgba(0.0, 0.0, 0.0, 0.4),
                        offset: iced::Vector::new(0.0, 8.0),
                        blur_radius: 24.0,
                    },
                    ..Default::default()
                }
            })
            .into()
    }
}

fn entry_row(entry: &SchemaEntry) -> Element<'_, SchemaExplorerMessage> {
    row![
        text(entry.full_name.as_str()).size(13).width(Length::Fill),
        text(entry.detail.as_str()).size(11).style(text::secondary),
    ]
    .align_y(Alignment::Center)
    .into()
}

fn flatten(schema: &Schema) -> Vec<SchemaEntry> {
    let mut entries = Vec::new();
    for s in &schema.schemas {
        entries.push(SchemaEntry {
            full_name: s.name.clone(),
            detail: format!("{} tables", s.tables.len()),
        });
        for t in &s.tables {
            entries.push(SchemaEntry {
                full_name: format!("{}.{}", s.name, t.name),
                detail: format!("{} columns", t.columns.len()),
            });
            for c in &t.columns {
                entries.push(SchemaEntry {
                    full_name: format!("{}.{}.{}", s.name, t.name, c.name),
                    detail: c.data_type.clone(),
                });
            }
        }
    }
    entries
}

fn filter_connections(names: &[String], query: &str) -> Vec<String> {
    let needle = query.to_lowercase();
    names
        .iter()
        .filter(|name| name.to_lowercase().contains(&needle))
        .cloned()
        .collect()
}
