use iced::widget::space::horizontal;
use iced::widget::{button, column, container, mouse_area, row, rule, svg, text};
use iced::{Color, Element, Length, Point, Task, Theme, alignment};
use iced::{Subscription, window};

use crate::components::connection_dialog::{ConnectionDialog, DialogMessage};
use crate::components::connection_item::ItemMessage;
use crate::components::sidebar::{self, SidebarMessage};
use crate::components::welcome_view;
use crate::connection_manager::{ConnManagerMessage, ConnectionManager};
use iced_aw::drop_down;

#[derive(Debug, Clone)]
pub enum Message {
    Sidebar(SidebarMessage),
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
    ToggleMenu,
    CloseMenu,
    AddConnection,
}

#[derive(Debug)]
pub struct App {
    pub manager: ConnectionManager,
    pub dialog: ConnectionDialog,
    pub zoom_multiplier: u8,
    pub is_maximized: bool,
    pub saved_position: Option<Point>,
    pub menu_open: bool,
}

impl Default for App {
    fn default() -> Self {
        Self {
            manager: ConnectionManager::default(),
            dialog: ConnectionDialog::default(),
            zoom_multiplier: 0,
            is_maximized: false,
            saved_position: None,
            menu_open: false,
        }
    }
}

impl App {
    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::AddConnection => {
                Task::done(Message::CloseMenu).chain(Task::done(Message::ConnManager(
                    ConnManagerMessage::ConnectionDialogMessage(DialogMessage::OpenNew),
                )))
            }
            Message::Sidebar(msg) => match msg {
                SidebarMessage::SelectConnection(id) => Task::done(Message::ConnManager(
                    ConnManagerMessage::ConnectionItemMessage(id, ItemMessage::Select),
                )),
                SidebarMessage::ItemMessage(id, item_msg) => Task::done(Message::ConnManager(
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
            Message::ToggleMenu => {
                self.menu_open = !self.menu_open;
                Task::none()
            }
            Message::CloseMenu => {
                self.menu_open = false;
                Task::none()
            }
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let main = self.view_main();
        let sidebar = sidebar::view(&self.manager.items).map(Message::Sidebar);

        let layout = container(column![
            self.view_title_bar(),
            row![sidebar, iced::widget::rule::vertical(1), main,]
        ])
        .style(|_theme: &Theme| -> container::Style {
            container::Style::default()
                .background(iced::Background::Color(_theme.palette().background))
                .border(iced::Border::default().rounded(12))
        });

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
        let hamburger_btn = button(
            svg(svg::Handle::from_memory(include_bytes!(
                "resources/menu.svg"
            )))
            .height(16)
            .width(16)
            .style(|_theme, _status| svg::Style {
                color: Some(Color::WHITE),
            }),
        )
        .on_press(Message::ToggleMenu)
        .style(|_theme, _status| button::Style {
            background: Some(iced::Background::Color(Color::TRANSPARENT)),
            ..Default::default()
        });

        let title = text("pgeru").size(13).align_x(text::Alignment::Left);
        let menu_content = self.menu_content_view();

        let dropdown = iced_aw::DropDown::new(hamburger_btn, menu_content, self.menu_open)
            .on_dismiss(Message::CloseMenu)
            .offset(iced_aw::drop_down::Offset::new(0.0, 25.0))
            .width(250)
            .alignment(drop_down::Alignment::BottomStart);

        let close_button = button(
            svg(svg::Handle::from_memory(include_bytes!("resources/x.svg")))
                .height(16)
                .width(16)
                .style(|_theme, _status| svg::Style {
                    color: Some(Color::WHITE),
                }),
        )
        .on_press(Message::Close)
        .style(|_theme, _status| button::Style {
            background: Some(iced::Background::Color(Color::TRANSPARENT)),
            ..Default::default()
        });
        let draggable_area =
            mouse_area(row![dropdown, title, horizontal()].align_y(alignment::Vertical::Center))
                .on_press(Message::Drag)
                .on_double_click(Message::ToggleMaximize);

        container(column![
            row![draggable_area, close_button]
                .width(Length::Fill)
                .align_y(iced::Alignment::Center),
            rule::horizontal(1.0)
        ])
        .style(|theme: &Theme| container::Style {
            background: Some(iced::Background::Color(
                theme.extended_palette().background.strong.color,
            )),
            ..Default::default()
        })
        .into()
    }

    pub fn key_press_handler(&self) -> Subscription<Message> {
        iced::keyboard::listen().filter_map(|event| match event {
            iced::keyboard::Event::KeyPressed { key, modifiers, .. } => {
                match (modifiers, key.as_ref()) {
                    (iced::keyboard::Modifiers::CTRL, iced::keyboard::Key::Character("=")) => {
                        Some(Message::ZoomIn)
                    }
                    (iced::keyboard::Modifiers::CTRL, iced::keyboard::Key::Character("-")) => {
                        Some(Message::ZoomOut)
                    }
                    _ => None,
                }
            }
            _ => None,
        })
    }

    fn menu_content_view(&self) -> Element<'_, Message> {
        let menu_content = container(
            column![
                button(text("Add Connection").size(13))
                    .on_press(Message::AddConnection)
                    .padding([6, 12])
                    .width(Length::Fill)
                    .style(button::subtle),
                button(text("Settings").size(13))
                    .on_press(Message::CloseMenu)
                    .padding([6, 12])
                    .width(Length::Fill)
                    .style(button::subtle),
                button(text("About").size(13))
                    .on_press(Message::CloseMenu)
                    .padding([6, 12])
                    .width(Length::Fill)
                    .style(button::subtle),
            ]
            .spacing(0),
        )
        .width(150)
        .style(|theme: &Theme| container::Style {
            background: Some(iced::Background::Color(
                theme.extended_palette().background.strong.color,
            )),
            border: iced::Border::default().rounded(4),
            ..Default::default()
        });
        menu_content.into()
    }
}
