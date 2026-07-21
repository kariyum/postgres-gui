use anyhow::Context;
use rig_core::model::ModelList;
use serde::{Deserialize, Serialize};

use crate::core::agent_client;

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

    pub async fn load_models(&self) -> anyhow::Result<ModelList> {
        if let Provider::OpenCode(OpenCode {
            api_key: Some(api_key),
            base_url,
            ..
        }) = self
        {
            agent_client::list_models(api_key.to_string(), Some(base_url.to_string()))
                .await
                .context("Failed to fetch model list")
        } else {
            Err(anyhow::anyhow!("load_models not implemented for Anthropic"))
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OpenCode {
    pub api_key: Option<String>,
    pub base_url: String,
    #[serde(default)]
    pub models: Vec<ModelList>,
}

impl Default for OpenCode {
    fn default() -> Self {
        Self {
            api_key: None,
            base_url: String::from("https://opencode.ai/zen/v1"),
            models: vec![],
        }
    }
}
