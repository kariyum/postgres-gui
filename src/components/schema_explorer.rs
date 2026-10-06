use iced::futures::channel::mpsc::Sender;
use iced::widget::{
    Column, Row, button, column, container, row, rule, scrollable, space, text, text_input,
};
use iced::{Alignment, Color, Element, Length, Task, Theme, border};

use crate::core::database_keeper::{self, DatabaseKeeperMessage};
use crate::types::Schema;
use crate::widgets::palette::{Palette, PaletteMessage};
use crate::widgets::raw_text_input::{self, RawTextInput};

#[derive(Debug, Clone)]
pub struct DatabaseMetadata {
    pub connection: String,
    pub schemas: Vec<Metadata>,
}
impl DatabaseMetadata {
    pub fn new(connection: String) -> Self {
        Self {
            connection,
            schemas: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct TableMetadata {
    schema: String,
    name: String,
}

#[derive(Debug, Clone)]
pub enum Metadata {
    Schema(SchemaMetadata),
    Table(TableMetadata),
    Column(ColumnMetadata),
}

#[derive(Debug, Clone)]
pub struct SchemaMetadata {
    name: String,
}

#[derive(Debug, Clone)]
pub struct ColumnMetadata {
    schema_name: String,
    table_name: String,
    name: String,
}
#[derive(Debug, Clone)]
pub struct SchemaExplorer {
    visible: bool,
    stage: Stage,
    database_keeper: Option<Sender<DatabaseKeeperMessage>>,
}

#[derive(Debug, Clone)]
struct Browse {
    connection: String,
    entries: Vec<SchemaEntry>,
    schema: Schema,
    metadata: DatabaseMetadata,
    filtered_metadata: DatabaseMetadata,
    search: String,
    loading: bool,
    error: Option<String>,
}

#[derive(Debug, Clone)]
enum Stage {
    SelectConnection {
        query: String,
        names: Vec<String>,
        filtered: Vec<String>,
    },
    Browse(Browse),
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
    SchemaFilter(SchemaFilter),
}

#[derive(Debug, Clone)]
pub enum SchemaFilter {
    Schemas,
    Tables,
    Columns,
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
                if let Stage::Browse(Browse {
                    search: ref mut search_field,
                    ..
                }) = self.stage
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
            SchemaExplorerMessage::SchemaFilter(filter) => Task::none(),
        }
    }

    fn select_connection(&mut self, name: String) -> Task<SchemaExplorerMessage> {
        let Some(mut actor) = self.database_keeper.clone() else {
            self.stage = Stage::Browse(Browse {
                connection: name.clone(),
                entries: Vec::new(),
                search: String::new(),
                loading: false,
                error: Some("Database service not ready".into()),
                schema: Schema::new(name.clone()),
                filtered_metadata: DatabaseMetadata::new(name.clone()),
                metadata: DatabaseMetadata::new(name),
            });
            return Task::none();
        };

        self.stage = Stage::Browse(Browse {
            connection: name.clone(),
            entries: Vec::new(),
            search: String::new(),
            schema: Schema::new(name.clone()),
            loading: true,
            error: None,
            filtered_metadata: DatabaseMetadata::new(name.clone()),
            metadata: DatabaseMetadata::new(name.clone()),
        });

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
                if let Stage::Browse(Browse {
                    connection: c,
                    entries: e,
                    loading,
                    error,
                    schema: s,
                    ..
                }) = &mut self.stage
                {
                    if *c == connection {
                        *e = entries;
                        *loading = false;
                        *error = None;
                        *s = schema.clone();
                    }
                }
            }
            Err(e) => {
                if let Stage::Browse(Browse {
                    connection: c,
                    loading,
                    error,
                    ..
                }) = &mut self.stage
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
            Stage::Browse(browse) => Some(self.view_browse(browse)),
        }
    }

    fn view_quick_filter(&self) -> Element<'_, SchemaExplorerMessage> {
        let filters: Vec<Element<'_, SchemaExplorerMessage>> = vec![
            button(text("Schemas").size(12))
                .on_press(SchemaExplorerMessage::SchemaFilter(SchemaFilter::Schemas)),
            button(text("Tables").size(12))
                .on_press(SchemaExplorerMessage::SchemaFilter(SchemaFilter::Tables)),
            button(text("Columns").size(12))
                .on_press(SchemaExplorerMessage::SchemaFilter(SchemaFilter::Columns)),
        ]
        .into_iter()
        .map(|btn| {
            btn.padding([0, 8])
                .style(|theme, status| {
                    let base = button::primary(theme, status);
                    button::Style {
                        border: border::Border::default().rounded(999.0),
                        ..base
                    }
                })
                .into()
        })
        .collect();

        container(
            row![text("Quick Filters: ").size(12),]
                .extend(filters)
                .spacing(4)
                .wrap(),
        )
        .padding([4, 8])
        .into()
    }

    fn view_details(&self) -> Element<'_, SchemaExplorerMessage> {
        container(text("ok")).into()
    }

    fn view_schema_list<'a>(
        &'a self,
        database_metadata: &'a DatabaseMetadata,
    ) -> Element<'a, SchemaExplorerMessage> {
        Row::from_iter(
            database_metadata
                .schemas
                .iter()
                .map(|metadata| match metadata {
                    Metadata::Schema(schema_metadata) => view_schema_metadata(schema_metadata),
                    Metadata::Table(table_metadata) => view_table_metadata(table_metadata),
                    Metadata::Column(column_metadata) => view_column_metadata(column_metadata),
                }),
        )
        .into()
    }

    fn view_browse<'a>(&'a self, browse: &'a Browse) -> Element<'a, SchemaExplorerMessage> {
        let Browse {
            connection,
            entries,
            schema,
            metadata,
            filtered_metadata,
            search,
            loading,
            error,
        } = browse;
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
        } else if filtered_metadata.schemas.is_empty() {
            text("No results.").size(13).style(text::secondary).into()
        } else {
            self.view_schema_list(filtered_metadata).into()
        };

        let search_input = container(
            RawTextInput::new("Search tables and columns...")
                .value(search.as_str())
                .on_input(SchemaExplorerMessage::SearchChanged),
        )
        .width(Length::Fill)
        .padding([0, 4]);

        let form = column![
            column![search_input, rule::horizontal(1), self.view_quick_filter(),],
            row![
                scrollable(container(list).padding([0, 8]))
                    .direction(scrollable::Direction::Vertical(
                        scrollable::Scrollbar::new().width(4).scroller_width(4),
                    ))
                    .height(Length::Fill)
                    .width(Length::FillPortion(1)),
                rule::vertical(1),
                container(self.view_details())
                    .padding([0, 8])
                    .height(Length::Fill)
                    .width(Length::FillPortion(1)),
            ]
        ]
        .spacing(8)
        .padding(0)
        .width(Length::Fixed(920.0))
        .height(Length::Fixed(680.0));

        container(form)
            .style(|theme: &Theme| {
                let palette = theme.palette();
                container::Style {
                    background: Some(palette.background.base.color.into()),
                    border: iced::Border {
                        color: palette.background.strong.color,
                        width: 1.0,
                        radius: 5.0.into(),
                    },
                    ..Default::default()
                }
            })
            .into()
    }
}

fn view_column_metadata(column_metadata: &ColumnMetadata) -> Element<'_, SchemaExplorerMessage> {
    button(row![
        text(column_metadata.schema_name.as_str()),
        text("."),
        text(column_metadata.table_name.as_str()),
        text("."),
        text(column_metadata.name.as_str())
    ])
    .into()
}

fn view_table_metadata(table_metadata: &TableMetadata) -> Element<'_, SchemaExplorerMessage> {
    button(row![
        text(table_metadata.schema.as_str()),
        text("."),
        text(table_metadata.name.as_str())
    ])
    .into()
}

fn view_schema_metadata(schema_metadata: &SchemaMetadata) -> Element<'_, SchemaExplorerMessage> {
    text(schema_metadata.name.as_str()).into()
}

fn entry_row(entry: &SchemaEntry) -> Element<'_, SchemaExplorerMessage> {
    container(
        row![
            text(entry.full_name.as_str()).size(13).width(Length::Fill),
            text(entry.detail.as_str()).size(11).style(text::secondary),
        ]
        .align_y(Alignment::Center),
    )
    .padding([4, 0])
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
