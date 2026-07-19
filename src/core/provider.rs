use iced::widget::{Column, button, column, container, row, rule, text};
use iced::{Color, Element, Length, Task, Theme};

use crate::ai_config::AIConfig;
use crate::app::Message;
use crate::components::provider_config::ProviderConfig;
use crate::theme;
use crate::ui::input_field::{InputField, InputFieldMessage};

#[derive(Clone, Debug)]
pub enum Provider {
    OpenCode(OpenCode),
    Anthropic { api_key: String },
}

#[derive(Clone)]
pub enum BaseProvider {
    OpenCode,
    Anthropic,
}

impl Provider {
    pub fn to_string(&self) -> String {
        match self {
            Provider::OpenCode(_) => String::from("OpenCode"),
            Provider::Anthropic { .. } => String::from("Anthropic"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct OpenCode {
    pub api_key: String,
    pub base_url: String,
}

impl Default for OpenCode {
    fn default() -> Self {
        Self {
            api_key: String::default(),
            base_url: String::from("https://opencode.ai/zen/v1"),
        }
    }
}
