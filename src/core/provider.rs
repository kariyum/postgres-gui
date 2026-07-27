use serde::{Deserialize, Serialize};

use crate::core::agent_client;
use crate::core::configured_provider::{BaseProvider, ConfiguredProvider};

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

    pub fn from_config(config: &ConfiguredProvider) -> Self {
        match config.base_provider {
            BaseProvider::OpenCode => Provider::OpenCode(OpenCode {
                api_key: Some(config.api_key.clone()),
                base_url: config
                    .base_url
                    .clone()
                    .unwrap_or_else(|| "https://opencode.ai/zen/v1".to_string()),
            }),
            BaseProvider::Anthropic => Provider::Anthropic {
                api_key: Some(config.api_key.clone()),
            },
        }
    }

    pub async fn load_models(&self) -> anyhow::Result<Vec<String>> {
        match self {
            Provider::OpenCode(OpenCode {
                api_key: Some(api_key),
                base_url,
                ..
            }) => {
                agent_client::list_opencode_models(api_key.to_string(), Some(base_url.to_string()))
                    .await
            }
            Provider::Anthropic {
                api_key: Some(api_key),
            } => agent_client::list_anthropic_models(api_key.to_string()).await,
            _ => Err(anyhow::anyhow!(
                "Cannot load models: missing API key"
            )),
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
