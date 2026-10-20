use iced::widget::scrollable::AbsoluteOffset;
use iced::widget::{
    Column, Id, button, column, container, operation, responsive, row, rule, scrollable, space,
    text, text_input,
};
use iced::{Alignment, Color, Element, Length, Task, border};

use crate::types::{ForeignKeyRef, RoutineKind, Schema};

const SEARCH_QUERY_INPUT_ID: &str = "search_query_picker_input";
const LIST_ID: &str = "schema_browser_list";
/// Fixed height of every row in the schema list. Virtualization relies on it.
const ROW_HEIGHT: f32 = 22.0;
/// Extra rows rendered above and below the viewport to hide scroll gaps.
const OVERSCAN_ROWS: usize = 12;

#[derive(Debug, Clone, Default)]
pub struct DatabaseMetadata {
    pub schemas: Vec<Metadata>,
}

impl From<Schema> for DatabaseMetadata {
    fn from(schema: Schema) -> Self {
        let Schema {
            schemas: schema_infos,
            ..
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

            for routine in schema_info.routines {
                schemas.push(Metadata::Routine(RoutineMetadata {
                    schema: schema_info.name.clone(),
                    name: routine.name,
                    kind: routine.kind,
                    arguments: routine.arguments,
                    return_type: routine.return_type,
                    definition: routine.definition,
                }));
            }

            for index in schema_info.indexes {
                schemas.push(Metadata::Index(IndexMetadata {
                    schema: schema_info.name.clone(),
                    table: index.table,
                    name: index.name,
                    definition: index.definition,
                    is_unique: index.is_unique,
                }));
            }
        }

        Self { schemas }
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
    Routine(RoutineMetadata),
    Index(IndexMetadata),
}

#[derive(Debug, Clone)]
pub struct RoutineMetadata {
    schema: String,
    name: String,
    kind: RoutineKind,
    arguments: String,
    return_type: Option<String>,
    definition: String,
}

#[derive(Debug, Clone)]
pub struct IndexMetadata {
    schema: String,
    table: String,
    name: String,
    definition: String,
    is_unique: bool,
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
pub struct SchemaBrowser {
    connection: String,
    schema: Schema,
    metadata: DatabaseMetadata,
    /// Indices into `metadata.schemas` that pass the current filters.
    matches: Vec<usize>,
    scroll_offset: f32,
    search: String,
    loading: bool,
    schema_filter: Option<SchemaFilter>,
    selected_metadata: Option<Metadata>,
    error: Option<String>,
}

#[derive(Debug, Clone)]
pub enum SchemaBrowserMessage {
    SearchChanged(String),
    SchemaFilter(SchemaFilter),
    SelectDetails(Metadata),
    Scrolled(f32),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchemaFilter {
    Schemas,
    Tables,
    Columns,
    Functions,
    Procedures,
    Indexes,
}

pub fn focus_search<T>() -> Task<T> {
    operation::focus(Id::new(SEARCH_QUERY_INPUT_ID))
}

impl SchemaBrowser {
    pub fn new(connection: String, loading: bool, error: Option<String>) -> Self {
        Self {
            schema: Schema::new(connection.clone()),
            metadata: DatabaseMetadata::default(),
            matches: Vec::new(),
            scroll_offset: 0.0,
            connection,
            search: String::new(),
            loading,
            schema_filter: None,
            selected_metadata: None,
            error,
        }
    }

    pub fn connection(&self) -> &str {
        &self.connection
    }

    pub fn apply_loaded(&mut self, result: Result<Schema, String>) {
        match result {
            Ok(schema) => {
                self.metadata = DatabaseMetadata::from(schema.clone());
                self.schema = schema;
                self.loading = false;
                self.error = None;
                self.apply_filters();
            }
            Err(e) => {
                if e == "Schema is still loading" {
                    self.loading = true;
                } else {
                    self.loading = false;
                    self.error = Some(e);
                }
            }
        }
    }

    pub fn update(&mut self, message: SchemaBrowserMessage) -> Task<SchemaBrowserMessage> {
        match message {
            SchemaBrowserMessage::SearchChanged(search) => {
                self.search = search;
                self.apply_filters();
                self.reset_scroll()
            }
            SchemaBrowserMessage::SchemaFilter(filter) => {
                if self.schema_filter.as_ref() == Some(&filter) {
                    self.schema_filter = None;
                } else {
                    self.schema_filter = Some(filter);
                }
                self.apply_filters();
                self.reset_scroll()
            }
            SchemaBrowserMessage::SelectDetails(metadata) => {
                self.selected_metadata = Some(metadata);
                Task::none()
            }
            SchemaBrowserMessage::Scrolled(offset) => {
                self.scroll_offset = offset;
                Task::none()
            }
        }
    }

    fn apply_filters(&mut self) {
        self.matches = filter_indices(&self.metadata, &self.search, self.schema_filter.as_ref());
    }

    fn reset_scroll(&mut self) -> Task<SchemaBrowserMessage> {
        self.scroll_offset = 0.0;
        operation::scroll_to(
            Id::new(LIST_ID),
            AbsoluteOffset {
                x: None,
                y: Some(0.0),
            },
        )
    }

    pub fn view(&self) -> Element<'_, SchemaBrowserMessage> {
        let SchemaBrowser {
            search,
            loading,
            error,
            selected_metadata,
            ..
        } = self;
        let list: Element<'_, SchemaBrowserMessage> = if *loading {
            scrollable(text("Loading schema...").size(13).style(text::secondary)).into()
        } else if let Some(err) = error {
            scrollable(text(err.as_str()).size(13).color(crate::theme::DANGER)).into()
        } else if self.matches.is_empty() {
            scrollable(text("No results.").size(13).style(text::secondary)).into()
        } else {
            responsive(move |size| self.view_list(size.height)).into()
        };

        let search_input = container(
            text_input("Search tables and columns...", search.as_str())
                .on_input(SchemaBrowserMessage::SearchChanged)
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
            column![search_input, rule::horizontal(1), self.view_quick_filter(),],
            row![
                container(list)
                    .height(Length::Fill)
                    .width(Length::FillPortion(1)),
                rule::vertical(1),
                container(self.view_details(selected_metadata))
                    .padding([0, 8])
                    .height(Length::Fill)
                    .width(Length::FillPortion(1)),
            ]
            .height(Length::Fill)
        ]
        .spacing(8)
        .padding(0)
        .width(Length::Fill)
        .height(Length::Fill);

        form.into()
    }

    fn view_quick_filter(&self) -> Element<'_, SchemaBrowserMessage> {
        let filters: Vec<Element<'_, SchemaBrowserMessage>> = vec![
            SchemaFilter::Schemas,
            SchemaFilter::Tables,
            SchemaFilter::Columns,
            SchemaFilter::Functions,
            SchemaFilter::Procedures,
            SchemaFilter::Indexes,
        ]
        .into_iter()
        .map(|schema_filter| {
            let title = match schema_filter {
                SchemaFilter::Schemas => "Schemas",
                SchemaFilter::Tables => "Tables",
                SchemaFilter::Columns => "Columns",
                SchemaFilter::Functions => "Functions",
                SchemaFilter::Procedures => "Procedures",
                SchemaFilter::Indexes => "Indexes",
            };

            button(text(title).size(12))
                .on_press(SchemaBrowserMessage::SchemaFilter(schema_filter.clone()))
                .padding([0, 8])
                .style(move |theme, status| {
                    let base = button::primary(theme, status);
                    button::Style {
                        border: border::Border::default().rounded(5.0),
                        text_color: {
                            if matches!(status, button::Status::Active | button::Status::Hovered)
                                && self
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
                                && self
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
    ) -> Element<'a, SchemaBrowserMessage> {
        let element: Element<'a, SchemaBrowserMessage> = match metadata.as_ref() {
            Some(metadata) => view_metadata_details(metadata).into(),
            None => space().into(),
        };
        container(element).into()
    }

    /// Builds only the rows inside the viewport (plus overscan). The rows above and
    /// below are replaced by spacers so the scrollable keeps its full height.
    fn view_list(&self, viewport_height: f32) -> Element<'_, SchemaBrowserMessage> {
        let total = self.matches.len();
        let max_offset = (total as f32 * ROW_HEIGHT - viewport_height).max(0.0);
        let first = (self.scroll_offset.min(max_offset) / ROW_HEIGHT) as usize;
        let visible = (viewport_height / ROW_HEIGHT).ceil() as usize;
        let end = (first + visible + OVERSCAN_ROWS).min(total);
        let start = first.saturating_sub(OVERSCAN_ROWS).min(end);

        let rows = self.matches[start..end]
            .iter()
            .map(|&index| view_metadata_row(&self.metadata.schemas[index]));

        scrollable(
            container(
                column![
                    space::vertical().height(start as f32 * ROW_HEIGHT),
                    Column::from_iter(rows),
                    space::vertical().height((total - end) as f32 * ROW_HEIGHT),
                ]
                .spacing(0),
            )
            .padding([4, 4]),
        )
        .id(Id::new(LIST_ID))
        .on_scroll(|viewport| SchemaBrowserMessage::Scrolled(viewport.absolute_offset().y))
        .direction(scrollable::Direction::Vertical(
            scrollable::Scrollbar::new().width(4).scroller_width(4),
        ))
        .height(Length::Fill)
        .width(Length::Fill)
        .into()
    }
}

fn view_metadata_row(metadata: &Metadata) -> Element<'_, SchemaBrowserMessage> {
    let row: Element<'_, SchemaBrowserMessage> = match metadata {
        Metadata::Schema(schema_metadata) => view_schema_metadata(schema_metadata),
        Metadata::Table(table_metadata) => view_table_metadata(table_metadata),
        Metadata::Column(column_metadata) => view_column_metadata(column_metadata),
        Metadata::Routine(routine_metadata) => view_routine_metadata(routine_metadata),
        Metadata::Index(index_metadata) => view_index_metadata(index_metadata),
    };
    container(row)
        .height(Length::Fixed(ROW_HEIGHT))
        .align_y(Alignment::Center)
        .into()
}

fn view_metadata_details<'a>(metadata: &'a Metadata) -> Element<'a, SchemaBrowserMessage> {
    match metadata {
        Metadata::Schema(schema_metadata) => text(schema_metadata.name.as_str()).into(),
        Metadata::Table(table_metadata) => view_table_details(table_metadata).into(),
        Metadata::Column(column_metadata) => view_column_details(column_metadata).into(),
        Metadata::Routine(routine_metadata) => view_routine_details(routine_metadata).into(),
        Metadata::Index(index_metadata) => view_index_details(index_metadata).into(),
    }
}

fn view_routine_details<'a>(
    routine_metadata: &'a RoutineMetadata,
) -> Element<'a, SchemaBrowserMessage> {
    let mut rows: Vec<Element<'_, SchemaBrowserMessage>> = vec![
        detail_row("Kind", routine_kind_label(routine_metadata.kind)),
        detail_row(
            "Arguments",
            if routine_metadata.arguments.is_empty() {
                "(none)"
            } else {
                routine_metadata.arguments.as_str()
            },
        ),
    ];
    if let Some(return_type) = &routine_metadata.return_type {
        rows.push(detail_row("Returns", return_type.as_str()));
    }

    container(
        scrollable(
            column![
                text(routine_metadata.name.as_str()).size(16),
                text(routine_metadata.schema.as_str())
                    .size(12)
                    .style(text::secondary),
                space::vertical().height(8),
                Column::from_iter(rows).spacing(4),
                space::vertical().height(8),
                section_label("Definition"),
                text(routine_metadata.definition.as_str()).size(12),
            ]
            .spacing(4),
        )
        .height(Length::Fill)
        .width(Length::Fill),
    )
    .padding(8)
    .into()
}

fn view_index_details<'a>(index_metadata: &'a IndexMetadata) -> Element<'a, SchemaBrowserMessage> {
    container(
        scrollable(
            column![
                text(index_metadata.name.as_str()).size(16),
                text(format!(
                    "{}.{}",
                    index_metadata.schema, index_metadata.table
                ))
                .size(12)
                .style(text::secondary),
                space::vertical().height(8),
                Column::from_iter(vec![
                    detail_row("Table", index_metadata.table.as_str()),
                    detail_row(
                        "Unique",
                        if index_metadata.is_unique {
                            "Yes"
                        } else {
                            "No"
                        },
                    ),
                ])
                .spacing(4),
                space::vertical().height(8),
                section_label("Definition"),
                text(index_metadata.definition.as_str()).size(12),
            ]
            .spacing(4),
        )
        .height(Length::Fill)
        .width(Length::Fill),
    )
    .padding(8)
    .into()
}

fn view_column_details<'a>(
    column_metadata: &'a ColumnMetadata,
) -> Element<'a, SchemaBrowserMessage> {
    let default_text = column_metadata
        .column_default
        .as_deref()
        .unwrap_or("(none)");

    let mut rows: Vec<Element<'_, SchemaBrowserMessage>> = vec![
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

fn view_table_details<'a>(table_metadata: &'a TableMetadata) -> Element<'a, SchemaBrowserMessage> {
    let primary_key_text = if table_metadata.primary_key.is_empty() {
        "(none)".to_string()
    } else {
        format!("({})", table_metadata.primary_key.join(", "))
    };

    let mut columns: Vec<Element<'_, SchemaBrowserMessage>> = vec![section_label("Columns")];
    columns.extend(table_metadata.columns.iter().map(view_table_column_row));

    let mut constraints: Vec<Element<'_, SchemaBrowserMessage>> = vec![
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

fn view_table_column_row(column: &ColumnMetadata) -> Element<'_, SchemaBrowserMessage> {
    let mut badges: Vec<Element<'_, SchemaBrowserMessage>> = Vec::new();
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

fn key_badge<'a>(label: &'a str, color: Color) -> Element<'a, SchemaBrowserMessage> {
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
) -> Element<'static, SchemaBrowserMessage> {
    row![
        text(label.into()).size(12).style(text::secondary),
        space::horizontal().width(8),
        text(value.into()).size(12),
    ]
    .spacing(4)
    .align_y(Alignment::Center)
    .into()
}

fn section_label<'a>(label: &'a str) -> Element<'a, SchemaBrowserMessage> {
    text(label)
        .size(11)
        .style(|theme| text::secondary(theme))
        .into()
}

fn view_column_metadata(column_metadata: &ColumnMetadata) -> Element<'_, SchemaBrowserMessage> {
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
        .align_y(Alignment::Center),
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
    .on_press(SchemaBrowserMessage::SelectDetails(Metadata::Column(
        column_metadata.clone(),
    )))
    .into()
}

fn view_table_metadata(table_metadata: &TableMetadata) -> Element<'_, SchemaBrowserMessage> {
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
        .align_y(Alignment::Center),
    )
    .on_press(SchemaBrowserMessage::SelectDetails(Metadata::Table(
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

fn view_schema_metadata(schema_metadata: &SchemaMetadata) -> Element<'_, SchemaBrowserMessage> {
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

fn routine_kind_label(kind: RoutineKind) -> &'static str {
    match kind {
        RoutineKind::Function => "Function",
        RoutineKind::Procedure => "Procedure",
    }
}

fn view_routine_metadata(routine_metadata: &RoutineMetadata) -> Element<'_, SchemaBrowserMessage> {
    let (label, color) = match routine_metadata.kind {
        RoutineKind::Function => ("fn", Color::from_rgb8(46, 125, 84)),
        RoutineKind::Procedure => ("proc", Color::from_rgb8(176, 96, 40)),
    };
    view_named_row(
        label,
        color,
        routine_metadata.schema.clone(),
        routine_metadata.name.clone(),
        Metadata::Routine(routine_metadata.clone()),
    )
}

fn view_index_metadata(index_metadata: &IndexMetadata) -> Element<'_, SchemaBrowserMessage> {
    view_named_row(
        "idx",
        Color::from_rgb8(90, 90, 160),
        format!("{}.{}", index_metadata.schema, index_metadata.table),
        index_metadata.name.clone(),
        Metadata::Index(index_metadata.clone()),
    )
}

fn view_named_row(
    label: &'static str,
    color: Color,
    qualifier: String,
    name: String,
    metadata: Metadata,
) -> Element<'static, SchemaBrowserMessage> {
    button(
        row![
            container(text(label).size(10))
                .style(move |_| container::Style {
                    background: Some(color.into()),
                    border: iced::Border {
                        radius: 5.0.into(),
                        width: 0.0,
                        color: iced::Color::TRANSPARENT,
                    },
                    ..Default::default()
                })
                .padding([0, 4]),
            space::horizontal().width(4),
            text(qualifier).style(text::secondary).size(12),
            space::horizontal().width(4),
            text(name).size(12),
        ]
        .align_y(Alignment::Center),
    )
    .on_press(SchemaBrowserMessage::SelectDetails(metadata))
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

fn filter_indices(
    metadata: &DatabaseMetadata,
    search: &str,
    schema_filter: Option<&SchemaFilter>,
) -> Vec<usize> {
    let needle = search.to_lowercase();
    metadata
        .schemas
        .iter()
        .enumerate()
        .filter(|(_, item)| match schema_filter {
            None => true,
            Some(SchemaFilter::Schemas) => matches!(item, Metadata::Schema(_)),
            Some(SchemaFilter::Tables) => matches!(item, Metadata::Table(_)),
            Some(SchemaFilter::Columns) => matches!(item, Metadata::Column(_)),
            Some(SchemaFilter::Functions) => {
                matches!(item, Metadata::Routine(routine) if routine.kind == RoutineKind::Function)
            }
            Some(SchemaFilter::Procedures) => {
                matches!(item, Metadata::Routine(routine) if routine.kind == RoutineKind::Procedure)
            }
            Some(SchemaFilter::Indexes) => matches!(item, Metadata::Index(_)),
        })
        .filter(|(_, item)| needle.is_empty() || matches_search(item, &needle))
        .map(|(index, _)| index)
        .collect()
}

fn matches_search(metadata: &Metadata, needle: &str) -> bool {
    let contains = |value: &str| value.to_lowercase().contains(needle);
    match metadata {
        Metadata::Schema(schema) => contains(&schema.name),
        Metadata::Table(table) => {
            contains(&table.name) || contains(&format!("{}.{}", table.schema, table.name))
        }
        Metadata::Column(column) => {
            contains(&column.name)
                || contains(&format!("{}.{}", column.schema_name, column.table_name))
        }
        Metadata::Routine(routine) => {
            contains(&routine.name) || contains(&format!("{}.{}", routine.schema, routine.name))
        }
        Metadata::Index(index) => {
            contains(&index.name) || contains(&format!("{}.{}", index.schema, index.table))
        }
    }
}
