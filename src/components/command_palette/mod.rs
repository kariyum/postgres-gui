mod command_list;
mod connection_picker;

use iced::widget::{Id, operation};
use iced::{Element, Task};

use crate::components::connection_config::ConnectionConfig;
use crate::widgets::palette;
use command_list::{CommandAction, CommandList};
use connection_picker::ConnectionPicker;

#[derive(Debug, Clone)]
pub struct CommandPalette {
    is_visible: bool,
    stage: Stage,
}

#[derive(Debug, Clone)]
enum Stage {
    Commands(CommandList),
    Connections(ConnectionPicker),
}

#[derive(Debug, Clone)]
pub enum Message {
    Toggle,
    Hide,
    ShowConnections(Vec<ConnectionConfig>),
    Event(Event),
    CommandList(command_list::Message),
    ConnectionPicker(connection_picker::Message),
}

#[derive(Debug, Clone)]
pub enum Event {
    ExploreSchema,
    ConnectionsRequested,
    ConnectionSelected(ConnectionConfig),
}

impl Default for CommandPalette {
    fn default() -> Self {
        Self {
            is_visible: false,
            stage: Stage::Commands(CommandList::new()),
        }
    }
}

impl CommandPalette {
    pub fn view(&self) -> Option<Element<'_, Message>> {
        self.is_visible.then(|| match &self.stage {
            Stage::Commands(list) => list.view().map(Message::CommandList),
            Stage::Connections(picker) => picker.view().map(Message::ConnectionPicker),
        })
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Toggle => {
                if self.is_visible {
                    self.hide();
                    Task::none()
                } else {
                    self.show()
                }
            }
            Message::Hide => {
                self.hide();
                Task::none()
            }
            Message::Event(_) => Task::none(),
            Message::ShowConnections(connections) => {
                self.is_visible = true;
                self.stage = Stage::Connections(ConnectionPicker::new(connections));
                operation::focus(Id::new(connection_picker::INPUT_ID))
            }
            Message::CommandList(command_list::Message::Run(action)) => self.run_command(action),
            Message::CommandList(command_list::Message::Palette(
                palette::PaletteMessage::Close,
            )) => {
                self.hide();
                Task::none()
            }
            Message::CommandList(message) => match &mut self.stage {
                Stage::Commands(list) => list.update(message).map(Message::CommandList),
                Stage::Connections(_) => Task::none(),
            },
            Message::ConnectionPicker(connection_picker::Message::Selected(connection)) => {
                self.hide();
                Task::done(Message::Event(Event::ConnectionSelected(connection)))
            }
            Message::ConnectionPicker(connection_picker::Message::Palette(
                palette::PaletteMessage::Close,
            )) => {
                self.hide();
                Task::none()
            }
            Message::ConnectionPicker(message) => match &mut self.stage {
                Stage::Connections(picker) => picker.update(message).map(Message::ConnectionPicker),
                Stage::Commands(_) => Task::none(),
            },
        }
    }

    fn run_command(&mut self, action: CommandAction) -> Task<Message> {
        match action {
            CommandAction::ExploreSchema => {
                self.hide();
                Task::done(Message::Event(Event::ExploreSchema))
            }
            CommandAction::OpenConnectionQueryEditor => {
                Task::done(Message::Event(Event::ConnectionsRequested))
            }
            CommandAction::Settings => Task::none(),
            CommandAction::Quit => todo!(),
            CommandAction::ToggleAssistant => todo!(),
        }
    }

    fn show(&mut self) -> Task<Message> {
        self.is_visible = true;
        self.stage = Stage::Commands(CommandList::new());
        operation::focus(Id::new(command_list::INPUT_ID))
    }

    fn hide(&mut self) {
        self.is_visible = false;
        self.stage = Stage::Commands(CommandList::new());
    }
}
