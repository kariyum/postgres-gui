use iced::widget::{button, column, container, row, text};
use iced::{Color, Element, Length, Task, Theme};

use crate::components::connection_dialog::{ConnectionDialog, DialogMessage};
use crate::components::connection_item::ItemMessage;
use crate::components::sidebar::{self, SidebarMessage};
use crate::components::tab_bar::{self, TabBarMessage};
use crate::components::welcome_view;
use crate::connection_manager::{ConnManagerMessage, ConnectionManager};

#[derive(Debug, Clone)]
pub enum Message {
    Sidebar(SidebarMessage),
    TabBar(TabBarMessage),
    ConnManager(ConnManagerMessage),

    ToggleSidebar,
    ZoomIn,
    ZoomOut,
    ZoomReset,

    Noop,
}

#[derive(Debug)]
pub struct App {
    pub manager: ConnectionManager,
    pub dialog: ConnectionDialog,
    pub sidebar_open: bool,
    pub zoom_multiplier: u8,
}

impl Default for App {
    fn default() -> Self {
        Self {
            manager: ConnectionManager::default(),
            dialog: ConnectionDialog::default(),
            sidebar_open: true,
            zoom_multiplier: 0,
        }
    }
}

impl App {
    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Sidebar(msg) => match msg {
                SidebarMessage::AddConnection => Task::done(Message::ConnManager(
                    ConnManagerMessage::ConnectionDialogMessage(DialogMessage::OpenNew),
                )),
                SidebarMessage::SelectConnection(id) => Task::done(Message::ConnManager(
                    ConnManagerMessage::ConnectionItemMessage(id, ItemMessage::Select),
                )),
                SidebarMessage::ItemMessage(id, item_msg) => Task::done(Message::ConnManager(
                    ConnManagerMessage::ConnectionItemMessage(id, item_msg),
                )),
            },

            Message::TabBar(msg) => match msg {
                TabBarMessage::SelectTab(id) => Task::done(Message::ConnManager(
                    ConnManagerMessage::ConnectionItemMessage(id, ItemMessage::Select),
                )),
                TabBarMessage::CloseTab(id) => Task::done(Message::ConnManager(
                    ConnManagerMessage::ConnectionItemMessage(id, ItemMessage::DisconnectRequested),
                )),
                TabBarMessage::ItemMessage(id, item_msg) => Task::done(Message::ConnManager(
                    ConnManagerMessage::ConnectionItemMessage(id, item_msg),
                )),
            },

            Message::ConnManager(msg) => {
                let task = self.manager.update(msg, &mut self.dialog);
                task.map(Message::ConnManager)
            }

            Message::ToggleSidebar => {
                self.sidebar_open = !self.sidebar_open;
                Task::none()
            }
            Message::ZoomIn => {
                self.zoom_multiplier += 1;
                Task::none()
            }
            Message::ZoomOut => {
                if self.zoom_multiplier > 0 {
                    self.zoom_multiplier -= 1;
                }
                Task::none()
            }
            Message::ZoomReset => {
                self.zoom_multiplier = 0;
                Task::none()
            }

            Message::Noop => Task::none(),
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let main = self.view_main();

        let layout: Element<Message> = if self.sidebar_open {
            let sidebar = sidebar::view(&self.manager.items).map(Message::Sidebar);
            row![sidebar, iced::widget::rule::vertical(1), main,].into()
        } else {
            row![
                container(
                    button(text("☰").size(16))
                        .on_press(Message::ToggleSidebar)
                        .padding([6, 8])
                        .style(iced::widget::button::secondary),
                )
                .padding([4, 0])
                .width(Length::Fixed(32.0))
                .height(Length::Fill)
                .align_y(iced::Alignment::Start),
                iced::widget::rule::vertical(1),
                main,
            ]
            .into()
        };

        if let Some(dialog) = self.dialog.view() {
            iced::widget::stack![
                layout,
                container(dialog.map(|msg| {
                    Message::ConnManager(ConnManagerMessage::ConnectionDialogMessage(msg))
                }))
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(iced::Alignment::Center)
                .align_y(iced::Alignment::Center)
                .style(|_: &Theme| iced::widget::container::Style {
                    background: Some(Color::from_rgba(0.0, 0.0, 0.0, 0.45).into()),
                    ..Default::default()
                }),
            ]
            .into()
        } else {
            layout
        }
    }

    fn view_main(&self) -> Element<'_, Message> {
        let body: Element<Message> = if let Some(ref active_id) = self.manager.active_connection {
            if let Some(item) = self.manager.items.iter().find(|i| &i.cfg.id == active_id) {
                item.view_editor().map(move |msg| {
                    Message::ConnManager(ConnManagerMessage::ConnectionItemMessage(
                        active_id.clone(),
                        msg,
                    ))
                })
            } else {
                welcome_view::view()
            }
        } else {
            welcome_view::view()
        };

        container(body).height(Length::Fill).into()
    }
}
