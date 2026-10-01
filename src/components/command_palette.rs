use iced::{Element, Task};

use crate::widgets::palette::{Palette, PaletteMessage};

#[derive(Debug, Clone)]
pub struct CommandPalette {
    is_visible: bool,
    search_query: String,
    commands: Vec<CommandItem>,
    filtered_commands: Vec<CommandItem>,
}

impl Default for CommandPalette {
    fn default() -> Self {
        let commands = vec![
            CommandItem {
                label: String::from("Explore schema"),
                action: CommandAction::ExploreSchema,
            },
            CommandItem {
                label: String::from("Connect to a database"),
                action: CommandAction::ConnectTo,
            },
            CommandItem {
                label: String::from("Settings"),
                action: CommandAction::Settings,
            },
            CommandItem {
                label: String::from("Quit"),
                action: CommandAction::Quit,
            },
            CommandItem {
                label: String::from("Toggle assistant"),
                action: CommandAction::ToggleAssistant,
            },

        ];

        Self {
            is_visible: false,
            search_query: String::new(),
            commands: commands.clone(),
            filtered_commands: commands,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    Toggle,
    Hide,
    Palette(PaletteMessage),
    ExploreSchema,
}

#[derive(Debug, Clone)]
pub enum CommandAction {
    ExploreSchema,
    ConnectTo,
    Settings,
    Quit,
    ToggleAssistant,
}

#[derive(Debug, Clone)]
pub struct CommandItem {
    pub label: String,
    pub action: CommandAction,
}

impl CommandPalette {
    pub fn view(&self) -> Option<Element<'_, Message>> {
        self.is_visible.then(|| {
            let labels: Vec<&str> = self
                .filtered_commands
                .iter()
                .map(|command| command.label.as_str())
                .collect();

            let palette: Element<'_, PaletteMessage> =
                Palette::new(&self.search_query, labels).into();

            palette.map(Message::Palette)
        })
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Toggle => {
                self.is_visible = !self.is_visible;
                self.search_query.clear();
                self.refresh_filtered_commands();
                Task::none()
            }
            Message::Hide => {
                self.is_visible = false;
                self.search_query.clear();
                self.refresh_filtered_commands();
                Task::none()
            }
            Message::Palette(palette_message) => match palette_message {
                PaletteMessage::InputChanged(query) => {
                    self.search_query = query;
                    self.refresh_filtered_commands();
                    Task::none()
                }
                PaletteMessage::Activated(index) => {
                    if let Some(command) = self.filtered_commands.get(index) {
                        let action = command.action.clone();
                        self.handle_command(action)
                    } else {
                        Task::none()
                    }
                }
            },
            Message::ExploreSchema => Task::none(),
        }
    }

    fn handle_command(&mut self, command: CommandAction) -> Task<Message> {
        match command {
            CommandAction::ExploreSchema => {
                self.is_visible = false;
                self.search_query.clear();
                self.refresh_filtered_commands();
                Task::done(Message::ExploreSchema)
            }
            CommandAction::ConnectTo => Task::none(),
            CommandAction::Settings => Task::none(),
            CommandAction::Quit => todo!(),
            CommandAction::ToggleAssistant => todo!(),
        }
    }

    fn refresh_filtered_commands(&mut self) {
        self.filtered_commands = filter_commands(&self.commands, &self.search_query);
    }
}

fn filter_commands(commands: &[CommandItem], search_query: &str) -> Vec<CommandItem> {
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
