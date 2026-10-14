use iced::widget::{Id, container, text};
use iced::{Border, Element, Task, Theme};

use crate::components::connection_config::ConnectionConfig;
use crate::widgets::palette::{Palette, PaletteMessage};

pub const INPUT_ID: &str = "connection_picker_input";

#[derive(Debug, Clone)]
pub enum Message {
    Palette(PaletteMessage),
    Selected(ConnectionConfig),
}

#[derive(Debug, Clone)]
pub struct ConnectionPicker {
    search_query: String,
    connections: Vec<ConnectionConfig>,
}

impl ConnectionPicker {
    pub fn new(connections: Vec<ConnectionConfig>) -> Self {
        Self {
            search_query: String::new(),
            connections,
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        if self.connections.is_empty() {
            return container(text("No connections configured").size(12))
                .padding([8, 12])
                .style(|theme: &Theme| container::Style {
                    background: Some(theme.palette().background.weak.color.into()),
                    border: Border::default().rounded(4),
                    ..Default::default()
                })
                .into();
        }

        let filtered = filter_connections(&self.connections, &self.search_query);
        let labels: Vec<&str> = filtered
            .iter()
            .map(|connection| connection.name.as_str())
            .collect();
        let palette: Element<'_, PaletteMessage> = Palette::new(&self.search_query, labels)
            .input_id(Id::new(INPUT_ID))
            .into();
        palette.map(Message::Palette)
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Palette(PaletteMessage::InputChanged(query)) => {
                self.search_query = query;
                Task::none()
            }
            Message::Palette(PaletteMessage::Activated(index)) => {
                let filtered = filter_connections(&self.connections, &self.search_query);
                match filtered.get(index) {
                    Some(connection) => Task::done(Message::Selected((*connection).clone())),
                    None => Task::none(),
                }
            }
            Message::Selected(_) => Task::none(),
        }
    }
}

fn filter_connections<'a>(
    connections: &'a [ConnectionConfig],
    search_query: &str,
) -> Vec<&'a ConnectionConfig> {
    let query = search_query.to_lowercase();
    connections
        .iter()
        .filter(|connection| connection.name.to_lowercase().contains(&query))
        .collect()
}
