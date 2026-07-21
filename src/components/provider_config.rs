use iced::widget::{column, container, rule, text};
use iced::{Element, Length, Task};

use crate::components::settings_dialog::AgentSettingsForm;
use crate::core::provider::{OpenCode, Provider};
use crate::ui::input_field::InputFieldMessage;

#[derive(Clone, Debug)]
pub struct ProviderConfig {
    pub provider: Provider,
    form: AgentSettingsForm,
    error: Option<String>,
}

#[derive(Clone, Debug)]
pub enum ProviderConfigMessage {
    ApiKeyField(InputFieldMessage),
    InitConfig(Provider),
}

impl ProviderConfig {
    pub fn opencode() -> Self {
        Self {
            provider: Provider::OpenCode(OpenCode::default()), // todo init with saved api_key
            form: AgentSettingsForm::new(OpenCode::default().api_key.unwrap_or_default()),
            error: None,
        }
    }

    pub fn anthropic() -> Self {
        Self {
            provider: Provider::Anthropic {
                api_key: None, // todo init with saved api_key
            },
            form: AgentSettingsForm::new(String::default()),
            error: None,
        }
    }

    pub fn view(&self) -> Element<'_, ProviderConfigMessage> {
        container(
            column![
                column![
                    text(format!("{} Config", self.provider.label())).size(14),
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
            ProviderConfigMessage::InitConfig(provider) => match (&provider, &self.provider) {
                (
                    Provider::OpenCode(OpenCode {
                        api_key: Some(api_key),
                        ..
                    }),
                    Provider::OpenCode(_),
                ) => {
                    self.form
                        .api_key
                        .update(InputFieldMessage::InputChanged(api_key.to_string()));
                }
                (
                    Provider::Anthropic {
                        api_key: Some(api_key),
                    },
                    Provider::Anthropic { .. },
                ) => {
                    self.form
                        .api_key
                        .update(InputFieldMessage::InputChanged(api_key.to_string()));
                }
                _ => (),
            },
        }
    }

    pub fn updated_provider(&self) -> Provider {
        match &self.provider {
            Provider::OpenCode(open_code) => Provider::OpenCode(OpenCode {
                api_key: Some(self.form.api_key.value.clone()),
                base_url: open_code.base_url.clone(),
                models: open_code.models.clone()
            }),
            Provider::Anthropic { .. } => Provider::Anthropic {
                api_key: Some(self.form.api_key.value.clone()),
            },
        }
    }
}
