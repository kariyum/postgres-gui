use iced::widget::{button, operation, scrollable, text};
use iced::{Background, Element, Length, Task, Theme};

use crate::widgets::palette::{LIST_PADDING, Palette, ROW_HEIGHT, ROW_STRIDE, SCROLLABLE_ID};

#[derive(Debug, Clone)]
pub struct CommandPalette {
    is_visible: bool,
    search_query: String,
    commands: Vec<CommandItem>,
    filtered_commands: Vec<CommandItem>,
    selected_command_index: usize,
    scroll_offset: f32,
    viewport_height: f32,
}

impl Default for CommandPalette {
    fn default() -> Self {
        let commands = vec![
            CommandItem {
                label: String::from("Explore schema"),
                message: Message::Command(CommandAction::ExploreSchema),
            },
            CommandItem {
                label: String::from("Connect to a database"),
                message: Message::Command(CommandAction::ConnectTo),
            },
            CommandItem {
                label: String::from("Settings"),
                message: Message::Command(CommandAction::Settings),
            },
            CommandItem {
                label: String::from("Quit"),
                message: Message::Command(CommandAction::Quit),
            },
            CommandItem {
                label: String::from("Toggle assistant"),
                message: Message::Command(CommandAction::ToggleAssistant),
            },
        ];

        Self {
            is_visible: false,
            search_query: String::new(),
            commands: commands.clone(),
            filtered_commands: commands,
            selected_command_index: 0,
            scroll_offset: 0.0,
            viewport_height: 0.0,
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
    Scrolled(scrollable::Viewport),
    Command(CommandAction),
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
                        .height(Length::Fixed(ROW_HEIGHT))
                        .style(move |theme: &Theme, status| button::Style {
                            background: Some(Background::Color(if is_selected {
                                theme.palette().background.stronger.color
                            } else {
                                theme.palette().background.weakest.color
                            })),
                            text_color: theme.palette().background.weak.text,
                            ..button::primary(theme, status)
                        })
                        .padding([4, 4])
                        .into()
                })
                .collect();

            Palette::new(&self.search_query, rows, Message::Scrolled)
                .on_input(Message::InputChanged)
                .on_select_next(Message::SelectNext)
                .on_select_previous(Message::SelectPrevious)
                .on_hover(Message::Hovered)
                .scroll_offset(self.scroll_offset)
                .into()
        })
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Toggle => {
                self.is_visible = !self.is_visible;
                self.scroll_offset = 0.0;
                self.viewport_height = 0.0;
                Task::none()
            }
            Message::Hide => {
                self.is_visible = false;
                self.search_query.clear();
                self.scroll_offset = 0.0;
                self.viewport_height = 0.0;
                self.refresh_filtered_commands();
                Task::none()
            }
            Message::InputChanged(query) => {
                self.search_query = query;
                self.selected_command_index = 0;
                self.refresh_filtered_commands();
                self.scroll_selected_into_view()
            }
            Message::SelectNext => {
                let count = self.filtered_commands.len();
                if count > 0 {
                    self.selected_command_index = (self.selected_command_index + 1) % count;
                }
                self.scroll_selected_into_view()
            }
            Message::SelectPrevious => {
                let count = self.filtered_commands.len();
                if count > 0 {
                    self.selected_command_index = (self.selected_command_index + count - 1) % count;
                }
                self.scroll_selected_into_view()
            }
            Message::Hovered(index) => {
                self.selected_command_index = index;
                Task::none()
            }
            Message::Scrolled(viewport) => {
                self.scroll_offset = viewport.absolute_offset().y;
                self.viewport_height = viewport.bounds().height;
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
            CommandAction::Quit => todo!(),
            CommandAction::ToggleAssistant => todo!(),
        }
    }

    fn scroll_selected_into_view(&self) -> Task<Message> {
        if self.filtered_commands.len() == 0 {
            return Task::none();
        }

        let index = self
            .selected_command_index
            .min(self.filtered_commands.len() - 1);

        let row_top = LIST_PADDING + index as f32 * ROW_STRIDE;
        let row_bottom = row_top + ROW_HEIGHT;

        let viewport_top = self.scroll_offset;
        let viewport_bottom = self.scroll_offset + self.viewport_height;

        let target = if self.viewport_height <= 0.0 {
            row_top
        } else if row_top < viewport_top {
            row_top
        } else if row_bottom > viewport_bottom {
            (row_bottom - self.viewport_height).max(0.0)
        } else {
            return Task::none();
        };

        operation::scroll_to(
            iced::widget::Id::new(SCROLLABLE_ID),
            operation::AbsoluteOffset {
                x: None,
                y: Some(target),
            },
        )
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
