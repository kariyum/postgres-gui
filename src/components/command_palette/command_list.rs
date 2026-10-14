use iced::widget::Id;
use iced::{Element, Task};

use crate::widgets::palette::{Palette, PaletteMessage};

pub const INPUT_ID: &str = "command_list_input";

#[derive(Debug, Clone, Copy)]
pub enum CommandAction {
    ExploreSchema,
    OpenConnectionQueryEditor,
    Settings,
    Quit,
    ToggleAssistant,
}

#[derive(Debug, Clone)]
pub enum Message {
    Palette(PaletteMessage),
    Run(CommandAction),
}

#[derive(Debug)]
struct CommandItem {
    label: &'static str,
    action: CommandAction,
}

const COMMANDS: &[CommandItem] = &[
    CommandItem {
        label: "Explore schema",
        action: CommandAction::ExploreSchema,
    },
    CommandItem {
        label: "Open Connection Query Editor",
        action: CommandAction::OpenConnectionQueryEditor,
    },
    CommandItem {
        label: "Settings",
        action: CommandAction::Settings,
    },
    CommandItem {
        label: "Quit",
        action: CommandAction::Quit,
    },
    CommandItem {
        label: "Toggle assistant",
        action: CommandAction::ToggleAssistant,
    },
];

#[derive(Debug, Clone)]
pub struct CommandList {
    search_query: String,
}

impl CommandList {
    pub fn new() -> Self {
        Self {
            search_query: String::new(),
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let filtered = filter_commands(&self.search_query);
        let labels: Vec<&str> = filtered.iter().map(|command| command.label).collect();
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
                let filtered = filter_commands(&self.search_query);
                match filtered.get(index) {
                    Some(command) => Task::done(Message::Run(command.action)),
                    None => Task::none(),
                }
            }
            Message::Run(_) => Task::none(),
        }
    }
}

fn filter_commands(search_query: &str) -> Vec<&'static CommandItem> {
    let query = search_query.to_lowercase();
    COMMANDS
        .iter()
        .filter(|command| command.label.to_lowercase().contains(&query))
        .collect()
}
