use iced::widget::{column, container, rule, text};
use iced::{Element, Length, Task};

use crate::components::settings_dialog::AgentSettingsForm;
use crate::core::configured_provider::{BaseProvider, ConfiguredProvider};
use crate::core::provider::{OpenCode, Provider};
use crate::ui::input_field::InputFieldMessage;

#[derive(Clone, Debug)]
pub struct ProviderConfig {
    pub provider: Provider,
    pub form: AgentSettingsForm,
    pub available_models: Vec<String>,
    pub selected_model: Option<String>,
    pub models_loading: bool,
    error: Option<String>,
}

#[derive(Clone, Debug)]
pub enum ProviderConfigMessage {
    ApiKeyField(InputFieldMessage),
    InitConfig(ConfiguredProvider),
    FetchModels,
    ModelsFetched(Result<Vec<String>, String>),
}

impl ProviderConfig {
    pub fn opencode() -> Self {
        Self {
            provider: Provider::OpenCode(OpenCode::default()),
            form: AgentSettingsForm::new(OpenCode::default().api_key.unwrap_or_default()),
            available_models: Vec::new(),
            selected_model: None,
            models_loading: false,
            error: None,
        }
    }

    pub fn anthropic() -> Self {
        Self {
            provider: Provider::Anthropic { api_key: None },
            form: AgentSettingsForm::new(String::default()),
            available_models: Vec::new(),
            selected_model: None,
            models_loading: false,
            error: None,
        }
    }

    pub fn view(&self) -> Element<'_, ProviderConfigMessage> {
        let content = column![
            column![
                text(format!("{} Config", self.provider.label())).size(14),
                rule::horizontal(1)
            ],
            self.form
                .api_key
                .view()
                .map(ProviderConfigMessage::ApiKeyField),
        ]
        .spacing(8);

        container(content)
            .padding([8, 12])
            .width(Length::Fill)
            .into()
    }

    pub fn update(&mut self, message: ProviderConfigMessage) -> Task<ProviderConfigMessage> {
        match message {
            ProviderConfigMessage::ApiKeyField(input_field_message) => {
                self.form.api_key.update(input_field_message);
                Task::none()
            }
            ProviderConfigMessage::InitConfig(configured_provider) => {
                let default_model = configured_provider.default_model.clone();
                self.form.api_key.update(InputFieldMessage::InputChanged(
                    configured_provider.api_key.to_string(),
                ));
                self.selected_model = default_model;
                if !self.form.api_key.value.is_empty()
                    && self.available_models.is_empty()
                    && !self.models_loading
                {
                    self.models_loading = true;
                    self.error = None;
                    let provider = Provider::from_config(&configured_provider);
                    Task::perform(
                        async move { provider.load_models().await },
                        |result| {
                            ProviderConfigMessage::ModelsFetched(
                                result.map_err(|e| e.to_string()),
                            )
                        },
                    )
                } else {
                    Task::none()
                }
            }
            ProviderConfigMessage::FetchModels => {
                self.models_loading = true;
                self.error = None;
                let provider = match &self.provider {
                    Provider::OpenCode(open_code) => Provider::OpenCode(OpenCode {
                        api_key: Some(self.form.api_key.value.clone()),
                        base_url: open_code.base_url.clone(),
                    }),
                    Provider::Anthropic { .. } => Provider::Anthropic {
                        api_key: Some(self.form.api_key.value.clone()),
                    },
                };
                Task::perform(
                    async move { provider.load_models().await },
                    |result| {
                        ProviderConfigMessage::ModelsFetched(
                            result.map_err(|e| e.to_string()),
                        )
                    },
                )
            }
            ProviderConfigMessage::ModelsFetched(result) => {
                self.models_loading = false;
                match result {
                    Ok(models) => {
                        if self.selected_model.is_none()
                            || !models.contains(self.selected_model.as_ref().unwrap())
                        {
                            self.selected_model = models.first().cloned();
                        }
                        self.available_models = models;
                    }
                    Err(err) => {
                        self.error = Some(err);
                        self.available_models = Vec::new();
                    }
                }
                Task::none()
            }
        }
    }

    pub fn updated_provider(&self) -> Option<ConfiguredProvider> {
        match &self.provider {
            Provider::OpenCode(open_code) => Some(ConfiguredProvider {
                api_key: self.form.api_key.value.clone(),
                base_url: Some(open_code.base_url.clone()),
                default_model: self.selected_model.clone(),
                base_provider: BaseProvider::OpenCode,
                available_models: self.available_models.clone(),
            }),
            Provider::Anthropic { .. } => Some(ConfiguredProvider {
                api_key: self.form.api_key.value.clone(),
                base_url: None,
                default_model: self.selected_model.clone(),
                base_provider: BaseProvider::Anthropic,
                available_models: self.available_models.clone(),
            }),
        }
    }
}
