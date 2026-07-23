use serde::{Deserialize, Serialize};

use crate::core::{configured_provider::ConfiguredProvider, provider::Provider};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentConfig {
    pub providers: Vec<ConfiguredProvider>,
}
