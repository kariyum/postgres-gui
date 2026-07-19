use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AIConfig {
    pub endpoint: String,
    #[serde(default)]
    pub api_key: Option<String>,
    pub model: String,
    pub tools_enabled: bool,
}

impl Default for AIConfig {
    fn default() -> Self {
        Self {
            endpoint: "https://ollama.com".into(),
            api_key: None,
            model: "gpt-oss:120b".into(),
            tools_enabled: true,
        }
    }
}
