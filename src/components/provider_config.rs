use iced::widget::{column, container, pick_list, rule, text};
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
    ModelSelected(String),
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
        let mut content = column![
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

        if !self.form.api_key.value.is_empty() {
            if self.models_loading {
                content = content.push(text("Loading models...").size(12));
            } else if self.available_models.is_empty() {
                content = content.push(
                    iced::widget::button(text("Fetch models").size(12))
                        .on_press(ProviderConfigMessage::FetchModels)
                        .padding([4, 8]),
                );
            } else {
                content = content.push(
                    column![
                        text("Default Model").size(12),
                        pick_list(
                            self.available_models.clone(),
                            self.selected_model.clone(),
                            ProviderConfigMessage::ModelSelected,
                        )
                        .placeholder("Select a model")
                        .width(Length::Fill),
                    ]
                    .spacing(4),
                );
            }
        }

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
                self.form.api_key.update(InputFieldMessage::InputChanged(
                    configured_provider.api_key.to_string(),
                ));
                self.selected_model = configured_provider.default_model;
                Task::none()
            }
            ProviderConfigMessage::FetchModels => {
                self.models_loading = true;
                self.error = None;
                let provider = self.provider.clone();
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
            ProviderConfigMessage::ModelSelected(model) => {
                self.selected_model = Some(model);
                Task::none()
            }
        }
    }

    pub fn updated_provider(&self) -> Option<ConfiguredProvider> {
        if self.form.api_key.value.is_empty() {
            return None;
        }
        match &self.provider {
            Provider::OpenCode(open_code) => Some(ConfiguredProvider {
                api_key: self.form.api_key.value.clone(),
                base_url: Some(open_code.base_url.clone()),
                default_model: self.selected_model.clone(),
                base_provider: BaseProvider::OpenCode,
            }),
            Provider::Anthropic { .. } => Some(ConfiguredProvider {
                api_key: self.form.api_key.value.clone(),
                base_url: None,
                default_model: self.selected_model.clone(),
                base_provider: BaseProvider::Anthropic,
            }),
        }
    }
}
