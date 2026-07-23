use iced::widget::{Column, button, column, container, row, rule, space, text};
use iced::{Color, Element, Length, Task, Theme};

use crate::app::Message;
use crate::components::provider_config::{ProviderConfig, ProviderConfigMessage};
use crate::core::agent_config::AgentConfig;
use crate::core::configured_provider::{BaseProvider, ConfiguredProvider};
use crate::core::provider::Provider;
use crate::ui::input_field::InputField;

#[derive(Debug, Clone)]
pub struct SettingsDialog {
    pub visible: bool,
    opencode_config: ProviderConfig,
    anthropic_config: ProviderConfig,
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
                    let palette = theme.extended_palette();
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

    pub fn update(&mut self, message: SettingsMessage) -> Task<Message> {
        match message {
            SettingsMessage::Open => {
                self.visible = true;
                Task::none()
            }
            SettingsMessage::Save => {
                let mut providers: Vec<ConfiguredProvider> = Vec::new();
                if let Some(configured_provider) = self.anthropic_config.updated_provider() {
                    providers.push(configured_provider);
                }
                if let Some(configured_provider) = self.opencode_config.updated_provider() {
                    providers.push(configured_provider);
                }
                let agent_config = AgentConfig { providers };
                Task::done(Message::SaveAgentSettings(agent_config))
            }
            SettingsMessage::Close => {
                self.visible = false;
                Task::none()
            }
            SettingsMessage::Saved => {
                self.visible = false;
                Task::none()
            }
            SettingsMessage::OpenCodeConfigMessage(msg) => {
                self.opencode_config.update(msg);
                Task::none()
            }
            SettingsMessage::AnthropicConfigMessage(msg) => {
                self.anthropic_config.update(msg);
                Task::none()
            }
            SettingsMessage::AgentConfig(agent_config) => {
                eprintln!("Agent config loaded {:?}", agent_config);
                for provider in agent_config.providers {
                    if let BaseProvider::Anthropic = &provider.base_provider {
                        self.anthropic_config
                            .update(ProviderConfigMessage::InitConfig(provider));
                    } else if let BaseProvider::OpenCode = &provider.base_provider {
                        self.opencode_config
                            .update(ProviderConfigMessage::InitConfig(provider));
                    }
                }
                Task::none()
            }
        }
    }
}
