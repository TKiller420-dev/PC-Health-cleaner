use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DuplicateGroup {
    pub hash: String,
    pub total_bytes: u64,
    pub files: Vec<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeftoverHit {
    pub category: String,
    pub path: PathBuf,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanSummary {
    pub roots_scanned: Vec<PathBuf>,
    pub files_seen: usize,
    pub duplicate_groups: Vec<DuplicateGroup>,
    pub duplicate_waste_bytes: u64,
    pub leftovers: Vec<LeftoverHit>,
    pub extension_breakdown: Vec<(String, usize)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthMetric {
    pub name: String,
    pub score: u8,
    pub details: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthReport {
    pub generated_at: String,
    pub overall_score: u8,
    pub metrics: Vec<HealthMetric>,
    pub warnings: Vec<String>,
    pub recommendations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub schedule_enabled: bool,
    pub schedule_minutes: u64,
    pub auto_cleanup_limit: usize,
    pub ignored_extensions: Vec<String>,
    pub ignored_paths: Vec<String>,
    pub last_scheduled_run: Option<String>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            schedule_enabled: false,
            schedule_minutes: 240,
            auto_cleanup_limit: 80,
            ignored_extensions: vec![".sys".into(), ".dll".into()],
            ignored_paths: vec!["C:/Windows".into(), "Program Files".into()],
            last_scheduled_run: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HistoryEntry {
    pub timestamp: String,
    pub overall_health: Option<u8>,
    pub duplicate_groups: usize,
    pub leftover_hits: usize,
    pub duplicate_waste_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuarantineEntry {
    pub original_path: PathBuf,
    pub quarantined_path: PathBuf,
    pub timestamp: String,
    pub restored: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CleanupMode {
    Quarantine,
    Delete,
    SecureDelete,
}

#[derive(Debug, Clone, Default)]
pub struct CleanupResult {
    pub moved: usize,
    pub restored: usize,
    pub deleted: usize,
    pub failed: usize,
    pub logs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrityIssue {
    pub check: String,
    pub severity: String,
    pub path: String,
    pub details: String,
    pub fix_hint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrityReport {
    pub generated_at: String,
    pub roots_scanned: Vec<PathBuf>,
    pub files_scanned: usize,
    pub issues: Vec<IntegrityIssue>,
    pub integrity_score: u8,
    pub critical_count: usize,
    pub warning_count: usize,
    pub info_count: usize,
    pub check_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunMode {
    Quick,
    Deep,
}
