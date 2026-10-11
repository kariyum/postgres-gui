use iced::futures::channel::mpsc::Sender;
use iced::widget::{
    Column, Id, button, column, container, operation, row, rule, scrollable, space, text,
    text_input,
};
use iced::{Alignment, Color, Element, Length, Task, Theme, border};

use crate::core::database_keeper::{self, DatabaseKeeperMessage};
use crate::types::{ForeignKeyRef, Schema};
use crate::widgets::palette::{Palette, PaletteMessage};

const CONNECTION_INPUT_ID: &str = "connection_picker_input";
const SEARCH_QUERY_INPUT_ID: &str = "search_query_picker_input";

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

impl From<Schema> for DatabaseMetadata {
    fn from(schema: Schema) -> Self {
        let Schema {
            connection,
            schemas: schema_infos,
        } = schema;

        let mut schemas = Vec::new();
        for schema_info in schema_infos {
            schemas.push(Metadata::Schema(SchemaMetadata {
                name: schema_info.name.clone(),
            }));

            for table in schema_info.tables {
                let columns: Vec<ColumnMetadata> = table
                    .columns
                    .iter()
                    .map(|column| ColumnMetadata {
                        schema_name: schema_info.name.clone(),
                        table_name: table.name.clone(),
                        name: column.name.clone(),
                        data_type: column.data_type.clone(),
                        nullable: column.nullable,
                        column_default: column.column_default.clone(),
                        character_maximum_length: column.character_maximum_length,
                        is_primary_key: column.is_primary_key,
                        is_foreign_key: column.is_foreign_key,
                        references: column.references.clone(),
                    })
                    .collect();

                for column in &columns {
                    schemas.push(Metadata::Column(column.clone()));
                }

                let primary_key: Vec<String> = columns
                    .iter()
                    .filter(|column| column.is_primary_key)
                    .map(|column| column.name.clone())
                    .collect();
                let foreign_keys: Vec<TableForeignKey> = columns
                    .iter()
                    .filter_map(|column| {
                        column.references.as_ref().map(|reference| TableForeignKey {
                            column: column.name.clone(),
                            references: reference.clone(),
                        })
                    })
                    .collect();

                schemas.push(Metadata::Table(TableMetadata {
                    schema: schema_info.name.clone(),
                    name: table.name.clone(),
                    columns,
                    primary_key,
                    foreign_keys,
                }));
            }
        }

        Self {
            connection,
            schemas,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TableMetadata {
    schema: String,
    name: String,
    columns: Vec<ColumnMetadata>,
    primary_key: Vec<String>,
    foreign_keys: Vec<TableForeignKey>,
}

#[derive(Debug, Clone)]
pub struct TableForeignKey {
    column: String,
    references: ForeignKeyRef,
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
    data_type: String,
    nullable: bool,
    column_default: Option<String>,
    character_maximum_length: Option<i32>,
    is_primary_key: bool,
    is_foreign_key: bool,
    references: Option<ForeignKeyRef>,
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
    schema: Schema,
    metadata: DatabaseMetadata,
    filtered_metadata: DatabaseMetadata,
    search: String,
    loading: bool,
    schema_filter: Option<SchemaFilter>,
    selected_metadata: Option<Metadata>,
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
pub enum SchemaExplorerMessage {
    ConnectionPalette(PaletteMessage),
    SearchChanged(String),
    SchemaLoaded {
        connection: String,
        result: Result<Schema, String>,
    },
    Close,
    SchemaFilter(SchemaFilter),
    SelectDetails(Metadata),
}

#[derive(Debug, Clone, PartialEq, Eq)]
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
    ) -> Task<SchemaExplorerMessage> {
        self.visible = true;
        self.database_keeper = database_keeper;
        self.stage = Stage::SelectConnection {
            query: String::new(),
            filtered: names.clone(),
            names,
        };

        operation::focus(Id::new(CONNECTION_INPUT_ID))
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
            SchemaExplorerMessage::SchemaFilter(filter) => {
                if let Stage::Browse(Browse { schema_filter, .. }) = &mut self.stage {
                    if schema_filter.as_ref() == Some(&filter) {
                        *schema_filter = None;
                    } else {
                        *schema_filter = Some(filter);
                    }
                };
                Task::none()
            }
            SchemaExplorerMessage::SelectDetails(metadata) => {
                if let Stage::Browse(Browse {
                    selected_metadata, ..
                }) = &mut self.stage
                {
                    *selected_metadata = Some(metadata);
                };
                Task::none()
            }
        }
    }

    fn select_connection(&mut self, name: String) -> Task<SchemaExplorerMessage> {
        let Some(mut actor) = self.database_keeper.clone() else {
            self.stage = Stage::Browse(Browse {
                connection: name.clone(),
                search: String::new(),
                loading: false,
                error: Some("Database service not ready".into()),
                schema: Schema::new(name.clone()),
                filtered_metadata: DatabaseMetadata::new(name.clone()),
                metadata: DatabaseMetadata::new(name),
                selected_metadata: None,
                schema_filter: None,
            });
            return Task::none();
        };

        self.stage = Stage::Browse(Browse {
            connection: name.clone(),
            search: String::new(),
            schema: Schema::new(name.clone()),
            loading: true,
            error: None,
            filtered_metadata: DatabaseMetadata::new(name.clone()),
            metadata: DatabaseMetadata::new(name.clone()),
            selected_metadata: None,
            schema_filter: None,
        });

        let connection = name.clone();
        Task::batch(vec![
            Task::perform(
                async move {
                    database_keeper::get_schema(&mut actor, &name)
                        .await
                        .map_err(|e| e.to_string())
                },
                move |result| SchemaExplorerMessage::SchemaLoaded { connection, result },
            ),
            operation::focus(Id::new(SEARCH_QUERY_INPUT_ID)),
        ])
    }

    fn handle_schema_loaded(&mut self, connection: String, result: Result<Schema, String>) {
        match result {
            Ok(schema) => {
                let metadata = DatabaseMetadata::from(schema.clone());
                if let Stage::Browse(Browse {
                    connection: c,
                    loading,
                    error,
                    schema: s,
                    metadata: m,
                    filtered_metadata: fm,
                    ..
                }) = &mut self.stage
                {
                    if *c == connection {
                        *loading = false;
                        *error = None;
                        *s = schema;
                        *m = metadata.clone();
                        *fm = metadata;
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
                let palette: Element<'_, PaletteMessage> = Palette::new(query.as_str(), labels)
                    .input_id(Id::new(CONNECTION_INPUT_ID))
                    .into();
                Some(palette.map(SchemaExplorerMessage::ConnectionPalette))
            }
            Stage::Browse(browse) => Some(self.view_browse(browse)),
        }
    }

    fn view_quick_filter<'a>(&'a self, browse: &'a Browse) -> Element<'a, SchemaExplorerMessage> {
        let filters: Vec<Element<'_, SchemaExplorerMessage>> = vec![
            SchemaFilter::Schemas,
            SchemaFilter::Tables,
            SchemaFilter::Columns,
        ]
        .into_iter()
        .map(|schema_filter| {
            let title = match schema_filter {
                SchemaFilter::Schemas => "Schemas",
                SchemaFilter::Tables => "Tables",
                SchemaFilter::Columns => "Columns",
            };

            button(text(title).size(12))
                .on_press(SchemaExplorerMessage::SchemaFilter(schema_filter.clone()))
                .padding([0, 8])
                .style(move |theme, status| {
                    let base = button::primary(theme, status);
                    button::Style {
                        border: border::Border::default().rounded(5.0),
                        text_color: {
                            if matches!(status, button::Status::Active | button::Status::Hovered)
                                && browse
                                    .schema_filter
                                    .as_ref()
                                    .is_some_and(|filter| *filter == schema_filter.clone())
                            {
                                base.text_color
                            } else {
                                button::subtle(theme, status).text_color
                            }
                        },
                        background: {
                            if matches!(status, button::Status::Active | button::Status::Hovered)
                                && browse
                                    .schema_filter
                                    .as_ref()
                                    .is_some_and(|filter| *filter == schema_filter.clone())
                            {
                                base.background
                            } else {
                                button::subtle(theme, status).background
                            }
                        },
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

    fn view_details<'a>(
        &'a self,
        metadata: &'a Option<Metadata>,
    ) -> Element<'a, SchemaExplorerMessage> {
        let element: Element<'a, SchemaExplorerMessage> = match metadata.as_ref() {
            Some(metadata) => view_metadata_details(metadata).into(),
            None => space().into(),
        };
        container(element).into()
    }

    fn view_schema_list<'a>(
        &'a self,
        database_metadata: &'a DatabaseMetadata,
        schema_filter: &'a Option<SchemaFilter>,
    ) -> Element<'a, SchemaExplorerMessage> {
        Column::from_iter(database_metadata.schemas.iter().map(|metadata| {
            schema_filter
                .as_ref()
                .map(|filter| match filter {
                    SchemaFilter::Schemas if let Metadata::Schema(schema) = metadata => {
                        view_schema_metadata(schema)
                    }
                    SchemaFilter::Tables if let Metadata::Table(metadata) = metadata => {
                        view_table_metadata(metadata)
                    }
                    SchemaFilter::Columns if let Metadata::Column(metadata) = metadata => {
                        view_column_metadata(metadata)
                    }
                    _ => space().into(),
                })
                .unwrap_or(match metadata {
                    Metadata::Schema(schema_metadata) => view_schema_metadata(schema_metadata),
                    Metadata::Table(table_metadata) => view_table_metadata(table_metadata),
                    Metadata::Column(column_metadata) => view_column_metadata(column_metadata),
                })
        }))
        .spacing(0)
        .into()
    }

    fn view_browse<'a>(&'a self, browse: &'a Browse) -> Element<'a, SchemaExplorerMessage> {
        let Browse {
            connection,
            schema,
            metadata,
            filtered_metadata,
            search,
            loading,
            error,
            selected_metadata,
            schema_filter,
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
            self.view_schema_list(filtered_metadata, schema_filter)
                .into()
        };

        let search_input = container(
            text_input("Search tables and columns...", search.as_str())
                .on_input(SchemaExplorerMessage::SearchChanged)
                .style(|theme, status| text_input::Style {
                    border: iced::Border::default().color(Color::TRANSPARENT),
                    background: iced::Background::Color(Color::TRANSPARENT).into(),
                    ..text_input::default(theme, status)
                })
                .id(SEARCH_QUERY_INPUT_ID)
                .size(12)
                .padding([8, 4]),
        )
        .width(Length::Fill)
        .padding([0, 4]);

        let form = column![
            column![
                search_input,
                rule::horizontal(1),
                self.view_quick_filter(browse),
            ],
            row![
                scrollable(container(list).padding([4, 4]))
                    .direction(scrollable::Direction::Vertical(
                        scrollable::Scrollbar::new().width(4).scroller_width(4),
                    ))
                    .height(Length::Fill)
                    .width(Length::FillPortion(1)),
                rule::vertical(1),
                container(self.view_details(selected_metadata))
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

fn view_metadata_details<'a>(metadata: &'a Metadata) -> Element<'a, SchemaExplorerMessage> {
    match metadata {
        Metadata::Schema(schema_metadata) => text(schema_metadata.name.as_str()).into(),
        Metadata::Table(table_metadata) => view_table_details(table_metadata).into(),
        Metadata::Column(column_metadata) => view_column_details(column_metadata).into(),
    }
}

fn view_column_details<'a>(
    column_metadata: &'a ColumnMetadata,
) -> Element<'a, SchemaExplorerMessage> {
    let default_text = column_metadata
        .column_default
        .as_deref()
        .unwrap_or("(none)");

    let mut rows: Vec<Element<'_, SchemaExplorerMessage>> = vec![
        detail_row("Type", column_metadata.data_type.as_str()),
        detail_row(
            "Nullable",
            if column_metadata.nullable {
                "Yes"
            } else {
                "No"
            },
        ),
        detail_row(
            "Primary key",
            if column_metadata.is_primary_key {
                "Yes"
            } else {
                "No"
            },
        ),
        detail_row("Default", default_text),
    ];

    if let Some(reference) = &column_metadata.references {
        rows.push(detail_row(
            "Foreign key",
            format!(
                "{}.{}.{}",
                reference.schema, reference.table, reference.column
            ),
        ));
    }

    if let Some(length) = column_metadata.character_maximum_length {
        rows.push(detail_row("Max length", length.to_string()));
    }

    container(
        column![
            text(column_metadata.name.as_str()).size(16),
            text(format!(
                "{}.{}",
                column_metadata.schema_name, column_metadata.table_name
            ))
            .size(12)
            .style(text::secondary),
            space::vertical().height(8),
            Column::from_iter(rows).spacing(4),
        ]
        .spacing(4),
    )
    .padding(8)
    .into()
}

fn view_table_details<'a>(table_metadata: &'a TableMetadata) -> Element<'a, SchemaExplorerMessage> {
    let primary_key_text = if table_metadata.primary_key.is_empty() {
        "(none)".to_string()
    } else {
        format!("({})", table_metadata.primary_key.join(", "))
    };

    let mut columns: Vec<Element<'_, SchemaExplorerMessage>> = vec![section_label("Columns")];
    columns.extend(table_metadata.columns.iter().map(view_table_column_row));

    let mut constraints: Vec<Element<'_, SchemaExplorerMessage>> = vec![
        section_label("Constraints"),
        detail_row("Primary key", primary_key_text),
    ];
    for foreign_key in &table_metadata.foreign_keys {
        constraints.push(detail_row(
            format!("FK {}", foreign_key.column),
            format!(
                "{}.{}.{}",
                foreign_key.references.schema,
                foreign_key.references.table,
                foreign_key.references.column
            ),
        ));
    }

    let column_details = table_metadata.columns.iter().map(view_column_details);

    container(
        scrollable(
            column![
                text(format!("{}.{}", table_metadata.schema, table_metadata.name)).size(16),
                space::vertical().height(8),
                Column::from_iter(columns).spacing(3),
                space::vertical().height(12),
                Column::from_iter(constraints).spacing(3),
                Column::from_iter(column_details).spacing(3)
            ]
            .spacing(4),
        )
        .height(Length::Fill)
        .width(Length::Fill),
    )
    .padding(8)
    .into()
}

fn view_table_column_row(column: &ColumnMetadata) -> Element<'_, SchemaExplorerMessage> {
    let mut badges: Vec<Element<'_, SchemaExplorerMessage>> = Vec::new();
    if column.is_primary_key {
        badges.push(key_badge("PK", Color::from_rgb8(196, 156, 54)));
    }
    if column.is_foreign_key {
        badges.push(key_badge("FK", Color::from_rgb8(54, 156, 140)));
    }

    row![
        text(column.name.as_str()).size(12),
        space::horizontal().width(6),
        text(column.data_type.as_str())
            .size(12)
            .style(text::secondary),
    ]
    .extend(badges)
    .spacing(4)
    .align_y(Alignment::Center)
    .into()
}

fn key_badge<'a>(label: &'a str, color: Color) -> Element<'a, SchemaExplorerMessage> {
    container(text(label).size(9).color(Color::WHITE))
        .style(move |_| container::Style {
            background: Some(color.into()),
            border: iced::Border {
                radius: 3.0.into(),
                width: 0.0,
                color: iced::Color::TRANSPARENT,
            },
            ..Default::default()
        })
        .padding([0, 3])
        .into()
}

fn detail_row(
    label: impl Into<String>,
    value: impl Into<String>,
) -> Element<'static, SchemaExplorerMessage> {
    row![
        text(label.into()).size(12).style(text::secondary),
        space::horizontal().width(8),
        text(value.into()).size(12),
    ]
    .spacing(4)
    .align_y(Alignment::Center)
    .into()
}

fn section_label<'a>(label: &'a str) -> Element<'a, SchemaExplorerMessage> {
    text(label)
        .size(11)
        .style(|theme| text::secondary(theme))
        .into()
}

fn view_column_metadata(column_metadata: &ColumnMetadata) -> Element<'_, SchemaExplorerMessage> {
    button(
        row![
            container(text("col").size(10))
                .style(|_| container::Style {
                    background: Some(Color::from_rgb8(64, 105, 153).into()),
                    border: iced::Border {
                        radius: 5.0.into(),
                        width: 0.0,
                        color: iced::Color::TRANSPARENT,
                    },
                    ..Default::default()
                })
                .padding([0, 4]),
            space::horizontal().width(4),
            text(format!(
                "{}.{}",
                column_metadata.schema_name.as_str(),
                column_metadata.table_name.as_str()
            ))
            .style(|theme| text::secondary(theme))
            .size(12),
            space::horizontal().width(4),
            text(column_metadata.name.as_str()).size(12),
            space::horizontal().width(4),
            text(column_metadata.data_type.as_str())
                .size(12)
                .style(text::secondary)
        ]
        .align_y(Alignment::Center)
        .wrap(),
    )
    .padding([2, 4])
    .style(|theme, status| {
        let background_color = match status {
            button::Status::Active => Color::TRANSPARENT,
            _ => theme.palette().background.weak.color,
        };
        button::Style {
            background: Some(background_color.into()),
            border: iced::Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: border::radius(5.0),
            },
            ..button::text(theme, status)
        }
    })
    .on_press(SchemaExplorerMessage::SelectDetails(Metadata::Column(
        column_metadata.clone(),
    )))
    .into()
}

fn view_table_metadata(table_metadata: &TableMetadata) -> Element<'_, SchemaExplorerMessage> {
    button(
        row![
            container(text("table").size(10))
                .style(|_| container::Style {
                    background: Some(Color::from_rgb8(110, 64, 153).into()),
                    border: iced::Border {
                        radius: 5.0.into(),
                        width: 0.0,
                        color: iced::Color::TRANSPARENT,
                    },
                    ..Default::default()
                })
                .padding([0, 4]),
            space::horizontal().width(4),
            text(table_metadata.schema.as_str())
                .style(text::secondary)
                .size(12),
            space::horizontal().width(4),
            text(table_metadata.name.as_str()).size(12)
        ]
        .align_y(Alignment::Center)
        .wrap(),
    )
    .on_press(SchemaExplorerMessage::SelectDetails(Metadata::Table(
        table_metadata.clone(),
    )))
    .padding([2, 4])
    .style(|theme, status| {
        let background_color = match status {
            button::Status::Active => Color::TRANSPARENT,
            _ => theme.palette().background.weak.color,
        };
        button::Style {
            background: Some(background_color.into()),
            border: iced::Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: border::radius(5.0),
            },
            ..button::text(theme, status)
        }
    })
    .into()
}

fn view_schema_metadata(schema_metadata: &SchemaMetadata) -> Element<'_, SchemaExplorerMessage> {
    row![
        container(text("schema").size(10))
            .style(|_| container::Style {
                background: Some(Color::from_rgb8(153, 64, 64).into()),
                border: iced::Border {
                    radius: 5.0.into(),
                    width: 0.0,
                    color: iced::Color::TRANSPARENT,
                },
                ..Default::default()
            })
            .padding([0, 4]),
        space::horizontal().width(4),
        text(schema_metadata.name.as_str()).size(12)
    ]
    .align_y(Alignment::Center)
    .padding([2, 4])
    .into()
}

fn filter_connections(names: &[String], query: &str) -> Vec<String> {
    let needle = query.to_lowercase();
    names
        .iter()
        .filter(|name| name.to_lowercase().contains(&needle))
        .cloned()
        .collect()
}
