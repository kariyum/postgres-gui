use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase", tag = "type")]
pub enum BaseProvider {
    OpenCode,
    Anthropic,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConfiguredProvider {
    pub api_key: String,
    pub base_provider: BaseProvider,
    pub default_model: Option<String>,
    pub base_url: Option<String>,
}
