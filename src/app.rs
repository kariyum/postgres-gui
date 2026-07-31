use std::time::Duration;

use anyhow::Context;
use iced::widget::space::horizontal;
use iced::widget::{
    Column, Scrollable, button, column, container, mouse_area, row, rule, scrollable, svg, text,
};
use iced::widget::{pane_grid, space};
use iced::{Color, Element, Length, Point, Task, Theme, alignment, border};
use iced::{Subscription, mouse, window};
use tracing::{error, info};

use crate::components::agent_chat::{AgentChat, AgentChatMessage};
use crate::components::connection_dialog::{ConnectionDialog, DialogMessage};
use crate::components::connection_item::{self, ItemMessage};
use crate::components::settings_dialog::{SettingsDialog, SettingsMessage};
use crate::components::sidebar::{self, SidebarMessage};
use crate::connection_manager::{ConnManagerMessage, ConnectionManager};
use crate::core::agent_config::AgentConfig;
use crate::core::config_loader::{self, AppConfig};
use crate::core::configured_provider::{BaseProvider, ConfiguredProvider};
use crate::core::connection_config::{self, ConnectionConfig};
use iced_aw::drop_down;

use crate::theme;

#[derive(Debug, Clone)]
pub enum Message {
    Sidebar(SidebarMessage),
    ConnManager(ConnManagerMessage),
    Close,
    Drag,
    DragResize(window::Direction),
    ConfigLoaded(AppConfig),
    SavePending,
    ToggleMaximize,
    PositionSaved(Option<Point>),
    RestorePosition,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    Noop,
    ToggleMenu,
    CloseMenu,
    AddConnection,
    WindowResized(window::Id),
    MaximizedQueried(bool),
    Settings(SettingsMessage),
    AgentChat(AgentChatMessage),
    OpenAiSettings,
    Resized(pane_grid::ResizeEvent),
    ToggleAgentMenu,
    CloseAgentMenu,
    AgentProviderSelected(ConfiguredProvider),
    Connect(connection_config::Message),
}

#[derive(Debug)]
pub enum PaneKind {
    Main,
    AgentChat,
}

#[derive(Debug)]
pub struct App {
    pub connection_manager: ConnectionManager,
    pub dialog: ConnectionDialog,
    pub settings: SettingsDialog,
    pub agent_config: AgentConfig,
    pub agent_chat: Option<AgentChat>,
    pub zoom_multiplier: u8,
    pub is_maximized: bool,
    pub saved_position: Option<Point>,
    pub menu_open: bool,
    pub agent_menu_open: bool,
    pub pending_save: bool,
    panes: pane_grid::State<PaneKind>,
    agent_chat_pane: Option<pane_grid::Pane>,
}

impl Default for App {
    fn default() -> Self {
        let (pane, _main_pane) = pane_grid::State::new(PaneKind::Main);
        Self {
            connection_manager: ConnectionManager::default(),
            dialog: ConnectionDialog::default(),
            settings: SettingsDialog::default(),
            agent_config: AgentConfig::default(),
            agent_chat: None,
            zoom_multiplier: 0,
            is_maximized: false,
            saved_position: None,
            menu_open: false,
            agent_menu_open: false,
            pending_save: false,
            panes: pane,
            agent_chat_pane: None,
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
                use crate::connection_manager::Action;
                match self.connection_manager.update(msg) {
                    Action::None => {
                        self.sync_connections();
                        Task::none()
                    }
                    Action::Run(task) => {
                        self.sync_connections();
                        task.map(Message::ConnManager)
                    }
                    Action::Dialog(msg) => self.dialog.update(msg).map(|m| {
                        Message::ConnManager(ConnManagerMessage::ConnectionDialogMessage(m))
                    }),
                }
            }
            Message::ConfigLoaded(config) => {
                self.zoom_multiplier = config.zoom_multiplier;
                self.agent_config = config.agent_config.clone();
                info!("Loaded config {:?}", config);
                self.sync_connections();
                Task::batch([
                    Task::done(Message::ConnManager(ConnManagerMessage::ConnectionsLoaded(
                        config.connections,
                    ))),
                    Task::done(Message::Settings(SettingsMessage::AgentConfig(
                        config.agent_config,
                    ))),
                ])
            }
            Message::SavePending => {
                if self.pending_save {
                    self.pending_save = false;
                    self.save_config()
                } else {
                    Task::none()
                }
            }
            Message::ZoomIn => {
                self.zoom_multiplier += 1;
                self.pending_save = true;
                Task::none()
            }
            Message::ZoomOut => {
                if self.zoom_multiplier > 0 {
                    self.zoom_multiplier -= 1;
                }
                self.pending_save = true;
                Task::none()
            }
            Message::ZoomReset => {
                self.zoom_multiplier = 0;
                self.pending_save = true;
                Task::none()
            }
            Message::Close => iced::exit(),
            Message::Drag => window::latest().and_then(window::drag),
            Message::DragResize(direction) => {
                window::latest().and_then(move |id| window::drag_resize(id, direction))
            }
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
                self.saved_position.take();
                Task::none()
            }
            Message::Noop => Task::none(),
            Message::ToggleMenu => {
                self.menu_open = !self.menu_open;
                Task::none()
            }
            Message::CloseMenu => {
                self.menu_open = false;
                Task::none()
            }
            Message::OpenAiSettings => {
                self.menu_open = false;
                Task::done(Message::Settings(SettingsMessage::Open))
            }
            Message::WindowResized(id) => window::is_maximized(id).map(Message::MaximizedQueried),
            Message::MaximizedQueried(maximized) => {
                self.is_maximized = maximized;
                Task::none()
            }
            Message::Settings(msg) => {
                use crate::components::settings_dialog::Action;
                match self.settings.update(msg) {
                    Action::None => Task::none(),
                    Action::Run(task) => task.map(Message::Settings),
                    Action::SaveRequested {
                        config,
                        fetch_models,
                    } => {
                        self.agent_config = config;
                        Task::batch([self.save_config(), fetch_models.map(Message::Settings)])
                    }
                }
            }
            Message::AgentChat(msg) => {
                if let AgentChatMessage::ModelChanged(provider) = msg {
                    match provider.base_provider {
                        BaseProvider::Anthropic => {
                            self.agent_config.anthropic_config = Some(provider);
                        }
                        BaseProvider::OpenCode => {
                            self.agent_config.opencode_config = Some(provider);
                        }
                    }
                    self.save_config()
                } else if let Some(ref mut agent) = self.agent_chat {
                    agent.update(msg).map(Message::AgentChat)
                } else {
                    Task::none()
                }
            }
            Message::Resized(event) => {
                self.panes.resize(event.split, event.ratio);
                Task::none()
            }
            Message::ToggleAgentMenu => {
                if let Some(pane) = self.agent_chat_pane {
                    self.panes.close(pane);
                } else {
                }
                self.agent_menu_open = !self.agent_menu_open;
                Task::none()
            }
            Message::CloseAgentMenu => {
                self.agent_menu_open = false;
                Task::none()
            }
            Message::AgentProviderSelected(mut provider) => {
                provider.available_models = match provider.base_provider {
                    BaseProvider::Anthropic => {
                        self.settings.anthropic_config.available_models.clone()
                    }
                    BaseProvider::OpenCode => {
                        self.settings.opencode_config.available_models.clone()
                    }
                };

                let configs: Vec<ConnectionConfig> = self
                    .connection_manager
                    .items
                    .iter()
                    .map(|connection_item| connection_item.cfg.clone())
                    .collect();

                let pools: std::collections::HashMap<String, sqlx::PgPool> = self
                    .connection_manager
                    .items
                    .iter()
                    .filter_map(|i| Some((i.cfg.name.clone(), i.pool.clone()?)))
                    .collect();
                let chat = AgentChat::new(provider, configs, pools);
                self.agent_chat = Some(chat);
                self.agent_menu_open = false;
                let main_pane = self
                    .panes
                    .iter()
                    .find(|(_, state)| matches!(state, PaneKind::Main))
                    .map(|(pane, _)| *pane);
                if let Some(main_pane) = main_pane {
                    if let Some((agent_pane, _split)) =
                        self.panes
                            .split(pane_grid::Axis::Vertical, main_pane, PaneKind::AgentChat)
                    {
                        self.agent_chat_pane = Some(agent_pane);
                    }
                }
                Task::none()
            }
            Message::Connect(_) => {
                info!("Got connect on config");
                Task::none()
            }
        }
    }

    fn save_config(&self) -> Task<Message> {
        let config = AppConfig {
            connections: self
                .connection_manager
                .items
                .iter()
                .map(|i| i.cfg.clone())
                .collect(),
            zoom_multiplier: self.zoom_multiplier,
            agent_config: self.agent_config.clone(),
        };
        Task::perform(
            async move {
                tokio::task::spawn_blocking(move || config_loader::save_config(&config))
                    .await
                    .context("Background task failed")
                    .flatten()
            },
            |result| match result {
                Ok(()) => Message::Settings(SettingsMessage::Saved),
                Err(err) => {
                    error!("Got an error {}", err);
                    Message::Noop
                } // todo report saving error
            },
        )
    }

    pub fn save_subscription(&self) -> Subscription<Message> {
        if self.pending_save {
            iced::time::every(Duration::from_millis(500)).map(|_| Message::SavePending)
        } else {
            Subscription::none()
        }
    }

    pub fn window_event_subscription(&self) -> Subscription<Message> {
        window::resize_events().map(|(id, _size)| Message::WindowResized(id))
    }

    pub fn view_footer(&self) -> Element<'_, Message> {
        let agent_btn = button(
            svg(svg::Handle::from_memory(include_bytes!(
                "resources/sparkles.svg"
            )))
            .height(14)
            .width(14)
            .style(|_theme, _status| svg::Style {
                color: Some(Color::WHITE),
            }),
        )
        .on_press(Message::ToggleAgentMenu)
        .style(button::background);

        let menu_content = self.agent_menu_content_view();

        let dropdown = iced_aw::DropDown::new(agent_btn, menu_content, self.agent_menu_open)
            .on_dismiss(Message::CloseAgentMenu)
            .width(Length::Shrink)
            .offset(iced_aw::drop_down::Offset::new(0.0, 20.0))
            .alignment(drop_down::Alignment::TopEnd);

        container(row![horizontal(), dropdown,]).height(25).into()
    }

    pub fn view(&self) -> Element<'_, Message> {
        let content_area = self.content_area();
        let connection_manager_dialog = self.view_connection_manager_dialog();
        let settings_dialog = self.view_settings_dialog();
        let dialog = connection_manager_dialog.or(settings_dialog);

        let layout = container(column![
            self.view_title_bar(),
            content_area,
            rule::horizontal(1.0),
            self.view_footer()
        ])
        .style(|_theme: &Theme| -> container::Style {
            container::Style::default()
                .background(iced::Background::Color(
                    _theme.palette().background.base.color,
                ))
                .border(iced::Border::default().rounded(12))
        });

        let content: Element<'_, Message> = if let Some(dialog) = dialog {
            iced::widget::stack![layout, dialog].into()
        } else {
            layout.into()
        };

        if self.is_maximized {
            content.into()
        } else {
            add_window_resize_mouse_interactions(content)
        }
    }

    fn content_area(&self) -> Element<'_, Message> {
        pane_grid::PaneGrid::new(&self.panes, |_pane, state, _is_maximized| match state {
            PaneKind::Main => pane_grid::Content::new(self.view_main()),
            PaneKind::AgentChat => pane_grid::Content::new(row![
                rule::vertical(1),
                self.agent_chat
                    .as_ref()
                    .unwrap()
                    .view()
                    .map(Message::AgentChat)
            ]),
        })
        .width(Length::Fill)
        .height(Length::Fill)
        .spacing(0)
        .on_resize(5, |event| Message::Resized(event))
        .into()
    }

    fn view_settings_dialog(&self) -> Option<container::Container<'_, Message>> {
        self.settings.view().map(|dialog| {
            container(dialog.map(Message::Settings))
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(iced::Alignment::Center)
                .align_y(iced::Alignment::Center)
                .style(|_: &Theme| iced::widget::container::Style {
                    background: Some(Color::from_rgba(0.0, 0.0, 0.0, 0.45).into()),
                    ..Default::default()
                })
        })
    }

    fn view_connection_manager_dialog(&self) -> Option<container::Container<'_, Message>> {
        self.dialog.view().map(|dialog| {
            container(
                dialog.map(|msg| {
                    Message::ConnManager(ConnManagerMessage::ConnectionDialogMessage(msg))
                }),
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(iced::Alignment::Center)
            .align_y(iced::Alignment::Center)
            .style(|_: &Theme| iced::widget::container::Style {
                background: Some(Color::from_rgba(0.0, 0.0, 0.0, 0.45).into()),
                ..Default::default()
            })
        })
    }

    fn view_configs(&self) -> Element<'_, Message> {
        scrollable(
            Column::from_vec(
                self.connection_manager
                    .items
                    .iter()
                    .map(|item| item.cfg.view().map(Message::Connect).into())
                    .collect(),
            )
            .spacing(12),
        )
        .into()
    }

    fn view_welcome(&self) -> Element<'_, Message> {
        container(
            column![
                text("pgeru").size(48).font(iced::Font {
                    weight: iced::font::Weight::Bold,
                    ..iced::Font::DEFAULT
                }),
                text("PostgreSQL client").size(18).color(theme::TEXT_MUTED),
                container(rule::horizontal(1)).width(400),
                container(self.view_configs()).height(400)
            ]
            .spacing(6)
            .align_x(iced::Alignment::Center),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(iced::Alignment::Center)
        .align_y(iced::Alignment::Center)
        .into()
    }

    fn view_main(&self) -> Element<'_, Message> {
        let body: Element<Message> =
            if let Some(ref active_id) = self.connection_manager.active_connection {
                if let Some(item) = self
                    .connection_manager
                    .items
                    .iter()
                    .find(|i| &i.cfg.id == active_id)
                {
                    item.view_editor().map(move |msg| {
                        Message::ConnManager(ConnManagerMessage::ConnectionItemMessage(
                            active_id.clone(),
                            msg,
                        ))
                    })
                } else {
                    self.view_welcome()
                }
            } else {
                self.view_welcome()
            };

        body.into()
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
                theme.palette().background.strong.color,
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
                    .style(|_theme, _status| button::Style {
                        border: border::rounded(0.0),
                        ..button::subtle(_theme, _status)
                    }),
                button(text("Settings").size(13))
                    .on_press(Message::OpenAiSettings)
                    .padding([6, 12])
                    .width(Length::Fill)
                    .style(|_theme, _status| button::Style {
                        border: border::rounded(0.0),
                        ..button::subtle(_theme, _status)
                    }),
                button(text("About").size(13))
                    .on_press(Message::CloseMenu)
                    .padding([6, 12])
                    .width(Length::Fill)
                    .style(|_theme, _status| button::Style {
                        border: border::rounded(0.0),
                        ..button::subtle(_theme, _status)
                    }),
            ]
            .spacing(0),
        )
        .width(150)
        .style(|theme: &Theme| container::Style {
            background: Some(iced::Background::Color(
                theme.palette().background.strong.color,
            )),
            border: iced::Border::default().rounded(4),
            ..Default::default()
        });
        menu_content.into()
    }

    fn agent_menu_content_view(&self) -> Element<'_, Message> {
        let providers: Vec<&ConfiguredProvider> = [
            self.agent_config.anthropic_config.as_ref(),
            self.agent_config.opencode_config.as_ref(),
        ]
        .into_iter()
        .flatten()
        .collect();

        let buttons: Vec<Element<'_, Message>> = providers
            .iter()
            .map(|provider| {
                button(text(provider.base_provider.to_string()).size(13))
                    .on_press(Message::AgentProviderSelected((*provider).clone()))
                    .padding([2, 4])
                    .width(Length::Fill)
                    .style(|_theme, _status| button::Style {
                        border: iced::Border::default().rounded(4),
                        background: match _status {
                            button::Status::Active => {
                                Some(iced::Background::Color(Color::TRANSPARENT))
                            }
                            _ => None,
                        },
                        ..button::subtle(_theme, _status)
                    })
                    .into()
            })
            .collect();

        let menu_content = if buttons.is_empty() {
            container(
                text("No providers configured")
                    .size(13)
                    .style(|_theme: &Theme| text::Style {
                        color: Some(Color::from_rgba(0.6, 0.6, 0.6, 1.0)),
                    }),
            )
            .padding([6, 12])
        } else {
            container(column(buttons).spacing(4)).padding([2, 2])
        };

        menu_content
            .width(100)
            .style(|theme: &Theme| container::Style {
                background: Some(iced::Background::Color(
                    theme.palette().background.weak.color,
                )),
                border: iced::Border::default().rounded(4),
                ..Default::default()
            })
            .into()
    }

    fn sync_connections(&self) {
        info!("Sync connections...");
        let configs: Vec<ConnectionConfig> = self
            .connection_manager
            .items
            .iter()
            .map(|connection_item| connection_item.cfg.clone())
            .collect();

        let pools: std::collections::HashMap<String, sqlx::PgPool> = self
            .connection_manager
            .items
            .iter()
            .filter_map(|i| Some((i.cfg.name.clone(), i.pool.clone()?)))
            .collect();

        info!("agent_chat is defined: {}", self.agent_chat.is_some());
        if let Some(ref agent_chat) = self.agent_chat {
            agent_chat.update_connections(configs, pools);
        }
    }
}

fn add_window_resize_mouse_interactions(content: Element<'_, Message>) -> Element<'_, Message> {
    let h = Length::Fixed(6.0);
    iced::widget::stack![
        content,
        container(resize_handle(
            window::Direction::North,
            mouse::Interaction::ResizingVertically,
            Length::Fill,
            h,
        ))
        .width(Length::Fill)
        .height(h)
        .align_y(iced::Alignment::Start),
        container(resize_handle(
            window::Direction::South,
            mouse::Interaction::ResizingVertically,
            Length::Fill,
            h,
        ))
        .width(Length::Fill)
        .height(Length::Fill)
        .align_y(iced::Alignment::End),
        container(resize_handle(
            window::Direction::West,
            mouse::Interaction::ResizingHorizontally,
            h,
            Length::Fill,
        ))
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(iced::Alignment::Start),
        container(resize_handle(
            window::Direction::East,
            mouse::Interaction::ResizingHorizontally,
            h,
            Length::Fill,
        ))
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(iced::Alignment::End),
        container(resize_handle(
            window::Direction::NorthWest,
            mouse::Interaction::ResizingDiagonallyDown,
            h,
            h,
        ))
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(iced::Alignment::Start)
        .align_y(iced::Alignment::Start),
        container(resize_handle(
            window::Direction::NorthEast,
            mouse::Interaction::ResizingDiagonallyUp,
            h,
            h,
        ))
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(iced::Alignment::End)
        .align_y(iced::Alignment::Start),
        container(resize_handle(
            window::Direction::SouthWest,
            mouse::Interaction::ResizingDiagonallyUp,
            h,
            h,
        ))
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(iced::Alignment::Start)
        .align_y(iced::Alignment::End),
        container(resize_handle(
            window::Direction::SouthEast,
            mouse::Interaction::ResizingDiagonallyDown,
            h,
            h,
        ))
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(iced::Alignment::End)
        .align_y(iced::Alignment::End),
    ]
    .into()
}

fn resize_handle(
    direction: window::Direction,
    interaction: mouse::Interaction,
    width: Length,
    height: Length,
) -> Element<'static, Message> {
    mouse_area(container("").width(width).height(height))
        .on_press(Message::DragResize(direction))
        .interaction(interaction)
        .into()
}
