use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, USER_AGENT};
use semver::Version;
use serde::Deserialize;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

const RELEASES_API_URL: &str = "https://api.github.com/repos/TKiller420-dev/PC-Health-cleaner/releases/latest";

#[derive(Debug, Clone)]
pub enum UpdateStatus {
    UpToDate,
    Downloaded,
    Unavailable,
    Checking,
    Error,
}

#[derive(Debug, Clone)]
pub struct UpdateCheckResult {
    pub status: UpdateStatus,
    pub message: String,
    pub downloaded_path: Option<PathBuf>,
    pub downloaded_version: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GitHubReleaseAsset {
    name: String,
    browser_download_url: String,
}

#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    assets: Vec<GitHubReleaseAsset>,
}

fn normalize_version(raw: &str) -> String {
    raw.trim_start_matches(['v', 'V']).trim().to_string()
}

fn current_version() -> Version {
    Version::parse(env!("CARGO_PKG_VERSION")).unwrap_or_else(|_| Version::new(0, 0, 0))
}

fn parse_version(raw: &str) -> Option<Version> {
    Version::parse(&normalize_version(raw)).ok()
}

fn is_packaged_windows_build() -> bool {
    if !cfg!(target_os = "windows") {
        return false;
    }

    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    let lower = exe.to_string_lossy().to_ascii_lowercase();
    !lower.contains("\\target\\debug\\") && !lower.contains("\\target\\release\\")
}

fn updates_dir() -> PathBuf {
    let base = std::env::var("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir());
    base.join("NexusPcCleaner").join("updates")
}

fn fetch_latest_release() -> Result<GitHubRelease, String> {
    let mut headers = HeaderMap::new();
    headers.insert(ACCEPT, HeaderValue::from_static("application/vnd.github+json"));
    headers.insert(USER_AGENT, HeaderValue::from_static("Cyb3rWrld-Checkers-Baby-Edition"));

    let client = Client::builder()
        .default_headers(headers)
        .build()
        .map_err(|e| format!("HTTP client build failed: {e}"))?;

    let response = client
        .get(RELEASES_API_URL)
        .send()
        .map_err(|e| format!("Release lookup failed: {e}"))?;

    if !response.status().is_success() {
        return Err(format!("Release lookup failed with {}.", response.status()));
    }

    response
        .json::<GitHubRelease>()
        .map_err(|e| format!("Release parse failed: {e}"))
}

fn download_asset(download_url: &str, destination: &Path) -> Result<(), String> {
    let mut headers = HeaderMap::new();
    headers.insert(ACCEPT, HeaderValue::from_static("application/octet-stream"));
    headers.insert(USER_AGENT, HeaderValue::from_static("Cyb3rWrld-Checkers-Baby-Edition"));

    let client = Client::builder()
        .default_headers(headers)
        .build()
        .map_err(|e| format!("HTTP client build failed: {e}"))?;

    let mut response = client
        .get(download_url)
        .send()
        .map_err(|e| format!("Update download failed: {e}"))?;

    if !response.status().is_success() {
        return Err(format!("Update download failed with {}.", response.status()));
    }

    let mut output = fs::File::create(destination)
        .map_err(|e| format!("Failed to create update file {}: {e}", destination.display()))?;

    response
        .copy_to(&mut output)
        .map_err(|e| format!("Failed writing update file: {e}"))?;

    Ok(())
}

pub fn check_for_updates(
    manual: bool,
    downloaded_update_path: Option<PathBuf>,
    downloaded_update_version: Option<String>,
) -> UpdateCheckResult {
    if !is_packaged_windows_build() {
        return UpdateCheckResult {
            status: UpdateStatus::Unavailable,
            message: "Updates can only be checked from the packaged Windows build.".into(),
            downloaded_path: downloaded_update_path,
            downloaded_version: downloaded_update_version,
        };
    }

    if let (Some(path), version) = (downloaded_update_path.clone(), downloaded_update_version.clone()) {
        if path.exists() {
            let version_label = version.unwrap_or_else(|| "latest version".into());
            return UpdateCheckResult {
                status: UpdateStatus::Downloaded,
                message: if manual {
                    format!("Update {} is already downloaded and ready to install.", version_label)
                } else {
                    format!("Update {} is downloaded and waiting for restart.", version_label)
                },
                downloaded_path: Some(path),
                downloaded_version: Some(version_label),
            };
        }
    }

    let latest_release = match fetch_latest_release() {
        Ok(r) => r,
        Err(e) => {
            return UpdateCheckResult {
                status: UpdateStatus::Error,
                message: e,
                downloaded_path: None,
                downloaded_version: None,
            }
        }
    };

    let latest_version_raw = normalize_version(&latest_release.tag_name);
    let latest_version = match parse_version(&latest_version_raw) {
        Some(v) => v,
        None => {
            return UpdateCheckResult {
                status: UpdateStatus::Error,
                message: format!("Invalid latest release version: {}", latest_release.tag_name),
                downloaded_path: None,
                downloaded_version: None,
            }
        }
    };

    if latest_version <= current_version() {
        return UpdateCheckResult {
            status: UpdateStatus::UpToDate,
            message: "You already have the latest build.".into(),
            downloaded_path: None,
            downloaded_version: Some(latest_version_raw),
        };
    }

    let executable_asset = latest_release
        .assets
        .iter()
        .find(|asset| asset.name.to_ascii_lowercase().ends_with(".exe"));

    let Some(asset) = executable_asset else {
        return UpdateCheckResult {
            status: UpdateStatus::Error,
            message: "Latest release is missing a Windows executable.".into(),
            downloaded_path: None,
            downloaded_version: Some(latest_version_raw),
        };
    };

    let update_dir = updates_dir();
    let _ = fs::create_dir_all(&update_dir);
    let update_path = update_dir.join(&asset.name);

    if let Err(e) = download_asset(&asset.browser_download_url, &update_path) {
        return UpdateCheckResult {
            status: UpdateStatus::Error,
            message: e,
            downloaded_path: None,
            downloaded_version: Some(latest_version_raw),
        };
    }

    UpdateCheckResult {
        status: UpdateStatus::Downloaded,
        message: format!("Downloaded update {}. Restart to install it.", latest_version_raw),
        downloaded_path: Some(update_path),
        downloaded_version: Some(latest_version_raw),
    }
}

pub fn install_downloaded_update(update_path: &Path) -> Result<(), String> {
    let executable_path = std::env::current_exe().map_err(|e| format!("Current exe not found: {e}"))?;

    let script_path = std::env::temp_dir().join(format!(
        "pc-health-cleaner-update-{}.cmd",
        chrono::Local::now().timestamp_millis()
    ));

    let quoted_update = format!("\"{}\"", update_path.display());
    let quoted_exe = format!("\"{}\"", executable_path.display());
    let quoted_script = format!("\"{}\"", script_path.display());

    let script = [
        "@echo off".to_string(),
        "setlocal".to_string(),
        ":retry".to_string(),
        format!("copy /Y {} {} >nul 2>nul", quoted_update, quoted_exe),
        "if errorlevel 1 (".to_string(),
        "  timeout /t 1 /nobreak >nul".to_string(),
        "  goto retry".to_string(),
        ")".to_string(),
        format!("del /F /Q {} >nul 2>nul", quoted_update),
        format!("start \"\" {}", quoted_exe),
        format!("del /F /Q {} >nul 2>nul", quoted_script),
    ]
    .join("\r\n");

    let mut file = fs::File::create(&script_path)
        .map_err(|e| format!("Failed to create update script {}: {e}", script_path.display()))?;
    file.write_all(script.as_bytes())
        .map_err(|e| format!("Failed to write update script: {e}"))?;

    Command::new("cmd")
        .args(["/C", script_path.to_string_lossy().as_ref()])
        .spawn()
        .map_err(|e| format!("Failed to launch updater script: {e}"))?;

    Ok(())
}
