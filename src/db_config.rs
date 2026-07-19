use std::fs::{self, File};
use std::io::{BufReader, BufWriter};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::core::agent_config::AIConfig;
use crate::core::connection_config::ConnectionConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub connections: Vec<ConnectionConfig>,
    #[serde(default)]
    pub zoom_multiplier: u8,
    #[serde(default)]
    pub ai: AIConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            connections: Vec::new(),
            zoom_multiplier: 0,
            ai: AIConfig::default(),
        }
    }
}

pub fn config_path() -> Option<PathBuf> {
    let home = std::env::var("USERPROFILE")
        .ok()
        .or_else(|| std::env::var("HOME").ok())
        .map(PathBuf::from);
    home.map(|h| h.join(".config").join("pgeru").join("connections.json"))
}

pub fn load_config() -> AppConfig {
    if let Some(path) = config_path() {
        if path.exists() {
            if let Ok(file) = File::open(&path) {
                let reader = BufReader::new(file);
                if let Ok(config) = serde_json::from_reader(reader) {
                    return config;
                }
            }
        }
    }
    AppConfig::default()
}

pub fn save_config(config: &AppConfig) -> Result<(), String> {
    let path = config_path().ok_or("Could not determine home directory")?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Failed to create directories: {e}"))?;
    }
    let file = File::create(&path).map_err(|e| format!("Failed to create file: {e}"))?;
    let writer = BufWriter::new(file);
    serde_json::to_writer_pretty(writer, config)
        .map_err(|e| format!("Failed to serialize config: {e}"))?;
    Ok(())
}
