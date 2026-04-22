use crate::models::{CleanupMode, CleanupResult, QuarantineEntry};
use crate::storage;
use chrono::Local;
use std::fs::{self, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

fn sanitize_for_fs(path: &Path) -> String {
    path.to_string_lossy()
        .replace(':', "")
        .replace("\\", "_")
        .replace('/', "_")
}

fn unique_quarantine_target(path: &Path) -> PathBuf {
    let ts = Local::now().format("%Y%m%d_%H%M%S").to_string();
    let safe = sanitize_for_fs(path);
    storage::quarantine_dir().join(format!("{}_{}", ts, safe))
}

pub fn cleanup_paths(paths: &[PathBuf], mode: CleanupMode) -> CleanupResult {
    let mut result = CleanupResult::default();
    let mut index = storage::load_quarantine_index();

    for path in paths {
        if !path.exists() {
            continue;
        }

        match mode {
            CleanupMode::Quarantine => {
                let target = unique_quarantine_target(path);
                if fs::rename(path, &target).is_ok() {
                    result.moved += 1;
                    index.push(QuarantineEntry {
                        original_path: path.clone(),
                        quarantined_path: target,
                        timestamp: Local::now().to_rfc3339(),
                        restored: false,
                    });
                } else {
                    result.failed += 1;
                    result.logs.push(format!("Failed to quarantine {}", path.display()));
                }
            }
            CleanupMode::Delete => {
                let delete_ok = if path.is_dir() {
                    fs::remove_dir_all(path).is_ok()
                } else {
                    fs::remove_file(path).is_ok()
                };
                if delete_ok {
                    result.deleted += 1;
                } else {
                    result.failed += 1;
                    result.logs.push(format!("Failed to delete {}", path.display()));
                }
            }
            CleanupMode::SecureDelete => {
                let secure_ok = if path.is_file() {
                    secure_delete_file(path)
                } else {
                    fs::remove_dir_all(path).is_ok()
                };
                if secure_ok {
                    result.deleted += 1;
                } else {
                    result.failed += 1;
                    result.logs.push(format!("Failed secure delete {}", path.display()));
                }
            }
        }
    }

    storage::save_quarantine_index(&index);
    result
}

fn secure_delete_file(path: &Path) -> bool {
    let Ok(mut file) = OpenOptions::new().write(true).open(path) else {
        return false;
    };
    let Ok(meta) = file.metadata() else {
        return false;
    };
    let mut remaining = meta.len();
    let zeros = [0u8; 8192];

    if file.seek(SeekFrom::Start(0)).is_err() {
        return false;
    }

    while remaining > 0 {
        let write_size = remaining.min(zeros.len() as u64) as usize;
        if file.write_all(&zeros[..write_size]).is_err() {
            return false;
        }
        remaining -= write_size as u64;
    }

    if file.flush().is_err() {
        return false;
    }

    fs::remove_file(path).is_ok()
}

pub fn restore_quarantine(limit: usize) -> CleanupResult {
    let mut result = CleanupResult::default();
    let mut index = storage::load_quarantine_index();

    let mut restore_candidates: Vec<usize> = index
        .iter()
        .enumerate()
        .filter_map(|(idx, item)| (!item.restored).then_some(idx))
        .collect();

    restore_candidates.reverse();
    restore_candidates.truncate(limit);

    for idx in restore_candidates {
        let item = &mut index[idx];
        if !item.quarantined_path.exists() {
            item.restored = true;
            continue;
        }

        if let Some(parent) = item.original_path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        if fs::rename(&item.quarantined_path, &item.original_path).is_ok() {
            item.restored = true;
            result.restored += 1;
        } else {
            result.failed += 1;
            result
                .logs
                .push(format!("Failed restore {}", item.original_path.display()));
        }
    }

    storage::save_quarantine_index(&index);
    result
}

pub fn list_quarantine() -> Vec<QuarantineEntry> {
    storage::load_quarantine_index()
}

pub fn restore_quarantine_item(unrestored_offset: usize) -> CleanupResult {
    let mut result = CleanupResult::default();
    let mut index = storage::load_quarantine_index();

    let mut unresolved: Vec<usize> = index
        .iter()
        .enumerate()
        .filter_map(|(idx, item)| (!item.restored).then_some(idx))
        .collect();
    unresolved.reverse();

    let Some(target_idx) = unresolved.get(unrestored_offset).copied() else {
        result.failed += 1;
        result.logs.push("No quarantine item at requested index".into());
        return result;
    };

    let item = &mut index[target_idx];
    if !item.quarantined_path.exists() {
        item.restored = true;
        storage::save_quarantine_index(&index);
        return result;
    }

    if let Some(parent) = item.original_path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    if fs::rename(&item.quarantined_path, &item.original_path).is_ok() {
        item.restored = true;
        result.restored = 1;
    } else {
        result.failed = 1;
        result
            .logs
            .push(format!("Failed restore {}", item.original_path.display()));
    }

    storage::save_quarantine_index(&index);
    result
}
