use std::time::Duration;

use anyhow::Context;
use iced::futures::SinkExt;
use iced::futures::channel::mpsc::Sender;
use iced::widget::pane_grid;
use iced::widget::space::horizontal;
use iced::widget::{
    Column, button, column, container, mouse_area, row, rule, scrollable, svg, text,
};
use iced::{Border, Color, Element, Length, Point, Task, Theme, alignment, border};
use iced::{Subscription, mouse, window};
use tracing::{error, info};
use uuid::Uuid;

use crate::components::agent_chat::{AgentChat, AgentChatMessage};
use crate::components::connection_config;
use crate::components::connection_dialog::{self, ConnectionDialog, DialogMessage};
use crate::components::editor::{self, Editor};
use crate::components::editor_config::EditorConfig;
use crate::components::provider_config::ProviderConfigMessage;
use crate::components::settings_dialog::{SettingsDialog, SettingsMessage};
use crate::core::agent_config::AgentConfig;
use crate::core::agent_tools::DatabaseKeeperMessage;
use crate::core::config_loader::{self, AppConfig};
use crate::core::configured_provider::{BaseProvider, ConfiguredProvider};
use crate::core::database_keeper::{self, DatabaseKeeper};
use iced_aw::drop_down;

use crate::theme;

#[derive(Debug, Clone)]
pub enum Message {
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
    ConnectionConfig(connection_config::Message),
    Editor(editor::Message),
    DialogMessage(DialogMessage),
    DatabaseKeeperReady(Sender<DatabaseKeeperMessage>),
}

#[derive(Debug)]
pub enum PaneKind {
    Main,
    AgentChat,
}

#[derive(Debug)]
pub struct App {
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
    main_pane: pane_grid::Pane,
    agent_chat_pane: Option<pane_grid::Pane>,
    editor: Editor,
    database_keeper_actor_tx: Option<Sender<DatabaseKeeperMessage>>,
    app_config: AppConfig,
}

impl Default for App {
    fn default() -> Self {
        let (pane, main_pane) = pane_grid::State::new(PaneKind::Main);
        Self {
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
            main_pane,
            agent_chat_pane: None,
            editor: Editor::default(),
            database_keeper_actor_tx: None,
            app_config: AppConfig::default(),
        }
    }
}

impl App {
    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::DatabaseKeeperReady(tx) => {
                self.database_keeper_actor_tx = Some(tx.clone());
                if !self.app_config.connections.is_empty() {
                    self.send_loaded_config(tx, self.app_config.connections.clone())
                } else {
                    Task::none()
                }
            }
            Message::Editor(msg) => self.editor.update(msg).map(Message::Editor),
            Message::AddConnection => Task::done(Message::CloseMenu)
                .chain(Task::done(Message::DialogMessage(DialogMessage::OpenNew))),
            Message::ConfigLoaded(config) => {
                info!("Loaded config {:?}", config);
                self.app_config = config.clone();
                self.zoom_multiplier = config.zoom_multiplier;
                self.agent_config = config.agent_config.clone();
                let tx_opt = self.database_keeper_actor_tx.clone();
                Task::batch([
                    Task::done(Message::Settings(SettingsMessage::AgentConfig(
                        config.agent_config,
                    ))),
                    if let Some(tx) = tx_opt {
                        self.send_loaded_config(tx, config.connections)
                    } else {
                        Task::none()
                    },
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
                if let Some(agent_chat) = self.agent_chat.as_mut() {
                    if let SettingsMessage::AnthropicConfigMessage(
                        ProviderConfigMessage::ModelsFetched(Ok(ref models)),
                    ) = msg
                    {
                        let _ = agent_chat.update(AgentChatMessage::ModelsLoaded {
                            models: models.clone(),
                            base_provider: BaseProvider::Anthropic,
                        });
                    };
                    if let SettingsMessage::OpenCodeConfigMessage(
                        ProviderConfigMessage::ModelsFetched(Ok(ref models)),
                    ) = msg
                    {
                        let _ = agent_chat.update(AgentChatMessage::ModelsLoaded {
                            models: models.clone(),
                            base_provider: BaseProvider::OpenCode,
                        });
                    };
                }
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
                if let Some(ref tx) = self.database_keeper_actor_tx {
                    provider.available_models = match provider.base_provider {
                        BaseProvider::Anthropic => {
                            self.settings.anthropic_config.available_models.clone()
                        }
                        BaseProvider::OpenCode => {
                            self.settings.opencode_config.available_models.clone()
                        }
                    };

                    self.agent_chat = Some(AgentChat::new(provider, tx.clone()));
                    self.agent_menu_open = false;
                    if let Some((agent_pane, _split)) = self.panes.split(
                        pane_grid::Axis::Vertical,
                        self.main_pane,
                        PaneKind::AgentChat,
                    ) {
                        self.agent_chat_pane = Some(agent_pane);
                    }
                }
                Task::none()
            }
            Message::ConnectionConfig(msg) => match msg {
                connection_config::Message::Connect(cfg) => {
                    // maybe init a connection here for the default database? maybe add fetches databases to config for faster login and error out when database not found
                    if let Some(ref tx) = self.database_keeper_actor_tx {
                        Task::done(Message::Editor(editor::Message::Add(EditorConfig::new(
                            cfg,
                            tx.clone(),
                        ))))
                    } else {
                        Task::none()
                    }
                }
                connection_config::Message::Edit(cfg) => {
                    Task::done(Message::DialogMessage(DialogMessage::OpenEdit(cfg)))
                }
                connection_config::Message::Duplicate(cfg) => {
                    self.app_config
                        .connections
                        .push(connection_config::ConnectionConfig {
                            id: Uuid::new_v4().to_string(),
                            ..cfg.clone()
                        });
                    let config = self.app_config.clone();
                    let tx_opt = self.database_keeper_actor_tx.clone();
                    Task::batch([
                        if let Some(mut tx) = tx_opt {
                            Task::perform(
                                async move {
                                    tx.send(DatabaseKeeperMessage::ConnectionAction(
                                        database_keeper::ConnectionAction::Add { config: cfg },
                                    ))
                                    .await
                                    .context("send config to database keeper")
                                },
                                |res| {
                                    if let Err(err) = res {
                                        tracing::error!("{err}");
                                    }
                                    Message::Noop
                                },
                            )
                        } else {
                            Task::none()
                        },
                        Task::perform(
                            async move {
                                tokio::task::spawn_blocking(move || {
                                    config_loader::save_config(&config)
                                })
                                .await
                                .context("Failed to save config")
                                .flatten()
                                .map_err(|err| err.to_string())
                            },
                            |res| {
                                if let Err(err) = res {
                                    tracing::error!(err);
                                }
                                Message::Noop
                            },
                        ),
                    ])
                }
                connection_config::Message::Delete(config) => {
                    if let Some(index) = self
                        .app_config
                        .connections
                        .iter()
                        .position(|cfg| cfg.id == config.id)
                    {
                        self.app_config.connections.remove(index);
                    }
                    let tx_opt = self.database_keeper_actor_tx.clone();
                    Task::batch([
                        if let Some(mut tx) = tx_opt {
                            Task::perform(
                                async move {
                                    tx.send(DatabaseKeeperMessage::ConnectionAction(
                                        database_keeper::ConnectionAction::Delete { config },
                                    ))
                                    .await
                                    .context("send config to database keeper")
                                },
                                |res| {
                                    if let Err(err) = res {
                                        tracing::error!("{err}");
                                    }
                                    Message::Noop
                                },
                            )
                        } else {
                            Task::none()
                        },
                        self.save_config(),
                    ])
                }
            },
            Message::DialogMessage(connection_dialog::DialogMessage::DialogSaved(config)) => {
                // if exists update otherwise add
                if let Some(index) = self
                    .app_config
                    .connections
                    .iter()
                    .position(|cfg| cfg.id == config.id)
                {
                    self.app_config.connections[index] = config.clone();
                } else {
                    self.app_config.connections.push(config.clone());
                };
                let tx_opt = self.database_keeper_actor_tx.clone();
                Task::batch([
                    if let Some(mut tx) = tx_opt {
                        Task::perform(
                            async move {
                                tx.send(DatabaseKeeperMessage::ConnectionAction(
                                    database_keeper::ConnectionAction::Add { config },
                                ))
                                .await
                                .context("send config to database keeper")
                            },
                            |res| {
                                if let Err(err) = res {
                                    tracing::error!("{err}");
                                }
                                Message::Noop
                            },
                        )
                    } else {
                        Task::none()
                    },
                    self.save_config(),
                ])
                .chain(Task::done(Message::DialogMessage(
                    connection_dialog::DialogMessage::DialogClose,
                )))
            }
            Message::DialogMessage(dialog_message) => self
                .dialog
                .update(dialog_message)
                .map(Message::DialogMessage),
        }
    }

    fn save_config(&self) -> Task<Message> {
        let mut config = self.app_config.clone();
        config.agent_config = self.agent_config.clone();
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

    fn send_loaded_config(
        &self,
        mut tx: Sender<DatabaseKeeperMessage>,
        configs: Vec<crate::components::connection_config::ConnectionConfig>,
    ) -> Task<Message> {
        Task::perform(
            async move {
                tx.send(DatabaseKeeperMessage::LoadedConfig { configs })
                    .await
                    .context("DatabaseKeeperActor failed on LoadedConfig message")
            },
            |result| {
                if let Err(err) = result {
                    error!("{err}")
                }
                Message::Noop
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

    pub fn database_keeper_subscription(&self) -> Subscription<Message> {
        struct DatabaseKeeperSub;
        Subscription::run_with(std::any::TypeId::of::<DatabaseKeeperSub>(), |_| {
            iced::stream::channel(
                1000,
                |mut output: iced::futures::channel::mpsc::Sender<Message>| async move {
                    let (tx, rx) = iced::futures::channel::mpsc::channel(1000);
                    let _ = output.try_send(Message::DatabaseKeeperReady(tx));
                    let mut actor = DatabaseKeeper::new(rx, output);
                    actor.run().await;
                },
            )
        })
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
            container(dialog.map(|msg| Message::DialogMessage(msg)))
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
        if self.app_config.connections.is_empty() {
            button("Add Connection")
                .on_press(Message::AddConnection)
                .padding([8, 12])
                .width(Length::Fill)
                .into()
        } else {
            scrollable(
                Column::from_vec(
                    self.app_config
                        .connections
                        .iter()
                        .map(|item| item.view().map(Message::ConnectionConfig).into())
                        .collect(),
                )
                .spacing(12),
            )
            .direction(scrollable::Direction::Vertical(
                scrollable::Scrollbar::new().width(4).scroller_width(4),
            ))
            .into()
        }
    }

    fn view_welcome(&self) -> Element<'_, Message> {
        container(
            column![
                column![
                    text("pgeru").size(48).font(iced::Font {
                        weight: iced::font::Weight::Bold,
                        ..iced::Font::DEFAULT
                    }),
                    text("PostgreSQL client").size(18).color(theme::TEXT_MUTED),
                ],
                container(rule::horizontal(1)).width(400),
                container(self.view_configs())
                    .width(Length::Fit.max(400.0))
                    .height(Length::Fit.max(400.0))
            ]
            .spacing(10)
            .align_x(iced::Alignment::Center),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(iced::Alignment::Center)
        .align_y(iced::Alignment::Center)
        .into()
    }

    fn view_main(&self) -> Element<'_, Message> {
        let body: Element<Message> = if let Some(editor) = self.editor.view() {
            editor.map(Message::Editor)
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
        .map(|maybe_provider| maybe_provider.filter(|provider| !provider.api_key.is_empty()))
        .flatten()
        .collect();

        let buttons: Vec<Element<'_, Message>> = providers
            .iter()
            .map(|provider| {
                button(text(provider.base_provider.to_string()).size(13))
                    .on_press(Message::AgentProviderSelected((*provider).clone()))
                    .width(Length::Fill)
                    .style(|_theme, _status| button::Style {
                        border: Border::default().rounded(0),
                        ..button::subtle(_theme, _status)
                    })
                    .into()
            })
            .collect();

        let menu_content = if buttons.is_empty() {
            container(text("No providers configured").size(13)).padding([6, 12])
        } else {
            container(column(buttons).spacing(0)).padding(0)
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
