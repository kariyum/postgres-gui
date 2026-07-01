use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiConfig {
    pub endpoint: String,
    #[serde(default)]
    pub api_key: Option<String>,
    pub model: String,
    pub system_prompt: String,
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            endpoint: "https://ollama.com".into(),
            api_key: None,
            model: "gpt-oss:120b".into(),
            system_prompt: "You are a PostgreSQL expert assistant.".into(),
        }
    }
}
