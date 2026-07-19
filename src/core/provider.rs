use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Provider {
    OpenCode(OpenCode),
    Anthropic { api_key: Option<String> },
}

impl Provider {
    pub fn label(&self) -> String {
        match self {
            Provider::OpenCode(_) => String::from("OpenCode"),
            Provider::Anthropic { .. } => String::from("Anthropic"),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OpenCode {
    pub api_key: Option<String>,
    pub base_url: String,
}

impl Default for OpenCode {
    fn default() -> Self {
        Self {
            api_key: None,
            base_url: String::from("https://opencode.ai/zen/v1"),
        }
    }
}
