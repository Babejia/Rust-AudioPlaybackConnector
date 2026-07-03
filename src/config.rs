//! Configuration persistence (JSON).
//! Saves/loads `reconnect` flag and `lastDevices` list.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

const CONFIG_NAME: &str = "AudioPlaybackConnector.json";

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct AppConfig {
    #[serde(default)]
    pub reconnect: bool,
    #[serde(default)]
    pub last_devices: Vec<String>,
}

/// Get config file path (same directory as executable)
pub fn config_path() -> PathBuf {
    let exe = std::env::current_exe().expect("failed to get exe path");
    let dir = exe.parent().expect("failed to get exe dir");
    dir.join(CONFIG_NAME)
}

pub fn load_config() -> AppConfig {
    let path = config_path();
    match fs::read_to_string(&path) {
        Ok(content) => {
            serde_json::from_str(&content).unwrap_or_else(|e| {
                eprintln!("[config] parse error: {} — using defaults", e);
                AppConfig::default()
            })
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            AppConfig::default()
        }
        Err(e) => {
            eprintln!("[config] read error: {} — using defaults", e);
            AppConfig::default()
        }
    }
}

pub fn save_config(config: &AppConfig) {
    let path = config_path();
    match serde_json::to_string_pretty(config) {
        Ok(content) => {
            if let Err(e) = fs::write(&path, &content) {
                eprintln!("[config] write error: {}", e);
            }
        }
        Err(e) => {
            eprintln!("[config] serialize error: {}", e);
        }
    }
}
