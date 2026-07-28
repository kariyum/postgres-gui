use serde::{Deserialize, Serialize};

use crate::core::configured_provider::ConfiguredProvider;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentConfig {
    #[serde(default)]
    pub anthropic_config: Option<ConfiguredProvider>,
    #[serde(default)]
    pub opencode_config: Option<ConfiguredProvider>,
}
