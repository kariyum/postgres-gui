use iced::widget::{Id, operation};
use iced::{Element, Task};

use crate::widgets::palette::{Palette, PaletteMessage};

const CONNECTION_INPUT_ID: &str = "connection_picker_input";

#[derive(Debug, Clone)]
pub struct ConnectionPicker {
    names: Vec<String>,
    query: String,
    filtered: Vec<String>,
}

impl ConnectionPicker {
    pub fn new(names: Vec<String>) -> Self {
        Self {
            query: String::new(),
            filtered: names.clone(),
            names,
        }
    }

    /// Handles a palette message and returns the connection name when one is activated.
    pub fn update(&mut self, message: PaletteMessage) -> Option<String> {
        match message {
            PaletteMessage::InputChanged(query) => {
                self.filtered = filter_connections(&self.names, &query);
                self.query = query;
                None
            }
            PaletteMessage::Activated(index) => self.filtered.get(index).cloned(),
            PaletteMessage::Close => None,
        }
    }

    pub fn view(&self) -> Element<'_, PaletteMessage> {
        let labels: Vec<&str> = self.filtered.iter().map(|s| s.as_str()).collect();
        Palette::new(self.query.as_str(), labels)
            .input_id(Id::new(CONNECTION_INPUT_ID))
            .into()
    }
}

pub fn focus<T>() -> Task<T> {
    operation::focus(Id::new(CONNECTION_INPUT_ID))
}

fn filter_connections(names: &[String], query: &str) -> Vec<String> {
    let needle = query.to_lowercase();
    names
        .iter()
        .filter(|name| name.to_lowercase().contains(&needle))
        .cloned()
        .collect()
}
