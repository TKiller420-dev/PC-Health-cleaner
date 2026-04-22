use crate::models::{IntegrityIssue, IntegrityReport};
use chrono::Local;
use std::collections::HashSet;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;
use zip::ZipArchive;

const MAX_JSON_CHECK_SIZE: u64 = 5 * 1024 * 1024;
const MAX_FILES_INSPECT: usize = 80_000;

fn push_issue(
    issues: &mut Vec<IntegrityIssue>,
    check: &str,
    severity: &str,
    path: String,
    details: String,
    fix_hint: &str,
) {
    issues.push(IntegrityIssue {
        check: check.into(),
        severity: severity.into(),
        path,
        details,
        fix_hint: fix_hint.into(),
    });
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
                .take(MAX_FILES_INSPECT)
                .collect::<Vec<_>>()
        })
        .take(MAX_FILES_INSPECT)
        .collect()
}

fn check_pe_valid(path: &Path) -> Result<(), String> {
    let mut file = File::open(path).map_err(|e| format!("Open failed: {e}"))?;
    let mut mz = [0u8; 2];
    file.read_exact(&mut mz)
        .map_err(|e| format!("Read header failed: {e}"))?;
    if &mz != b"MZ" {
        return Err("Missing MZ signature".into());
    }

    file.seek(SeekFrom::Start(0x3C))
        .map_err(|e| format!("Seek to e_lfanew failed: {e}"))?;
    let mut pe_offset_raw = [0u8; 4];
    file.read_exact(&mut pe_offset_raw)
        .map_err(|e| format!("Read e_lfanew failed: {e}"))?;
    let pe_offset = u32::from_le_bytes(pe_offset_raw) as u64;

    let len = file
        .metadata()
        .map_err(|e| format!("Metadata failed: {e}"))?
        .len();

    if pe_offset + 4 > len {
        return Err("PE offset points outside file".into());
    }

    file.seek(SeekFrom::Start(pe_offset))
        .map_err(|e| format!("Seek to PE signature failed: {e}"))?;
    let mut pe = [0u8; 4];
    file.read_exact(&mut pe)
        .map_err(|e| format!("Read PE signature failed: {e}"))?;

    if &pe != b"PE\0\0" {
        return Err("Invalid PE signature".into());
    }

    Ok(())
}

fn maybe_json_corrupt(path: &Path) -> Result<(), String> {
    let meta = std::fs::metadata(path).map_err(|e| format!("Metadata failed: {e}"))?;
    if meta.len() > MAX_JSON_CHECK_SIZE {
        return Ok(());
    }

    let file = File::open(path).map_err(|e| format!("Open failed: {e}"))?;
    serde_json::from_reader::<_, serde_json::Value>(file)
        .map(|_| ())
        .map_err(|e| format!("JSON parse error: {e}"))
}

fn maybe_zip_corrupt(path: &Path) -> Result<(), String> {
    let file = File::open(path).map_err(|e| format!("Open failed: {e}"))?;
    let mut archive = ZipArchive::new(file).map_err(|e| format!("ZIP index parse failed: {e}"))?;
    if archive.is_empty() {
        return Err("ZIP has no entries".into());
    }

    let mut tested = 0usize;
    for idx in 0..archive.len().min(5) {
        let mut entry = archive
            .by_index(idx)
            .map_err(|e| format!("ZIP entry read failed: {e}"))?;
        let mut sink = [0u8; 256];
        let _ = entry.read(&mut sink);
        tested += 1;
    }
    if tested == 0 {
        return Err("ZIP entries unreadable".into());
    }

    Ok(())
}

fn looks_partial_download(path: &Path) -> bool {
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();
    matches!(
        ext.as_str(),
        "part" | "crdownload" | "download" | "partial" | "tmpdl"
    )
}

fn is_config_ext(path: &Path) -> bool {
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();
    matches!(ext.as_str(), "json" | "config" | "cfg")
}

fn is_executable_or_lib(path: &Path) -> bool {
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();
    matches!(ext.as_str(), "exe" | "dll")
}

fn find_orphan_app_dirs() -> Vec<String> {
    let mut issues = Vec::new();
    let mut roots = Vec::new();
    if Path::new("C:/Program Files").exists() {
        roots.push(PathBuf::from("C:/Program Files"));
    }
    if Path::new("C:/Program Files (x86)").exists() {
        roots.push(PathBuf::from("C:/Program Files (x86)"));
    }

    for root in roots {
        let read = std::fs::read_dir(root);
        let Ok(entries) = read else {
            continue;
        };

        for entry in entries.flatten().take(350) {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            let mut has_exe = false;
            let mut has_dll = false;
            let mut file_count = 0usize;

            for item in WalkDir::new(&path)
                .max_depth(2)
                .into_iter()
                .filter_map(Result::ok)
                .filter(|e| e.file_type().is_file())
                .take(600)
            {
                file_count += 1;
                let ext = item
                    .path()
                    .extension()
                    .and_then(|s| s.to_str())
                    .map(|s| s.to_ascii_lowercase())
                    .unwrap_or_default();
                if ext == "exe" {
                    has_exe = true;
                }
                if ext == "dll" {
                    has_dll = true;
                }
                if has_exe && has_dll {
                    break;
                }
            }

            if has_dll && !has_exe && file_count > 10 {
                issues.push(path.to_string_lossy().to_string());
            }
        }
    }

    issues
}

pub fn run_integrity_checks(roots: &[PathBuf], deep: bool) -> IntegrityReport {
    let files = collect_files(roots);
    let mut issues = Vec::new();
    let mut checked_name_dirs = HashSet::new();

    for path in &files {
        let path_str = path.to_string_lossy().to_string();

        let meta = match std::fs::metadata(path) {
            Ok(m) => m,
            Err(err) => {
                push_issue(
                    &mut issues,
                    "Metadata Access",
                    "critical",
                    path_str,
                    format!("Metadata read failed: {err}"),
                    "Check file system permissions and disk health.",
                );
                continue;
            }
        };

        if meta.len() == 0 {
            push_issue(
                &mut issues,
                "Zero-byte File",
                "warning",
                path_str.clone(),
                "File length is zero bytes.".into(),
                "Validate if this file is expected; reinstall app if needed.",
            );
        }

        if looks_partial_download(path) {
            push_issue(
                &mut issues,
                "Partial Download Artifact",
                "info",
                path_str.clone(),
                "Download appears incomplete or interrupted.".into(),
                "Retry the download and remove stale partial artifacts.",
            );
        }

        if is_executable_or_lib(path) {
            if meta.len() < 4096 {
                push_issue(
                    &mut issues,
                    "Tiny Executable/Library",
                    "critical",
                    path_str.clone(),
                    "Executable/library size is unexpectedly small.".into(),
                    "Reinstall this application from a trusted source.",
                );
            }

            if let Err(err) = check_pe_valid(path) {
                push_issue(
                    &mut issues,
                    "PE Header Corruption",
                    "critical",
                    path_str.clone(),
                    err,
                    "Replace or reinstall the affected executable/library.",
                );
            }
        }

        if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
            let ext = ext.to_ascii_lowercase();
            if ext == "zip" {
                if let Err(err) = maybe_zip_corrupt(path) {
                    push_issue(
                        &mut issues,
                        "ZIP Archive Integrity",
                        "warning",
                        path_str.clone(),
                        err,
                        "Re-extract or re-download this archive.",
                    );
                }
            }
        }

        if is_config_ext(path) {
            if let Err(err) = maybe_json_corrupt(path) {
                push_issue(
                    &mut issues,
                    "Config Parse Error",
                    "warning",
                    path_str.clone(),
                    err,
                    "Repair syntax or restore from backup.",
                );
            }
        }

        let parent_key = path
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        if !parent_key.is_empty() && checked_name_dirs.insert(parent_key.clone()) {
            let mut same_stem_diff_size = 0usize;
            let mut stems: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
            if let Ok(read_dir) = std::fs::read_dir(&parent_key) {
                for e in read_dir.flatten().take(180) {
                    let p = e.path();
                    if !p.is_file() {
                        continue;
                    }
                    let stem = p
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .map(|s| s.to_ascii_lowercase())
                        .unwrap_or_default();
                    let size = std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
                    if let Some(existing) = stems.get(&stem) {
                        if *existing != size {
                            same_stem_diff_size += 1;
                        }
                    } else {
                        stems.insert(stem, size);
                    }
                }
            }
            if same_stem_diff_size > 4 {
                push_issue(
                    &mut issues,
                    "Sibling Version Clash",
                    "info",
                    parent_key.clone(),
                    format!(
                        "Found multiple files with same stem and differing sizes ({} clashes).",
                        same_stem_diff_size
                    ),
                    "Review folder for mixed partial versions and stale extractions.",
                );
            }
        }
    }

    if deep {
        for dir in find_orphan_app_dirs() {
            push_issue(
                &mut issues,
                "Orphan App Folder",
                "warning",
                dir,
                "App folder has libraries but no executable entrypoint.".into(),
                "Repair or uninstall/reinstall this app package.",
            );
        }

        let startup_dirs = [
            std::env::var("APPDATA")
                .ok()
                .map(|v| {
                    Path::new(&v)
                        .join("Microsoft")
                        .join("Windows")
                        .join("Start Menu")
                        .join("Programs")
                        .join("Startup")
                }),
            Some(PathBuf::from(
                "C:/ProgramData/Microsoft/Windows/Start Menu/Programs/StartUp",
            )),
        ];

        for dir in startup_dirs.into_iter().flatten() {
            if !dir.exists() {
                continue;
            }
            let mut unresolved_links = 0usize;
            for item in WalkDir::new(&dir)
                .max_depth(1)
                .into_iter()
                .filter_map(Result::ok)
                .filter(|e| e.file_type().is_file())
            {
                let ext = item
                    .path()
                    .extension()
                    .and_then(|s| s.to_str())
                    .map(|s| s.to_ascii_lowercase())
                    .unwrap_or_default();
                if ext == "lnk" || ext == "url" {
                    unresolved_links += 1;
                }
            }
            if unresolved_links > 40 {
                push_issue(
                    &mut issues,
                    "Startup Entry Saturation",
                    "warning",
                    dir.to_string_lossy().to_string(),
                    format!("Startup folder contains {} shortcut entries.", unresolved_links),
                    "Trim startup shortcuts and keep only essential launchers.",
                );
            }
        }
    }

    let critical_count = issues.iter().filter(|i| i.severity == "critical").count();
    let warning_count = issues.iter().filter(|i| i.severity == "warning").count();
    let info_count = issues.iter().filter(|i| i.severity == "info").count();

    let mut score = 100i64;
    score -= (critical_count as i64) * 7;
    score -= (warning_count as i64) * 3;
    score -= info_count as i64;
    if score < 5 {
        score = 5;
    }

    IntegrityReport {
        generated_at: Local::now().to_rfc3339(),
        roots_scanned: roots.to_vec(),
        files_scanned: files.len(),
        issues,
        integrity_score: score as u8,
        critical_count,
        warning_count,
        info_count,
        check_count: if deep { 12 } else { 9 },
    }
}
