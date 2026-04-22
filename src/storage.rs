use crate::models::{AppConfig, HistoryEntry};
use std::fs;
use std::path::PathBuf;

fn base_dir() -> PathBuf {
    let root = std::env::var("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir());
    root.join("NexusPcCleaner")
}

pub fn app_data_dir() -> PathBuf {
    ensure_dirs()
}

pub fn ensure_dirs() -> PathBuf {
    let dir = base_dir();
    let _ = fs::create_dir_all(dir.join("quarantine"));
    dir
}

pub fn quarantine_dir() -> PathBuf {
    ensure_dirs().join("quarantine")
}

fn config_file() -> PathBuf {
    ensure_dirs().join("config.json")
}

fn history_file() -> PathBuf {
    ensure_dirs().join("history.json")
}

fn quarantine_index_file() -> PathBuf {
    ensure_dirs().join("quarantine_index.json")
}

pub fn load_config() -> AppConfig {
    let path = config_file();
    let Ok(raw) = fs::read_to_string(path) else {
        return AppConfig::default();
    };
    serde_json::from_str(&raw).unwrap_or_else(|_| AppConfig::default())
}

pub fn save_config(config: &AppConfig) {
    if let Ok(raw) = serde_json::to_string_pretty(config) {
        let _ = fs::write(config_file(), raw);
    }
}

pub fn load_history() -> Vec<HistoryEntry> {
    let Ok(raw) = fs::read_to_string(history_file()) else {
        return Vec::new();
    };
    serde_json::from_str(&raw).unwrap_or_default()
}

pub fn append_history(entry: HistoryEntry) {
    let mut items = load_history();
    items.push(entry);
    if items.len() > 300 {
        let start = items.len().saturating_sub(300);
        items = items[start..].to_vec();
    }
    if let Ok(raw) = serde_json::to_string_pretty(&items) {
        let _ = fs::write(history_file(), raw);
    }
}

pub fn clear_history() {
    let _ = fs::write(history_file(), "[]");
}

pub fn save_quarantine_index(entries: &[crate::models::QuarantineEntry]) {
    if let Ok(raw) = serde_json::to_string_pretty(entries) {
        let _ = fs::write(quarantine_index_file(), raw);
    }
}

pub fn load_quarantine_index() -> Vec<crate::models::QuarantineEntry> {
    let Ok(raw) = fs::read_to_string(quarantine_index_file()) else {
        return Vec::new();
    };
    serde_json::from_str(&raw).unwrap_or_default()
}
