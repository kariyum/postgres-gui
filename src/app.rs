use iced::widget::space::horizontal;
use iced::widget::text::Alignment;
use iced::widget::{button, column, container, mouse_area, row, svg, text};
use iced::window;
use iced::{Color, Element, Length, Point, Task, Theme};

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
    Close,
    Drag,
    ToggleMaximize,
    PositionSaved(Option<Point>),
    RestorePosition,
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
    pub zoom_multiplier: u8,
    pub is_maximized: bool,
    pub saved_position: Option<Point>,
}

impl Default for App {
    fn default() -> Self {
        Self {
            manager: ConnectionManager::default(),
            dialog: ConnectionDialog::default(),
            zoom_multiplier: 0,
            is_maximized: false,
            saved_position: None,
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

            Message::Close => iced::exit(),
            Message::Drag => window::latest().and_then(window::drag),
            Message::ToggleMaximize => {
                if self.is_maximized {
                    self.is_maximized = false;
                    window::latest()
                        .and_then(window::toggle_maximize)
                        .map(|()| Message::RestorePosition)
                } else {
                    self.is_maximized = true;
                    window::latest()
                        .and_then(window::position)
                        .map(Message::PositionSaved)
                }
            }
            Message::PositionSaved(pos) => {
                self.saved_position = pos;
                window::latest().and_then(window::toggle_maximize)
            }
            Message::RestorePosition => {
                if let Some(pos) = self.saved_position.take() {
                    window::latest().and_then(move |id| window::move_to(id, pos))
                } else {
                    Task::none()
                }
            }
            Message::ToggleSidebar => Task::none(),
            Message::Noop => Task::none(),
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let main = self.view_main();
        let sidebar = sidebar::view(&self.manager.items).map(Message::Sidebar);

        let layout = column![
            self.view_title_bar(),
            row![sidebar, iced::widget::rule::vertical(1), main,]
        ];

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
            layout.into()
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

    fn view_title_bar(&self) -> Element<'_, Message> {
        let title = text("pgeru").size(13).align_x(Alignment::Left);
        let close_button = button(
            svg(svg::Handle::from_memory(include_bytes!("resources/x.svg")))
                .height(14)
                .width(14)
                .style(|_theme, _status| svg::Style {
                    color: Some(Color::WHITE),
                }),
        )
        .on_press(Message::Close)
        .style(|_theme, _status| button::Style {
            ..Default::default()
        });
        let draggable_area = mouse_area(row![title, horizontal()])
            .on_press(Message::Drag)
            .on_double_click(Message::ToggleMaximize);
        row![draggable_area, close_button]
            .width(Length::Fill)
            .padding([4, 8])
            .into()
    }
}
