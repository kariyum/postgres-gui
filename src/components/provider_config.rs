use iced::widget::{Column, button, column, container, row, rule, text};
use iced::{Color, Element, Length, Task, Theme};

use crate::ai_config::AIConfig;
use crate::app::Message;
use crate::components::settings_dialog::AiSettingsForm;
use crate::core::provider::{OpenCode, Provider};
use crate::theme;
use crate::ui::input_field::{InputField, InputFieldMessage};

#[derive(Clone, Debug)]
pub struct ProviderConfig {
    pub provider: Provider,
    form: AiSettingsForm,
    error: Option<String>,
}

#[derive(Clone, Debug)]
pub enum ProviderConfigMessage {
    ApiKeyField(InputFieldMessage),
}

impl ProviderConfig {
    pub fn opencode() -> Self {
        Self {
            provider: Provider::OpenCode(OpenCode::default()), // todo init with saved api_key
            form: AiSettingsForm::new(OpenCode::default().api_key),
            error: None,
        }
    }

    pub fn anthropic() -> Self {
        Self {
            provider: Provider::Anthropic {
                api_key: String::new(), // todo init with saved api_key
            },
            form: AiSettingsForm::new(String::default()),
            error: None,
        }
    }

    pub fn view(&self) -> Element<'_, ProviderConfigMessage> {
        container(
            column![
                column![
                    text(format!("{} Config", self.provider.to_string())).size(14),
                    rule::horizontal(1)
                ],
                self.form
                    .api_key
                    .view()
                    .map(ProviderConfigMessage::ApiKeyField),
            ]
            .spacing(8),
        )
        .padding([8, 12])
        .width(Length::Fill)
        .into()
    }

    pub fn update(&mut self, message: ProviderConfigMessage) {
        match message {
            ProviderConfigMessage::ApiKeyField(input_field_message) => {
                self.form.api_key.update(input_field_message)
            }
        }
    }
}
