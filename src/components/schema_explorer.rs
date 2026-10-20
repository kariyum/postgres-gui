use iced::futures::channel::mpsc::Sender;
use iced::widget::{button, column, row, rule, space, text};
use iced::{Alignment, Element, Length, Task};

use crate::core::database_keeper::{self, DatabaseKeeperMessage};
use crate::types::Schema;
use crate::widgets::palette::PaletteMessage;

mod connection_picker;
mod schema_browser;

use connection_picker::ConnectionPicker;
use schema_browser::{SchemaBrowser, SchemaBrowserMessage};

#[derive(Debug, Clone)]
pub struct SchemaExplorer {
    names: Vec<String>,
    picker: Option<ConnectionPicker>,
    stage: Stage,
    database_keeper: Option<Sender<DatabaseKeeperMessage>>,
}

#[derive(Debug, Clone)]
enum Stage {
    Empty,
    Browse(SchemaBrowser),
}

#[derive(Debug, Clone)]
pub enum SchemaExplorerMessage {
    ConnectionPalette(PaletteMessage),
    SchemaLoaded {
        connection: String,
        result: Result<Schema, String>,
    },
    ClosePicker,
    OpenPicker,
    RefreshSchema,
    Browser(SchemaBrowserMessage),
}

impl Default for SchemaExplorer {
    fn default() -> Self {
        Self {
            names: Vec::new(),
            picker: None,
            stage: Stage::Empty,
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
        self.database_keeper = database_keeper;
        self.names = names;

        let keeps_browse = matches!(
            &self.stage,
            Stage::Browse(browser) if self.names.iter().any(|name| name == browser.connection())
        );
        if !keeps_browse {
            self.stage = Stage::Empty;
        }

        if matches!(self.stage, Stage::Browse(_)) {
            Task::none()
        } else {
            self.open_picker()
        }
    }

    pub fn open_picker(&mut self) -> Task<SchemaExplorerMessage> {
        self.picker = Some(ConnectionPicker::new(self.names.clone()));
        connection_picker::focus()
    }

    pub fn close_picker(&mut self) {
        self.picker = None;
    }

    pub fn is_picking(&self) -> bool {
        self.picker.is_some()
    }

    pub fn is_browsing(&self) -> bool {
        matches!(self.stage, Stage::Browse(_))
    }

    pub fn update(&mut self, message: SchemaExplorerMessage) -> Task<SchemaExplorerMessage> {
        match message {
            SchemaExplorerMessage::ConnectionPalette(message) => {
                let selected = self
                    .picker
                    .as_mut()
                    .and_then(|picker| picker.update(message));
                match selected {
                    Some(name) => self.select_connection(name),
                    None => Task::none(),
                }
            }
            SchemaExplorerMessage::SchemaLoaded { connection, result } => {
                self.handle_schema_loaded(connection, result);
                Task::none()
            }
            SchemaExplorerMessage::ClosePicker => {
                self.close_picker();
                Task::none()
            }
            SchemaExplorerMessage::OpenPicker => self.open_picker(),
            SchemaExplorerMessage::RefreshSchema => self.refresh_schema(),
            SchemaExplorerMessage::Browser(message) => match &mut self.stage {
                Stage::Browse(browser) => {
                    browser.update(message).map(SchemaExplorerMessage::Browser)
                }
                Stage::Empty => Task::none(),
            },
        }
    }

    fn refresh_schema(&mut self) -> Task<SchemaExplorerMessage> {
        let Some(mut actor) = self.database_keeper.clone() else {
            return Task::none();
        };
        let Stage::Browse(browser) = &mut self.stage else {
            return Task::none();
        };
        if browser.is_loading() {
            return Task::none();
        }

        let connection = browser.connection().to_string();
        let reset = browser.begin_refresh().map(SchemaExplorerMessage::Browser);
        let loaded_connection = connection.clone();
        Task::batch([
            reset,
            Task::perform(
                async move {
                    database_keeper::refresh_schema(&mut actor, &connection)
                        .await
                        .map_err(|e| e.to_string())
                },
                move |result| SchemaExplorerMessage::SchemaLoaded {
                    connection: loaded_connection,
                    result,
                },
            ),
        ])
    }

    fn select_connection(&mut self, name: String) -> Task<SchemaExplorerMessage> {
        self.picker = None;
        let Some(mut actor) = self.database_keeper.clone() else {
            self.stage = Stage::Browse(SchemaBrowser::new(
                name,
                false,
                Some("Database service not ready".into()),
            ));
            return Task::none();
        };

        self.stage = Stage::Browse(SchemaBrowser::new(name.clone(), true, None));

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
            schema_browser::focus_search(),
        ])
    }

    fn handle_schema_loaded(&mut self, connection: String, result: Result<Schema, String>) {
        if let Stage::Browse(browser) = &mut self.stage {
            if browser.connection() == connection.as_str() {
                browser.apply_loaded(result);
            }
        }
    }

    pub fn view_picker(&self) -> Option<Element<'_, SchemaExplorerMessage>> {
        let picker = self.picker.as_ref()?;
        Some(picker.view().map(SchemaExplorerMessage::ConnectionPalette))
    }

    pub fn view_pane(&self) -> Element<'_, SchemaExplorerMessage> {
        let title = match &self.stage {
            Stage::Browse(browser) => browser.connection(),
            Stage::Empty => "Schema Explorer",
        };

        let switch_button: Element<'_, SchemaExplorerMessage> = if self.names.is_empty() {
            space::horizontal().into()
        } else {
            button(text("Switch connection").size(12))
                .on_press(SchemaExplorerMessage::OpenPicker)
                .padding([2, 8])
                .style(button::subtle)
                .into()
        };

        let refresh_button: Element<'_, SchemaExplorerMessage> = match &self.stage {
            Stage::Browse(browser) if !self.names.is_empty() => button(text("Refresh").size(12))
                .on_press_maybe(
                    (!browser.is_loading()).then_some(SchemaExplorerMessage::RefreshSchema),
                )
                .padding([2, 8])
                .style(button::subtle)
                .into(),
            _ => space::horizontal().into(),
        };

        let header = row![
            text(title).size(14),
            space::horizontal(),
            refresh_button,
            switch_button
        ]
        .spacing(4)
        .align_y(Alignment::Center);

        let body: Element<'_, SchemaExplorerMessage> = if self.names.is_empty() {
            text("No connections yet. Add a connection to explore its schema.")
                .size(13)
                .style(text::secondary)
                .into()
        } else {
            match &self.stage {
                Stage::Browse(browser) => browser.view().map(SchemaExplorerMessage::Browser),
                Stage::Empty => text("Choose a connection to explore its schema.")
                    .size(13)
                    .style(text::secondary)
                    .into(),
            }
        };

        column![header, rule::horizontal(1), body]
            .spacing(8)
            .padding(8)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }
}
