use serde::{Deserialize, Serialize};

pub const SYSTEM_PROMPT: &str = "You are a PostgreSQL expert assistant. \
    You help users write SQL queries, understand database schemas, \
    and analyze query results.";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiConfig {
    pub endpoint: String,
    #[serde(default)]
    pub api_key: Option<String>,
    pub model: String,
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            endpoint: "https://ollama.com".into(),
            api_key: None,
            model: "gpt-oss:120b".into(),
        }
    }
}
