use iced::widget::{button, text};
use iced::{Background, Element, Length, Task, Theme};

use crate::widgets::palette::Palette;

#[derive(Debug, Clone)]
pub struct CommandPalette {
    is_visible: bool,
    search_query: String,
    commands: Vec<CommandItem>,
    filtered_commands: Vec<CommandItem>,
    selected_command_index: usize,
}

impl Default for CommandPalette {
    fn default() -> Self {
        let commands = vec![
            CommandItem {
                label: String::from("Explore schema"),
                message: Message::Command(CommandAction::ExploreSchema),
            },
            CommandItem {
                label: String::from("Connect to"),
                message: Message::Command(CommandAction::ConnectTo),
            },
            CommandItem {
                label: String::from("Settings"),
                message: Message::Command(CommandAction::Settings),
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
    Hovered(usize),
    Command(CommandAction),
}

#[derive(Debug, Clone)]
pub enum CommandAction {
    ExploreSchema,
    ConnectTo,
    Settings,
}

#[derive(Debug, Clone)]
pub struct CommandItem {
    pub label: String,
    pub message: Message,
}

impl CommandPalette {
    pub fn view(&self) -> Option<Element<'_, Message>> {
        self.is_visible.then(|| {
            let selected_index = self
                .selected_command_index
                .min(self.filtered_commands.len().saturating_sub(1));

            let rows = self
                .filtered_commands
                .iter()
                .enumerate()
                .map(|(index, command)| {
                    let label = command.label.clone();
                    let message = command.message.clone();
                    let is_selected = index == selected_index;

                    button(text(label).size(12))
                        .on_press(message)
                        .width(Length::Fill)
                        .style(move |theme: &Theme, status| button::Style {
                            background: Some(Background::Color(if is_selected {
                                theme.palette().background.stronger.color
                            } else {
                                theme.palette().background.weak.color
                            })),
                            text_color: theme.palette().background.weak.text,
                            ..button::primary(theme, status)
                        })
                        .padding([4, 4])
                        .into()
                })
                .collect();

            Palette::new(&self.search_query, rows)
                .on_input(Message::InputChanged)
                .on_select_next(Message::SelectNext)
                .on_select_previous(Message::SelectPrevious)
                .on_hover(Message::Hovered)
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
            Message::Hovered(index) => {
                self.selected_command_index = index;
                Task::none()
            }
            Message::Command(command) => self.handle_command(command),
        }
    }

    fn handle_command(&mut self, command: CommandAction) -> Task<Message> {
        match command {
            CommandAction::ExploreSchema => Task::none(),
            CommandAction::ConnectTo => Task::none(),
            CommandAction::Settings => Task::none(),
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
