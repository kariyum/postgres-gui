use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AIConfig {
    pub endpoint: String,
    #[serde(default)]
    pub api_key: Option<String>,
    pub model: String,
    #[serde(default = "default_tools_enabled")]
    pub tools_enabled: bool,
}

fn default_tools_enabled() -> bool {
    true
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
