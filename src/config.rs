//! Configuration persistence (JSON).
//! Saves/loads `reconnect`, `lastDevices` and the phone-gate settings.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

const CONFIG_NAME: &str = "AudioPlaybackConnector.json";

/// 手机音轨自动静音门（详见 volume.rs）
#[derive(Serialize, Deserialize, Clone)]
pub struct GateSettings {
    /// 是否启用自动静音门
    #[serde(default = "d_true")]
    pub enabled: bool,
    /// 低于该电平视为“安静”（dBFS，负值）
    #[serde(default = "d_threshold")]
    pub threshold_db: f32,
    /// 高于该电平立即恢复（dBFS）
    #[serde(default = "d_release")]
    pub release_db: f32,
    /// 安静持续多久后静音（毫秒）
    #[serde(default = "d_hold")]
    pub hold_ms: u32,
    /// 手动指定音轨名匹配关键字；留空则自动识别（推荐）
    #[serde(default)]
    pub match_key: String,
    /// 当前连接的 A2DP 设备名，运行时自动填入，不落盘
    #[serde(skip)]
    pub device_name: String,
    /// 全局热键，例如 "Pause" / "Ctrl+Alt+F12"；留空表示不注册
    #[serde(default = "d_hotkey")]
    pub hotkey: String,
}

fn d_true() -> bool { true }
fn d_threshold() -> f32 { -34.0 }
fn d_release() -> f32 { -30.0 }
fn d_hold() -> u32 { 4000 }
fn d_hotkey() -> String { "Pause".to_string() }

impl Default for GateSettings {
    fn default() -> Self {
        GateSettings {
            enabled: true,
            threshold_db: d_threshold(),
            release_db: d_release(),
            hold_ms: d_hold(),
            match_key: String::new(),
            device_name: String::new(),
            hotkey: d_hotkey(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct AppConfig {
    #[serde(default)]
    pub reconnect: bool,
    #[serde(default)]
    pub last_devices: Vec<String>,
    #[serde(flatten)]
    pub gate: GateSettings,
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
