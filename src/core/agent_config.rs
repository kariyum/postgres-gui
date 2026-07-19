use serde::{Deserialize, Serialize};

use crate::core::provider::Provider;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentConfig {
    pub provider: Vec<Provider>,
}
