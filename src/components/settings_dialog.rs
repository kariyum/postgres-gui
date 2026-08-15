use iced::widget::{Column, button, column, container, row, rule, space, text};
use iced::{Color, Element, Length, Task, Theme};
use tracing::info;

use crate::components::provider_config::{ProviderConfig, ProviderConfigMessage};
use crate::core::agent_config::AgentConfig;
use crate::ui::input_field::InputField;

#[derive(Debug, Clone)]
pub struct SettingsDialog {
    pub visible: bool,
    pub opencode_config: ProviderConfig,
    pub anthropic_config: ProviderConfig,
}

#[derive(Debug, Clone)]
pub enum SettingsMessage {
    Open,
    OpenCodeConfigMessage(ProviderConfigMessage),
    AnthropicConfigMessage(ProviderConfigMessage),
    AgentConfig(AgentConfig),
    Save,
    Close,
    Saved,
}

#[derive(Debug, Clone)]
pub struct AgentSettingsForm {
    pub api_key: InputField,
}

impl AgentSettingsForm {
    pub fn new(api_key: String) -> Self {
        Self {
            api_key: InputField::default()
                .placeholder("API key".into())
                .label("API Key".into())
                .value(api_key)
                .secure(true),
        }
    }
}

#[derive(Debug)]
pub enum Action {
    None,
    Run(Task<SettingsMessage>),
    SaveRequested {
        config: AgentConfig,
        fetch_models: Task<SettingsMessage>,
    },
}

impl Default for SettingsDialog {
    fn default() -> Self {
        Self {
            visible: false,
            opencode_config: ProviderConfig::opencode(),
            anthropic_config: ProviderConfig::anthropic(),
        }
    }
}

impl SettingsDialog {
    fn view_sidebar(&self) -> Element<'_, SettingsMessage> {
        Column::from_iter(
            vec![&self.opencode_config, &self.anthropic_config]
                .iter()
                .map(|item| text(item.provider.label()).size(12).into()),
        )
        .padding([8, 12])
        .spacing(12)
        .width(140)
        .into()
    }

    pub fn view(&self) -> Option<Element<'_, SettingsMessage>> {
        if !self.visible {
            return None;
        }

        let form = column![
            container("Settings").padding([8, 12]),
            rule::horizontal(1),
            row![
                self.view_sidebar(),
                rule::vertical(1),
                column![
                    self.opencode_config
                        .view()
                        .map(SettingsMessage::OpenCodeConfigMessage),
                    self.anthropic_config
                        .view()
                        .map(SettingsMessage::AnthropicConfigMessage),
                ]
                .spacing(4)
                .padding([8, 12])
                .width(Length::Fill)
            ],
            rule::horizontal(1),
            container(
                row![
                    space::horizontal(),
                    button(text("Cancel").size(12))
                        .on_press(SettingsMessage::Close)
                        .padding([4, 8])
                        .style(iced::widget::button::secondary),
                    button(text("Save").size(12))
                        .on_press(SettingsMessage::Save)
                        .padding([4, 8]),
                ]
                .spacing(10)
            )
            .padding([8, 12])
        ];

        Some(
            container(form)
                .style(|theme: &Theme| {
                    let palette = theme.palette();
                    container::Style {
                        background: Some(palette.background.base.color.into()),
                        border: iced::Border {
                            color: palette.background.strong.color,
                            width: 1.0,
                            radius: 5.0.into(),
                        },
                        shadow: iced::Shadow {
                            color: Color::from_rgba(0.0, 0.0, 0.0, 0.4),
                            offset: iced::Vector::new(0.0, 8.0),
                            blur_radius: 24.0,
                        },
                        ..Default::default()
                    }
                })
                .width(Length::Fixed(640.0))
                .height(Length::Fixed(440.0))
                .into(),
        )
    }

    pub fn update(&mut self, message: SettingsMessage) -> Action {
        match message {
            SettingsMessage::Open => {
                self.visible = true;
                Action::None
            }
            SettingsMessage::Save => {
                let config = AgentConfig {
                    anthropic_config: self.anthropic_config.updated_provider(),
                    opencode_config: self.opencode_config.updated_provider(),
                };
                let mut fetch_tasks: Vec<Task<SettingsMessage>> = Vec::new();

                if !self.opencode_config.form.api_key.value.is_empty()
                    && self.opencode_config.available_models.is_empty()
                {
                    fetch_tasks.push(
                        self.opencode_config
                            .update(ProviderConfigMessage::FetchModels)
                            .map(SettingsMessage::OpenCodeConfigMessage),
                    );
                }

                if !self.anthropic_config.form.api_key.value.is_empty()
                    && self.anthropic_config.available_models.is_empty()
                {
                    fetch_tasks.push(
                        self.anthropic_config
                            .update(ProviderConfigMessage::FetchModels)
                            .map(SettingsMessage::AnthropicConfigMessage),
                    );
                }

                Action::SaveRequested {
                    config,
                    fetch_models: Task::batch(fetch_tasks),
                }
            }
            SettingsMessage::Close => {
                self.visible = false;
                Action::None
            }
            SettingsMessage::Saved => {
                self.visible = false;
                Action::None
            }
            SettingsMessage::OpenCodeConfigMessage(msg) => Action::Run(
                self.opencode_config
                    .update(msg)
                    .map(SettingsMessage::OpenCodeConfigMessage),
            ),
            SettingsMessage::AnthropicConfigMessage(msg) => Action::Run(
                self.anthropic_config
                    .update(msg)
                    .map(SettingsMessage::AnthropicConfigMessage),
            ),
            SettingsMessage::AgentConfig(agent_config) => {
                info!("Agent config loaded {:?}", agent_config);
                let mut tasks: Vec<Task<SettingsMessage>> = Vec::new();
                if let Some(provider) = agent_config.anthropic_config {
                    tasks.push(
                        self.anthropic_config
                            .update(ProviderConfigMessage::InitConfig(provider))
                            .map(SettingsMessage::AnthropicConfigMessage),
                    );
                }
                if let Some(provider) = agent_config.opencode_config {
                    tasks.push(
                        self.opencode_config
                            .update(ProviderConfigMessage::InitConfig(provider))
                            .map(SettingsMessage::OpenCodeConfigMessage),
                    );
                }
                Action::Run(Task::batch(tasks))
            }
        }
    }
}
