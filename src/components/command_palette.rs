use iced::{Element, Task};

use crate::components::palette::Palette;

#[derive(Debug, Clone)]
pub struct CommandPalette {
    is_visible: bool,
    search_query: String,
    commands: Vec<Command>,
    filtered_commands: Vec<Command>,
    selected_command_index: usize,
}

impl Default for CommandPalette {
    fn default() -> Self {
        let commands = vec![
            Command {
                label: String::from("Explore schema"),
                message: Message::ExploreSchema,
            },
            Command {
                label: String::from("Connect to"),
                message: Message::ConnectTo,
            },
            Command {
                label: String::from("Settings"),
                message: Message::Settings,
            },
        ];

        Self {
            is_visible: false,
            search_query: String::new(),
            commands: commands.clone(),
            filtered_commands: commands,
            selected_command_index: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    Toggle,
    Hide,
    InputChanged(String),
    SelectNext,
    SelectPrevious,
    ExploreSchema,
    ConnectTo,
    Settings,
}

#[derive(Debug, Clone)]
pub struct Command {
    pub label: String,
    pub message: Message,
}

impl CommandPalette {
    pub fn view(&self) -> Option<Element<'_, Message>> {
        self.is_visible.then(|| {
            let selected_index = if self.filtered_commands.is_empty() {
                0
            } else {
                self.selected_command_index
                    .min(self.filtered_commands.len() - 1)
            };

            Palette::new(
                &self.search_query,
                self.filtered_commands.clone(),
                selected_index,
            )
            .into()
        })
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Toggle => {
                self.is_visible = !self.is_visible;
                Task::none()
            }
            Message::Hide => {
                self.is_visible = false;
                self.search_query.clear();
                self.refresh_filtered_commands();
                Task::none()
            }
            Message::InputChanged(query) => {
                self.search_query = query;
                self.selected_command_index = 0;
                self.refresh_filtered_commands();
                Task::none()
            }
            Message::SelectNext => {
                let count = self.filtered_commands.len();
                if count > 0 {
                    self.selected_command_index = (self.selected_command_index + 1) % count;
                }
                Task::none()
            }
            Message::SelectPrevious => {
                let count = self.filtered_commands.len();
                if count > 0 {
                    self.selected_command_index = (self.selected_command_index + count - 1) % count;
                }
                Task::none()
            }
            Message::ExploreSchema => Task::none(),
            Message::ConnectTo => Task::none(),
            Message::Settings => Task::none(),
        }
    }

    fn refresh_filtered_commands(&mut self) {
        self.filtered_commands = filter_commands(&self.commands, &self.search_query);
    }
}

fn filter_commands(commands: &[Command], search_query: &str) -> Vec<Command> {
    commands
        .iter()
        .filter(|command| {
            command
                .label
                .to_lowercase()
                .contains(search_query.to_lowercase().as_str())
        })
        .map(|command| command.clone())
        .collect()
}
