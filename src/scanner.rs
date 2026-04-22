use crate::models::{DuplicateGroup, LeftoverHit, ScanSummary};
use blake3::Hasher;
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{Read, Result as IoResult};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

fn hash_file(path: &Path) -> IoResult<String> {
    let mut file = File::open(path)?;
    let mut hasher = Hasher::new();
    let mut buffer = [0u8; 8192];

    loop {
        let bytes_read = file.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    Ok(hasher.finalize().to_hex().to_string())
}

fn collect_files(roots: &[PathBuf]) -> Vec<PathBuf> {
    roots
        .iter()
        .flat_map(|root| {
            WalkDir::new(root)
                .into_iter()
                .filter_map(Result::ok)
                .filter(|entry| entry.file_type().is_file())
                .map(|entry| entry.path().to_path_buf())
                .collect::<Vec<_>>()
        })
        .collect()
}

fn is_archive_ext(ext: &str) -> bool {
    matches!(
        ext.to_ascii_lowercase().as_str(),
        "zip" | "rar" | "7z" | "tar" | "gz" | "bz2" | "xz" | "iso"
    )
}

fn detect_leftovers(files: &[PathBuf]) -> Vec<LeftoverHit> {
    let mut leftovers = Vec::new();
    let mut archive_stems: HashMap<String, PathBuf> = HashMap::new();

    for file in files {
        if let Some(ext) = file.extension().and_then(|s| s.to_str()) {
            if is_archive_ext(ext) {
                if let Some(stem) = file.file_stem().and_then(|s| s.to_str()) {
                    archive_stems.insert(stem.to_ascii_lowercase(), file.clone());
                }
            }
        }

        if let Some(name) = file.file_name().and_then(|n| n.to_str()) {
            let lowered = name.to_ascii_lowercase();
            if lowered.contains("setup") && lowered.ends_with(".exe") {
                leftovers.push(LeftoverHit {
                    category: "Installer Leftover".into(),
                    path: file.clone(),
                    reason: "Executable installer likely no longer needed after install".into(),
                });
            }
            if lowered.ends_with(".tmp") || lowered.ends_with(".old") || lowered.ends_with(".bak") {
                leftovers.push(LeftoverHit {
                    category: "Temporary/Backup Leftover".into(),
                    path: file.clone(),
                    reason: "Temporary or backup artifact".into(),
                });
            }
        }
    }

    for file in files {
        if let Some(parent) = file.parent() {
            if let Some(dir_name) = parent.file_name().and_then(|n| n.to_str()) {
                let lowered = dir_name.to_ascii_lowercase();
                if lowered.contains("extract") || lowered.contains("unpack") {
                    leftovers.push(LeftoverHit {
                        category: "Extraction Leftover".into(),
                        path: file.clone(),
                        reason: "File appears inside extraction workspace".into(),
                    });
                }
            }
        }

        if let Some(stem) = file.file_stem().and_then(|s| s.to_str()) {
            let key = stem.to_ascii_lowercase();
            if let Some(archive_path) = archive_stems.get(&key) {
                leftovers.push(LeftoverHit {
                    category: "Archive + Extracted Pair".into(),
                    path: file.clone(),
                    reason: format!(
                        "Possible extracted copy while archive still exists at {}",
                        archive_path.display()
                    ),
                });
            }
        }
    }

    leftovers
}

pub fn run_scan(roots: &[PathBuf]) -> ScanSummary {
    let files = collect_files(roots);
    let mut ext_hist: HashMap<String, usize> = HashMap::new();
    for file in &files {
        let ext = file
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| format!(".{}", s.to_ascii_lowercase()))
            .unwrap_or_else(|| "[no-ext]".into());
        *ext_hist.entry(ext).or_insert(0) += 1;
    }

    let metadata: Vec<(PathBuf, u64)> = files
        .iter()
        .filter_map(|path| std::fs::metadata(path).ok().map(|m| (path.clone(), m.len())))
        .collect();

    let mut by_size: HashMap<u64, Vec<PathBuf>> = HashMap::new();
    for (path, len) in &metadata {
        by_size.entry(*len).or_default().push(path.clone());
    }

    let candidate_files: Vec<PathBuf> = by_size
        .values()
        .filter(|group| group.len() > 1)
        .flat_map(|group| group.iter().cloned())
        .collect();

    let hashed: Vec<(String, PathBuf, u64)> = candidate_files
        .par_iter()
        .filter_map(|path| {
            let size = std::fs::metadata(path).ok()?.len();
            let hash = hash_file(path).ok()?;
            Some((hash, path.clone(), size))
        })
        .collect();

    let mut by_hash: HashMap<String, Vec<(PathBuf, u64)>> = HashMap::new();
    for (hash, path, size) in hashed {
        by_hash.entry(hash).or_default().push((path, size));
    }

    let mut duplicate_groups = Vec::new();
    let mut duplicate_waste_bytes = 0_u64;

    for (hash, mut entries) in by_hash {
        if entries.len() > 1 {
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            let size = entries.first().map(|(_, s)| *s).unwrap_or(0);
            let files: Vec<PathBuf> = entries.iter().map(|(p, _)| p.clone()).collect();
            duplicate_waste_bytes += size.saturating_mul((files.len().saturating_sub(1)) as u64);
            duplicate_groups.push(DuplicateGroup {
                hash,
                total_bytes: size.saturating_mul(files.len() as u64),
                files,
            });
        }
    }

    duplicate_groups.sort_by(|a, b| b.total_bytes.cmp(&a.total_bytes));

    let leftovers = detect_leftovers(&files);

    ScanSummary {
        roots_scanned: roots.to_vec(),
        files_seen: files.len(),
        duplicate_groups,
        duplicate_waste_bytes,
        leftovers,
        extension_breakdown: {
            let mut items: Vec<(String, usize)> = ext_hist.into_iter().collect();
            items.sort_by(|a, b| b.1.cmp(&a.1));
            items
        },
    }
}

fn is_ignored(path: &Path, ignored_paths: &[String], ignored_exts: &[String]) -> bool {
    let path_lower = path.to_string_lossy().to_ascii_lowercase();
    if ignored_paths
        .iter()
        .map(|p| p.to_ascii_lowercase())
        .any(|needle| path_lower.contains(&needle))
    {
        return true;
    }

    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| format!(".{}", s.to_ascii_lowercase()));
    if let Some(ext) = ext {
        return ignored_exts
            .iter()
            .map(|e| e.to_ascii_lowercase())
            .any(|needle| needle == ext);
    }

    false
}

pub fn cleanup_candidates(
    summary: &ScanSummary,
    limit: usize,
    ignored_paths: &[String],
    ignored_exts: &[String],
) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();

    for group in &summary.duplicate_groups {
        for candidate in group.files.iter().skip(1) {
            if !candidate.exists() || is_ignored(candidate, ignored_paths, ignored_exts) {
                continue;
            }
            if seen.insert(candidate.clone()) {
                out.push(candidate.clone());
                if out.len() >= limit {
                    return out;
                }
            }
        }
    }

    for hit in &summary.leftovers {
        let candidate = &hit.path;
        if !candidate.exists() || is_ignored(candidate, ignored_paths, ignored_exts) {
            continue;
        }
        if seen.insert(candidate.clone()) {
            out.push(candidate.clone());
            if out.len() >= limit {
                return out;
            }
        }
    }

    out
}
