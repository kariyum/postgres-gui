use std::fmt;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase", tag = "type")]
pub enum BaseProvider {
    OpenCode,
    Anthropic,
}

impl fmt::Display for BaseProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BaseProvider::OpenCode => write!(f, "OpenCode"),
            BaseProvider::Anthropic => write!(f, "Anthropic"),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConfiguredProvider {
    pub api_key: String,
    #[serde(flatten)]
    pub base_provider: BaseProvider,
    pub default_model: Option<String>,
    pub base_url: Option<String>,
}
