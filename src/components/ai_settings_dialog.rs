use iced::widget::{button, column, container, row, text};
use iced::{Color, Element, Length, Task, Theme};

use crate::ai_config::AIConfig;
use crate::theme;
use crate::ui::input_field::{InputField, InputFieldMessage};

#[derive(Debug, Clone)]
pub struct AiSettingsDialog {
    pub visible: bool,
    pub form: AiSettingsForm,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub enum AiSettingsMessage {
    Open(AIConfig),
    EndpointField(InputFieldMessage),
    ApiKeyField(InputFieldMessage),
    ModelField(InputFieldMessage),
    Save,
    Close,
    Saved(AIConfig),
}

#[derive(Debug, Clone)]
pub struct AiSettingsForm {
    pub endpoint: InputField,
    pub api_key: InputField,
    pub model: InputField,
}

impl AiSettingsForm {
    fn new(config: &AIConfig) -> Self {
        Self {
            endpoint: InputField::default()
                .placeholder("https://ollama.com".into())
                .label("Endpoint".into())
                .value(config.endpoint.clone()),

            api_key: InputField::default()
                .placeholder("API key (optional)".into())
                .label("API Key".into())
                .value(config.api_key.clone().unwrap_or_default())
                .secure(true),

            model: InputField::default()
                .placeholder("gpt-oss:120b".into())
                .label("Model".into())
                .value(config.model.clone()),
        }
    }

    fn to_config(&self) -> AIConfig {
        let api_key = self.api_key.value.trim();
        AIConfig {
            endpoint: self.endpoint.value.trim().to_string(),
            api_key: if api_key.is_empty() {
                None
            } else {
                Some(api_key.to_string())
            },
            model: self.model.value.trim().to_string(),
            tools_enabled: true,
        }
    }
}

impl Default for AiSettingsDialog {
    fn default() -> Self {
        Self {
            visible: false,
            form: AiSettingsForm::new(&AIConfig::default()),
            error: None,
        }
    }
}

impl AiSettingsDialog {
    pub fn view(&self) -> Option<Element<'_, AiSettingsMessage>> {
        if !self.visible {
            return None;
        }

        let mut form = column![
            text("AI Settings").size(18),
            iced::widget::rule::horizontal(1),
            self.form
                .endpoint
                .view()
                .map(AiSettingsMessage::EndpointField),
            self.form
                .api_key
                .view()
                .map(AiSettingsMessage::ApiKeyField),
            self.form
                .model
                .view()
                .map(AiSettingsMessage::ModelField),
        ]
        .spacing(14)
        .padding(24)
        .width(Length::Fixed(440.0));

        if let Some(ref err) = self.error {
            form = form.push(text(err.as_str()).size(13).color(theme::DANGER));
        }

        let actions = row![
            button(text("Cancel").size(14))
                .on_press(AiSettingsMessage::Close)
                .padding([8, 18])
                .style(iced::widget::button::secondary),
            button(text("Save").size(14))
                .on_press(AiSettingsMessage::Save)
                .padding([8, 18]),
        ]
        .spacing(10);

        form = form.push(actions);

        Some(
            container(form)
                .style(|theme: &Theme| {
                    let palette = theme.extended_palette();
                    container::Style {
                        background: Some(palette.background.base.color.into()),
                        border: iced::Border {
                            color: palette.background.strong.color,
                            width: 1.0,
                            radius: 10.0.into(),
                        },
                        shadow: iced::Shadow {
                            color: Color::from_rgba(0.0, 0.0, 0.0, 0.4),
                            offset: iced::Vector::new(0.0, 8.0),
                            blur_radius: 24.0,
                        },
                        ..Default::default()
                    }
                })
                .into(),
        )
    }

    pub fn update(&mut self, message: AiSettingsMessage) -> Task<AiSettingsMessage> {
        match message {
            AiSettingsMessage::Open(config) => {
                self.visible = true;
                self.error = None;
                self.form = AiSettingsForm::new(&config);
                Task::none()
            }
            AiSettingsMessage::EndpointField(msg) => self
                .form
                .endpoint
                .update(msg)
                .map(AiSettingsMessage::EndpointField),
            AiSettingsMessage::ApiKeyField(msg) => self
                .form
                .api_key
                .update(msg)
                .map(AiSettingsMessage::ApiKeyField),
            AiSettingsMessage::ModelField(msg) => self
                .form
                .model
                .update(msg)
                .map(AiSettingsMessage::ModelField),
            AiSettingsMessage::Save => {
                if self.form.endpoint.value.trim().is_empty() {
                    self.error = Some("Endpoint cannot be empty.".into());
                    return Task::none();
                }
                if self.form.model.value.trim().is_empty() {
                    self.error = Some("Model cannot be empty.".into());
                    return Task::none();
                }
                let config = self.form.to_config();
                self.visible = false;
                self.error = None;
                Task::done(AiSettingsMessage::Saved(config))
            }
            AiSettingsMessage::Close => {
                self.visible = false;
                self.error = None;
                Task::none()
            }
            AiSettingsMessage::Saved(_) => Task::none(),
        }
    }
}
