use std::fs::{self, File};
use std::io::{BufReader, BufWriter};
use std::path::PathBuf;

use crate::core::connection_config::ConnectionConfig;

/// Returns the configuration file path: ~/.config/pgeru/connections.json
/// On Windows, it resolves to AppData/Roaming or falls back to user profile's .config.
pub fn config_path() -> Option<PathBuf> {
    let home = std::env::var("USERPROFILE")
        .ok()
        .or_else(|| std::env::var("HOME").ok())
        .map(PathBuf::from);

    if let Some(home_path) = home {
        Some(
            home_path
                .join(".config")
                .join("pgeru")
                .join("connections.json"),
        )
    } else {
        None
    }
}

pub fn load_connections() -> Vec<ConnectionConfig> {
    if let Some(path) = config_path() {
        if path.exists() {
            if let Ok(file) = File::open(&path) {
                let reader = BufReader::new(file);
                if let Ok(conns) = serde_json::from_reader(reader) {
                    return conns;
                }
            }
        }
    }
    Vec::new()
}

/// Save connections to disk.
pub fn save_connections(conns: &[ConnectionConfig]) -> Result<(), String> {
    if let Some(path) = config_path() {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("Failed to create directories: {e}"))?;
        }
        let file = File::create(&path).map_err(|e| format!("Failed to create file: {e}"))?;
        let writer = BufWriter::new(file);
        serde_json::to_writer_pretty(writer, conns)
            .map_err(|e| format!("Failed to serialize connections: {e}"))?;
        Ok(())
    } else {
        Err("Could not determine home directory".to_string())
    }
}
