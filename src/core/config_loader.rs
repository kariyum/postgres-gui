use std::fs::{self, File};
use std::io::{BufReader, BufWriter};
use std::path::PathBuf;

use anyhow::Context;
use serde::{Deserialize, Serialize};

use crate::core::agent_config::AgentConfig;
use crate::core::connection_config::ConnectionConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub connections: Vec<ConnectionConfig>,
    #[serde(default)]
    pub zoom_multiplier: u8,
    #[serde(default)]
    pub agent_config: AgentConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            connections: Vec::new(),
            zoom_multiplier: 0,
            agent_config: AgentConfig::default(),
        }
    }
}

pub fn config_path() -> Option<PathBuf> {
    let home = std::env::var("USERPROFILE")
        .ok()
        .or_else(|| std::env::var("HOME").ok())
        .map(PathBuf::from);
    home.map(|path| path.join(".config").join("pgeru").join("connections.json"))
}

pub fn load_config() -> anyhow::Result<AppConfig> {
    if let Some(path) = config_path()
        && path.exists()
    {
        let file = File::open(&path).context("Failed to open file")?;
        let reader = BufReader::new(file);
        serde_json::from_reader(reader).context("Failed to deserialize")
    } else {
        Ok(AppConfig::default())
    }
}

pub fn save_config(config: &AppConfig) -> anyhow::Result<()> {
    let path = config_path().context("Could not determine home directory")?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).context("Failed to create directories")?;
    }
    let file = File::create(&path).context("Failed to create file")?;
    let writer = BufWriter::new(file);
    serde_json::to_writer_pretty(writer, config).context("Failed to serialize config")?;
    Ok(())
}
