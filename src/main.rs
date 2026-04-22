mod cleanup;
mod health;
mod integrity;
mod models;
mod scanner;
mod storage;

use eframe::egui::{
    self, Align, CentralPanel, Color32, Context, Frame, Layout, ProgressBar, RichText, ScrollArea,
    SidePanel, Stroke, TopBottomPanel,
};
use eframe::{App, NativeOptions};
use models::{
    AppConfig, CleanupMode, HealthReport, HistoryEntry, IntegrityIssue, IntegrityReport, RunMode,
    ScanSummary,
};
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn now_rfc3339() -> String {
    chrono::Local::now().to_rfc3339()
}

fn minutes_since(then_rfc3339: &str) -> Option<i64> {
    let parsed = chrono::DateTime::parse_from_rfc3339(then_rfc3339).ok()?;
    let then_local = parsed.with_timezone(&chrono::Local);
    let delta = chrono::Local::now() - then_local;
    Some(delta.num_minutes())
}

fn bytes_to_gb(bytes: u64) -> f64 {
    bytes as f64 / 1024.0 / 1024.0 / 1024.0
}

fn health_grade(score: u8) -> &'static str {
    if score >= 93 {
        "A+"
    } else if score >= 86 {
        "A"
    } else if score >= 78 {
        "B"
    } else if score >= 68 {
        "C"
    } else if score >= 55 {
        "D"
    } else {
        "F"
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UiTab {
    Dashboard,
    Cleaner,
    Health,
    Integrity,
    Automation,
    Tools,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ThemePreset {
    Amber,
    Matrix,
    Ice,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DuplicateSort {
    Waste,
    Copies,
    Path,
}

struct CandidateRow {
    path: PathBuf,
    source: String,
    reason: String,
    risk: u8,
}

struct CleanerApp {
    scan_roots_input: String,
    scan_summary: Option<ScanSummary>,
    health_report: Option<HealthReport>,
    config: AppConfig,
    history: Vec<HistoryEntry>,
    cleanup_mode: CleanupMode,
    export_file_name: String,
    status: String,
    tab: UiTab,
    theme: ThemePreset,
    duplicate_sort: DuplicateSort,
    candidate_filter: String,
    destructive_phrase: String,
    selected_quarantine_idx: usize,
    selected_root_preset: usize,
    notes: String,
    integrity_report: Option<IntegrityReport>,
    integrity_deep: bool,
    integrity_filter: String,
    show_only_critical: bool,
    issue_limit: usize,
    selected_issue_idx: usize,
    integrity_export_file: String,
    integrity_baseline_score: Option<u8>,
}

impl Default for CleanerApp {
    fn default() -> Self {
        let home = home::home_dir().unwrap_or_else(|| PathBuf::from("C:/"));
        let desktop = home.join("Desktop");
        let downloads = home.join("Downloads");
        let docs = home.join("Documents");

        let default_roots = [desktop, downloads, docs]
            .into_iter()
            .filter(|p| p.exists())
            .map(|p| p.to_string_lossy().to_string())
            .collect::<Vec<_>>()
            .join("; ");

        let config = storage::load_config();

        Self {
            scan_roots_input: default_roots,
            scan_summary: None,
            health_report: None,
            config,
            history: storage::load_history(),
            cleanup_mode: CleanupMode::Quarantine,
            export_file_name: "nexus_report.json".into(),
            status: "Ready. Run Deep Health and Scan to activate the full toolkit.".into(),
            tab: UiTab::Dashboard,
            theme: ThemePreset::Matrix,
            duplicate_sort: DuplicateSort::Waste,
            candidate_filter: String::new(),
            destructive_phrase: String::new(),
            selected_quarantine_idx: 0,
            selected_root_preset: 0,
            notes: String::new(),
            integrity_report: None,
            integrity_deep: true,
            integrity_filter: String::new(),
            show_only_critical: false,
            issue_limit: 400,
            selected_issue_idx: 0,
            integrity_export_file: "nexus_integrity_report.json".into(),
            integrity_baseline_score: None,
        }
    }
}

impl CleanerApp {
    fn apply_theme(&self, ctx: &Context) {
        let mut style = (*ctx.style()).clone();
        match self.theme {
            ThemePreset::Amber => {
                style.visuals.window_fill = Color32::from_rgb(28, 18, 10);
                style.visuals.panel_fill = Color32::from_rgb(38, 24, 14);
                style.visuals.widgets.inactive.bg_fill = Color32::from_rgb(61, 38, 20);
                style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(90, 55, 31);
                style.visuals.widgets.active.bg_fill = Color32::from_rgb(123, 72, 39);
                style.visuals.widgets.noninteractive.fg_stroke =
                    Stroke::new(1.0, Color32::from_rgb(255, 202, 121));
            }
            ThemePreset::Matrix => {
                style.visuals.window_fill = Color32::from_rgb(7, 15, 17);
                style.visuals.panel_fill = Color32::from_rgb(10, 24, 28);
                style.visuals.widgets.inactive.bg_fill = Color32::from_rgb(20, 43, 49);
                style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(28, 62, 70);
                style.visuals.widgets.active.bg_fill = Color32::from_rgb(40, 82, 90);
                style.visuals.widgets.noninteractive.fg_stroke =
                    Stroke::new(1.0, Color32::from_rgb(150, 220, 205));
            }
            ThemePreset::Ice => {
                style.visuals.window_fill = Color32::from_rgb(12, 17, 28);
                style.visuals.panel_fill = Color32::from_rgb(18, 25, 40);
                style.visuals.widgets.inactive.bg_fill = Color32::from_rgb(35, 48, 73);
                style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(52, 70, 103);
                style.visuals.widgets.active.bg_fill = Color32::from_rgb(66, 87, 128);
                style.visuals.widgets.noninteractive.fg_stroke =
                    Stroke::new(1.0, Color32::from_rgb(166, 215, 255));
            }
        }
        ctx.set_style(style);
    }

    fn parse_roots(&self) -> Vec<PathBuf> {
        self.scan_roots_input
            .split(';')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
            .filter(|p| p.exists())
            .collect()
    }

    fn apply_root_preset(&mut self) {
        let home = home::home_dir().unwrap_or_else(|| PathBuf::from("C:/"));
        let preset_paths = match self.selected_root_preset {
            1 => vec![home.join("Downloads")],
            2 => vec![home.join("Desktop")],
            3 => vec![home.join("Documents")],
            4 => std::env::var("TEMP").map(PathBuf::from).into_iter().collect(),
            5 => vec![home.join("Videos"), home.join("Pictures")],
            _ => vec![home.join("Desktop"), home.join("Downloads"), home.join("Documents")],
        };
        let values = preset_paths
            .into_iter()
            .filter(|p| p.exists())
            .map(|p| p.to_string_lossy().to_string())
            .collect::<Vec<_>>()
            .join("; ");
        if !values.is_empty() {
            self.scan_roots_input = values;
            self.status = "Root preset applied.".into();
        }
    }

    fn run_clean_scan(&mut self) {
        let roots = self.parse_roots();
        if roots.is_empty() {
            self.status = "No valid roots found. Use semicolon-separated folder paths.".into();
            return;
        }

        self.status = "Running duplicate + leftover scan...".into();
        let summary = scanner::run_scan(&roots);
        self.scan_summary = Some(summary);
        self.record_history();
        self.status = "Scan complete. Cleaner intelligence updated.".into();
    }

    fn run_health(&mut self, mode: RunMode) {
        self.status = match mode {
            RunMode::Quick => "Running quick health check...".into(),
            RunMode::Deep => "Running deep extensive health check...".into(),
        };
        self.health_report = Some(health::run_health_check(mode));
        self.record_history();
        self.status = "Health check complete.".into();
    }

    fn save_config(&mut self) {
        storage::save_config(&self.config);
    }

    fn record_history(&mut self) {
        let entry = HistoryEntry {
            timestamp: now_rfc3339(),
            overall_health: self.health_report.as_ref().map(|h| h.overall_score),
            duplicate_groups: self
                .scan_summary
                .as_ref()
                .map(|s| s.duplicate_groups.len())
                .unwrap_or(0),
            leftover_hits: self
                .scan_summary
                .as_ref()
                .map(|s| s.leftovers.len())
                .unwrap_or(0),
            duplicate_waste_bytes: self
                .scan_summary
                .as_ref()
                .map(|s| s.duplicate_waste_bytes)
                .unwrap_or(0),
        };
        storage::append_history(entry.clone());
        self.history.push(entry);
        if self.history.len() > 300 {
            let start = self.history.len().saturating_sub(300);
            self.history = self.history[start..].to_vec();
        }
    }

    fn run_scheduled_if_due(&mut self) {
        if !self.config.schedule_enabled {
            self.status = "Schedule is disabled.".into();
            return;
        }
        let due = match &self.config.last_scheduled_run {
            Some(last) => minutes_since(last)
                .map(|mins| mins >= self.config.schedule_minutes as i64)
                .unwrap_or(true),
            None => true,
        };

        if !due {
            self.status = "Scheduled run is not due yet.".into();
            return;
        }

        self.run_health(RunMode::Deep);
        self.run_clean_scan();
        self.config.last_scheduled_run = Some(now_rfc3339());
        self.save_config();
        self.status = "Scheduled deep run completed.".into();
    }

    fn cleanup_preview(&self) -> (usize, f64) {
        let Some(summary) = &self.scan_summary else {
            return (0, 0.0);
        };
        let candidates = scanner::cleanup_candidates(
            summary,
            self.config.auto_cleanup_limit,
            &self.config.ignored_paths,
            &self.config.ignored_extensions,
        );

        let estimate = summary.duplicate_waste_bytes as f64 / 1024.0 / 1024.0 / 1024.0;
        (candidates.len(), estimate)
    }

    fn apply_one_click_cleanup(&mut self) {
        let Some(summary) = &self.scan_summary else {
            self.status = "Run scan before cleanup.".into();
            return;
        };

        if (self.cleanup_mode == CleanupMode::Delete || self.cleanup_mode == CleanupMode::SecureDelete)
            && self.destructive_phrase.trim() != "CONFIRM"
        {
            self.status = "Type CONFIRM to allow Delete/Secure Delete modes.".into();
            return;
        }

        let candidates = scanner::cleanup_candidates(
            summary,
            self.config.auto_cleanup_limit,
            &self.config.ignored_paths,
            &self.config.ignored_extensions,
        );

        if candidates.is_empty() {
            self.status = "No cleanup candidates after ignore filters.".into();
            return;
        }

        let result = cleanup::cleanup_paths(&candidates, self.cleanup_mode);
        self.status = format!(
            "Cleanup complete. moved:{} deleted:{} failed:{}",
            result.moved, result.deleted, result.failed
        );
    }

    fn restore_recent_quarantine(&mut self) {
        let result = cleanup::restore_quarantine(self.config.auto_cleanup_limit.min(50));
        self.status = format!(
            "Restore complete. restored:{} failed:{}",
            result.restored, result.failed
        );
    }

    fn restore_selected_quarantine(&mut self) {
        let result = cleanup::restore_quarantine_item(self.selected_quarantine_idx);
        self.status = format!(
            "Selected restore done. restored:{} failed:{}",
            result.restored, result.failed
        );
    }

    fn open_path_in_explorer(&mut self, path: PathBuf) {
        let result = Command::new("explorer").arg(path).spawn();
        if result.is_ok() {
            self.status = "Opened folder in Explorer.".into();
        } else {
            self.status = "Failed to open folder in Explorer.".into();
        }
    }

    fn clear_history(&mut self) {
        storage::clear_history();
        self.history.clear();
        self.status = "History cleared.".into();
    }

    fn filtered_candidates(&self) -> Vec<CandidateRow> {
        let mut rows = Vec::new();
        let Some(summary) = &self.scan_summary else {
            return rows;
        };

        let filter = self.candidate_filter.to_ascii_lowercase();

        let mut dupes = summary.duplicate_groups.clone();
        match self.duplicate_sort {
            DuplicateSort::Waste => {
                dupes.sort_by(|a, b| b.total_bytes.cmp(&a.total_bytes));
            }
            DuplicateSort::Copies => {
                dupes.sort_by(|a, b| b.files.len().cmp(&a.files.len()));
            }
            DuplicateSort::Path => {
                dupes.sort_by(|a, b| {
                    let pa = a.files.first().map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
                    let pb = b.files.first().map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
                    pa.cmp(&pb)
                });
            }
        }

        for g in dupes {
            for path in g.files.into_iter().skip(1) {
                let p = path.to_string_lossy().to_string();
                if !filter.is_empty() && !p.to_ascii_lowercase().contains(&filter) {
                    continue;
                }
                rows.push(CandidateRow {
                    path,
                    source: "Duplicate".into(),
                    reason: "Additional copy in duplicate group".into(),
                    risk: 45,
                });
            }
        }

        for l in &summary.leftovers {
            let p = l.path.to_string_lossy().to_string();
            if !filter.is_empty() && !p.to_ascii_lowercase().contains(&filter) {
                continue;
            }
            let risk = if l.category.contains("Extraction") {
                62
            } else if l.category.contains("Installer") {
                57
            } else {
                49
            };
            rows.push(CandidateRow {
                path: l.path.clone(),
                source: l.category.clone(),
                reason: l.reason.clone(),
                risk,
            });
        }

        rows
    }

    fn export_report(&mut self) {
        #[derive(serde::Serialize)]
        struct ExportPayload {
            exported_at: String,
            health: Option<HealthReport>,
            scan: Option<ScanSummary>,
            history_tail: Vec<HistoryEntry>,
            config: AppConfig,
            notes: String,
        }

        let payload = ExportPayload {
            exported_at: now_rfc3339(),
            health: self.health_report.clone(),
            scan: self.scan_summary.clone(),
            history_tail: self.history.iter().rev().take(20).cloned().collect(),
            config: self.config.clone(),
            notes: self.notes.clone(),
        };

        match serde_json::to_string_pretty(&payload) {
            Ok(raw) => {
                let path = PathBuf::from(self.export_file_name.trim());
                if fs::write(&path, raw).is_ok() {
                    self.status = format!("Report exported to {}", path.display());
                } else {
                    self.status = "Failed to write export file.".into();
                }
            }
            Err(_) => {
                self.status = "Failed to serialize report.".into();
            }
        }
    }

    fn run_integrity(&mut self) {
        let roots = self.parse_roots();
        if roots.is_empty() {
            self.status = "No valid scan roots for integrity checks.".into();
            return;
        }
        self.status = "Running corruption/broken-app integrity checks...".into();
        self.integrity_report = Some(integrity::run_integrity_checks(&roots, self.integrity_deep));
        self.status = "Integrity checks complete.".into();
    }

    fn visible_integrity_issues(&self) -> Vec<IntegrityIssue> {
        let Some(report) = &self.integrity_report else {
            return Vec::new();
        };
        let filter = self.integrity_filter.to_ascii_lowercase();

        report
            .issues
            .iter()
            .filter(|issue| {
                if self.show_only_critical && issue.severity != "critical" {
                    return false;
                }
                if filter.is_empty() {
                    return true;
                }
                issue.path.to_ascii_lowercase().contains(&filter)
                    || issue.check.to_ascii_lowercase().contains(&filter)
                    || issue.details.to_ascii_lowercase().contains(&filter)
            })
            .take(self.issue_limit)
            .cloned()
            .collect()
    }

    fn export_integrity_json(&mut self) {
        let Some(report) = &self.integrity_report else {
            self.status = "Run integrity checks before exporting.".into();
            return;
        };

        match serde_json::to_string_pretty(report) {
            Ok(raw) => {
                let path = PathBuf::from(self.integrity_export_file.trim());
                if fs::write(&path, raw).is_ok() {
                    self.status = format!("Integrity JSON exported to {}", path.display());
                } else {
                    self.status = "Failed to export integrity JSON.".into();
                }
            }
            Err(_) => {
                self.status = "Failed to serialize integrity report.".into();
            }
        }
    }

    fn export_integrity_csv(&mut self) {
        let Some(report) = &self.integrity_report else {
            self.status = "Run integrity checks before exporting CSV.".into();
            return;
        };

        let csv_path = PathBuf::from(self.integrity_export_file.trim().replace(".json", ".csv"));
        let mut out = String::from("check,severity,path,details,fix_hint\n");
        for issue in &report.issues {
            let row = format!(
                "\"{}\",\"{}\",\"{}\",\"{}\",\"{}\"\n",
                issue.check.replace('"', "'"),
                issue.severity.replace('"', "'"),
                issue.path.replace('"', "'"),
                issue.details.replace('"', "'"),
                issue.fix_hint.replace('"', "'")
            );
            out.push_str(&row);
        }

        if fs::write(&csv_path, out).is_ok() {
            self.status = format!("Integrity CSV exported to {}", csv_path.display());
        } else {
            self.status = "Failed to export integrity CSV.".into();
        }
    }

    fn write_repair_playbook(&mut self) {
        let Some(report) = &self.integrity_report else {
            self.status = "Run integrity checks before generating a repair playbook.".into();
            return;
        };

        let path = PathBuf::from("nexus_repair_playbook.txt");
        let mut text = String::new();
        text.push_str("Nexus Repair Playbook\n");
        text.push_str(&format!("Generated: {}\n\n", now_rfc3339()));
        for (idx, issue) in report.issues.iter().take(120).enumerate() {
            text.push_str(&format!(
                "{}. [{}] {}\nPath: {}\nFix: {}\n\n",
                idx + 1,
                issue.severity,
                issue.check,
                issue.path,
                issue.fix_hint
            ));
        }
        if fs::write(&path, text).is_ok() {
            self.status = format!("Repair playbook written to {}", path.display());
        } else {
            self.status = "Failed to write repair playbook.".into();
        }
    }

    fn set_integrity_baseline(&mut self) {
        let Some(report) = &self.integrity_report else {
            self.status = "Run integrity checks first to set a baseline.".into();
            return;
        };
        self.integrity_baseline_score = Some(report.integrity_score);
        self.status = format!("Integrity baseline set at {}", report.integrity_score);
    }

    fn add_selected_issue_parent_to_ignore(&mut self) {
        let issues = self.visible_integrity_issues();
        if issues.is_empty() {
            self.status = "No visible integrity issues to add ignore token from.".into();
            return;
        }
        let idx = self.selected_issue_idx.min(issues.len() - 1);
        let path = PathBuf::from(&issues[idx].path);
        if let Some(parent) = path.parent() {
            let token = parent.to_string_lossy().to_string();
            if !self.config.ignored_paths.iter().any(|p| p.eq_ignore_ascii_case(&token)) {
                self.config.ignored_paths.push(token.clone());
                self.save_config();
                self.status = format!("Added ignore path token from selected issue: {}", token);
            } else {
                self.status = "Selected issue parent path token is already in ignore list.".into();
            }
        } else {
            self.status = "Selected issue path has no parent directory.".into();
        }
    }

    fn open_selected_issue_folder(&mut self) {
        let issues = self.visible_integrity_issues();
        if issues.is_empty() {
            self.status = "No visible integrity issues to open.".into();
            return;
        }
        let idx = self.selected_issue_idx.min(issues.len() - 1);
        let issue_path = PathBuf::from(&issues[idx].path);
        let target = if issue_path.is_dir() {
            issue_path
        } else {
            issue_path.parent().map(PathBuf::from).unwrap_or(issue_path)
        };
        self.open_path_in_explorer(target);
    }

    fn snapshot_delta(&self) -> Option<(i64, i64, f64)> {
        if self.history.len() < 2 {
            return None;
        }
        let last = self.history.get(self.history.len() - 1)?;
        let prev = self.history.get(self.history.len() - 2)?;
        Some((
            last.duplicate_groups as i64 - prev.duplicate_groups as i64,
            last.leftover_hits as i64 - prev.leftover_hits as i64,
            bytes_to_gb(last.duplicate_waste_bytes) - bytes_to_gb(prev.duplicate_waste_bytes),
        ))
    }

    fn render_top_bar(&mut self, ctx: &Context) {
        TopBottomPanel::top("top_banner").show(ctx, |ui| {
            Frame::none()
                .fill(Color32::from_rgb(9, 28, 34))
                .inner_margin(10.0)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.heading(
                            RichText::new("NEXUS // CONTROL SURFACE")
                                .color(Color32::from_rgb(109, 247, 211))
                                .size(26.0),
                        );
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let t = ctx.input(|i| i.time);
                            let pulse = ((t * 2.2).sin() * 0.5 + 0.5) as f32;
                            let status_color = Color32::from_rgb(
                                (180.0 + pulse * 70.0) as u8,
                                (120.0 + pulse * 80.0) as u8,
                                95,
                            );
                            ui.label(RichText::new("LIVE").color(status_color).strong());
                            ui.label(
                                RichText::new("HACK PANEL MODE")
                                    .color(Color32::from_rgb(255, 205, 120))
                                    .strong(),
                            );
                        });
                    });
                });
        });
    }

    fn render_side_tabs(&mut self, ctx: &Context) {
        SidePanel::left("left_nav")
            .resizable(false)
            .default_width(170.0)
            .show(ctx, |ui| {
                ui.heading(RichText::new("Modules").color(Color32::from_rgb(142, 232, 255)));
                ui.separator();

                ui.selectable_value(&mut self.tab, UiTab::Dashboard, "Dashboard");
                ui.selectable_value(&mut self.tab, UiTab::Cleaner, "Cleaner");
                ui.selectable_value(&mut self.tab, UiTab::Health, "Health Lab");
                ui.selectable_value(&mut self.tab, UiTab::Integrity, "Integrity Lab");
                ui.selectable_value(&mut self.tab, UiTab::Automation, "Automation");
                ui.selectable_value(&mut self.tab, UiTab::Tools, "Tool Deck");

                ui.add_space(12.0);
                ui.separator();
                ui.label(RichText::new("Theme").strong());
                ui.selectable_value(&mut self.theme, ThemePreset::Matrix, "Matrix");
                ui.selectable_value(&mut self.theme, ThemePreset::Amber, "Amber CRT");
                ui.selectable_value(&mut self.theme, ThemePreset::Ice, "Ice Core");

                ui.add_space(12.0);
                ui.separator();
                ui.label(RichText::new("Root Preset").strong());
                ui.selectable_value(&mut self.selected_root_preset, 0, "Balanced");
                ui.selectable_value(&mut self.selected_root_preset, 1, "Downloads");
                ui.selectable_value(&mut self.selected_root_preset, 2, "Desktop");
                ui.selectable_value(&mut self.selected_root_preset, 3, "Documents");
                ui.selectable_value(&mut self.selected_root_preset, 4, "Temp");
                ui.selectable_value(&mut self.selected_root_preset, 5, "Media");
                if ui.button("Apply Preset").clicked() {
                    self.apply_root_preset();
                }
            });
    }

    fn render_dashboard_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading(RichText::new("Mission Dashboard").size(24.0));
        ui.label("Run health and scan modules to update this board.");
        ui.add_space(8.0);

        ui.horizontal_wrapped(|ui| {
            if ui.button("Quick Health").clicked() {
                self.run_health(RunMode::Quick);
            }
            if ui.button("Deep Health").clicked() {
                self.run_health(RunMode::Deep);
            }
            if ui.button("Scan Duplicates + Leftovers").clicked() {
                self.run_clean_scan();
            }
        });

        let (candidate_count, reclaim_gb) = self.cleanup_preview();

        ui.add_space(8.0);
        ui.columns(3, |cols| {
            cols[0].group(|ui| {
                let score = self.health_report.as_ref().map(|h| h.overall_score).unwrap_or(0);
                ui.label(RichText::new("Health Grade").strong());
                ui.label(RichText::new(format!("{} ({})", health_grade(score), score)).size(28.0));
            });
            cols[1].group(|ui| {
                ui.label(RichText::new("Cleanup Preview").strong());
                ui.label(format!("Candidates: {}", candidate_count));
                ui.label(format!("Est reclaim: {:.2} GB", reclaim_gb));
            });
            cols[2].group(|ui| {
                ui.label(RichText::new("History Samples").strong());
                ui.label(format!("Stored snapshots: {}", self.history.len()));
                if let Some((d_dupes, d_left, d_waste)) = self.snapshot_delta() {
                    ui.label(format!("Delta dupes: {:+}", d_dupes));
                    ui.label(format!("Delta leftovers: {:+}", d_left));
                    ui.label(format!("Delta waste: {:+.2} GB", d_waste));
                }
            });
        });

        ui.add_space(10.0);
        ui.group(|ui| {
            ui.label(RichText::new("Operator Notes").strong());
            ui.add(
                egui::TextEdit::multiline(&mut self.notes)
                    .desired_rows(4)
                    .hint_text("Write cleanup strategy notes, exceptions, and observations..."),
            );
        });

        ui.add_space(8.0);
        ui.label(RichText::new(&self.status).color(Color32::from_rgb(236, 243, 220)));
    }

    fn render_cleaner_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading(RichText::new("Cleaner Matrix").size(24.0));
        ui.label("Precision targeting for duplicates and leftovers.");

        ui.add_space(6.0);
        ui.label(RichText::new("Scan Roots").strong());
        ui.text_edit_singleline(&mut self.scan_roots_input);

        ui.horizontal_wrapped(|ui| {
            ui.label("Sort duplicate groups:");
            ui.selectable_value(&mut self.duplicate_sort, DuplicateSort::Waste, "By Waste");
            ui.selectable_value(&mut self.duplicate_sort, DuplicateSort::Copies, "By Copies");
            ui.selectable_value(&mut self.duplicate_sort, DuplicateSort::Path, "By Path");
        });

        ui.horizontal_wrapped(|ui| {
            ui.label("Filter candidate path:");
            ui.text_edit_singleline(&mut self.candidate_filter);
        });

        ui.horizontal_wrapped(|ui| {
            if ui.button("Run Scan").clicked() {
                self.run_clean_scan();
            }
            ui.selectable_value(&mut self.cleanup_mode, CleanupMode::Quarantine, "Quarantine");
            ui.selectable_value(&mut self.cleanup_mode, CleanupMode::Delete, "Delete");
            ui.selectable_value(&mut self.cleanup_mode, CleanupMode::SecureDelete, "Secure Delete");
        });

        ui.horizontal_wrapped(|ui| {
            ui.label("Type CONFIRM for destructive modes:");
            ui.text_edit_singleline(&mut self.destructive_phrase);
            if ui.button("Apply One-Click Cleanup").clicked() {
                self.apply_one_click_cleanup();
            }
            if ui.button("Restore Recent").clicked() {
                self.restore_recent_quarantine();
            }
        });

        let rows = self.filtered_candidates();
        let risk_avg = if rows.is_empty() {
            0.0
        } else {
            rows.iter().map(|r| r.risk as u64).sum::<u64>() as f64 / rows.len() as f64
        };

        ui.add_space(6.0);
        ui.group(|ui| {
            ui.label(RichText::new("Candidate Intelligence").strong());
            ui.label(format!("Visible candidates: {}", rows.len()));
            ui.label(format!("Average risk score: {:.1}", risk_avg));
            let risk_ratio = (risk_avg / 100.0) as f32;
            ui.add(ProgressBar::new(risk_ratio).text("Risk gauge"));
        });

        ui.add_space(8.0);
        ScrollArea::vertical().max_height(460.0).show(ui, |ui| {
            for row in rows.iter().take(140) {
                ui.group(|ui| {
                    ui.label(RichText::new(&row.source).strong());
                    ui.label(row.path.to_string_lossy().to_string());
                    ui.label(&row.reason);
                    ui.label(format!("risk: {}", row.risk));
                });
            }
        });
    }

    fn render_health_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading(RichText::new("Health Lab").size(24.0));
        ui.horizontal_wrapped(|ui| {
            if ui.button("Quick Health").clicked() {
                self.run_health(RunMode::Quick);
            }
            if ui.button("Deep Health").clicked() {
                self.run_health(RunMode::Deep);
            }
        });

        if let Some(report) = &self.health_report {
            let health_color = if report.overall_score >= 80 {
                Color32::from_rgb(108, 235, 167)
            } else if report.overall_score >= 60 {
                Color32::from_rgb(255, 211, 120)
            } else {
                Color32::from_rgb(255, 123, 123)
            };
            ui.label(
                RichText::new(format!(
                    "Overall Health: {} ({})",
                    report.overall_score,
                    health_grade(report.overall_score)
                ))
                .size(22.0)
                .color(health_color)
                .strong(),
            );
            ui.add(
                ProgressBar::new(report.overall_score as f32 / 100.0)
                    .show_percentage()
                    .fill(health_color),
            );

            ui.add_space(6.0);
            ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                for metric in &report.metrics {
                    ui.group(|ui| {
                        ui.label(
                            RichText::new(format!("{} [{}]", metric.name, metric.score))
                                .strong()
                                .color(Color32::from_rgb(173, 255, 235)),
                        );
                        ui.label(&metric.details);
                    });
                }
                if !report.warnings.is_empty() {
                    ui.label(RichText::new("Warnings").color(Color32::from_rgb(255, 142, 142)).strong());
                    for warning in &report.warnings {
                        ui.label(format!("! {}", warning));
                    }
                }
                if !report.recommendations.is_empty() {
                    ui.label(RichText::new("Recommendations").strong());
                    for recommendation in &report.recommendations {
                        ui.label(format!("> {}", recommendation));
                    }
                }
            });
        } else {
            ui.label("Run health check to load metrics.");
        }

        if let Some(summary) = &self.scan_summary {
            ui.add_space(8.0);
            ui.group(|ui| {
                ui.label(RichText::new("File Type Breakdown").strong());
                for (ext, count) in summary.extension_breakdown.iter().take(10) {
                    ui.label(format!("{}: {}", ext, count));
                }
            });
        }
    }

    fn render_automation_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading(RichText::new("Automation Core").size(24.0));

        ui.horizontal_wrapped(|ui| {
            ui.checkbox(&mut self.config.schedule_enabled, "Enable schedule");
            ui.label("Interval (minutes):");
            ui.add(egui::DragValue::new(&mut self.config.schedule_minutes).range(15..=10080));
            ui.label("Batch size:");
            ui.add(egui::DragValue::new(&mut self.config.auto_cleanup_limit).range(10..=400));
        });

        ui.horizontal_wrapped(|ui| {
            if ui.button("Run Scheduled If Due").clicked() {
                self.run_scheduled_if_due();
            }
            if ui.button("Save Config").clicked() {
                self.save_config();
                self.status = "Config saved.".into();
            }
        });

        ui.horizontal_wrapped(|ui| {
            ui.label("Ignored extensions (comma):");
            let mut exts = self.config.ignored_extensions.join(",");
            if ui.text_edit_singleline(&mut exts).lost_focus() {
                self.config.ignored_extensions = exts
                    .split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string())
                    .collect();
            }
        });

        ui.horizontal_wrapped(|ui| {
            ui.label("Ignored path tokens (comma):");
            let mut paths = self.config.ignored_paths.join(",");
            if ui.text_edit_singleline(&mut paths).lost_focus() {
                self.config.ignored_paths = paths
                    .split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string())
                    .collect();
            }
        });

        ui.add_space(8.0);
        ui.group(|ui| {
            ui.label(RichText::new("History Trend").strong());
            let tail: Vec<_> = self.history.iter().rev().take(12).collect();
            for item in tail {
                let bar = "#".repeat((item.overall_health.unwrap_or(0) / 5) as usize);
                ui.label(format!(
                    "{} | {} | dup:{} left:{} waste:{:.2}GB",
                    item.timestamp,
                    bar,
                    item.duplicate_groups,
                    item.leftover_hits,
                    bytes_to_gb(item.duplicate_waste_bytes)
                ));
            }
        });
    }

    fn render_integrity_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading(RichText::new("Integrity Lab").size(24.0));
        ui.label("Corruption and broken app checks for files, archives, configs, binaries, and install trees.");

        ui.horizontal_wrapped(|ui| {
            ui.checkbox(&mut self.integrity_deep, "Deep mode");
            ui.checkbox(&mut self.show_only_critical, "Critical only");
            ui.label("Issue limit:");
            ui.add(egui::DragValue::new(&mut self.issue_limit).range(20..=2000));
            if ui.button("Run Integrity Checks").clicked() {
                self.run_integrity();
            }
            if ui.button("Set Baseline").clicked() {
                self.set_integrity_baseline();
            }
        });

        ui.horizontal_wrapped(|ui| {
            ui.label("Filter issues:");
            ui.text_edit_singleline(&mut self.integrity_filter);
            ui.label("Export file:");
            ui.text_edit_singleline(&mut self.integrity_export_file);
            if ui.button("Export JSON").clicked() {
                self.export_integrity_json();
            }
            if ui.button("Export CSV").clicked() {
                self.export_integrity_csv();
            }
        });

        ui.horizontal_wrapped(|ui| {
            if ui.button("Generate Repair Playbook").clicked() {
                self.write_repair_playbook();
            }
            if ui.button("Open Selected Issue Folder").clicked() {
                self.open_selected_issue_folder();
            }
            if ui.button("Ignore Selected Issue Parent").clicked() {
                self.add_selected_issue_parent_to_ignore();
            }
        });

        if let Some(report) = &self.integrity_report {
            let baseline_delta = self
                .integrity_baseline_score
                .map(|b| report.integrity_score as i64 - b as i64);
            ui.group(|ui| {
                ui.label(RichText::new("Integrity Summary").strong());
                ui.label(format!("Score: {}", report.integrity_score));
                if let Some(delta) = baseline_delta {
                    ui.label(format!("Delta vs baseline: {:+}", delta));
                }
                ui.label(format!("Files scanned: {}", report.files_scanned));
                ui.label(format!("Checks active: {}", report.check_count));
                ui.label(format!(
                    "Issues critical:{} warning:{} info:{}",
                    report.critical_count, report.warning_count, report.info_count
                ));
                ui.add(ProgressBar::new(report.integrity_score as f32 / 100.0).show_percentage());
            });

            let visible = self.visible_integrity_issues();
            if !visible.is_empty() {
                let max_idx = visible.len() - 1;
                if self.selected_issue_idx > max_idx {
                    self.selected_issue_idx = max_idx;
                }
                ui.horizontal_wrapped(|ui| {
                    ui.label("Selected issue:");
                    ui.add(egui::DragValue::new(&mut self.selected_issue_idx).range(0..=max_idx));
                });
            }

            ScrollArea::vertical().max_height(470.0).show(ui, |ui| {
                for issue in visible.iter().take(self.issue_limit) {
                    let severity_color = match issue.severity.as_str() {
                        "critical" => Color32::from_rgb(255, 120, 120),
                        "warning" => Color32::from_rgb(255, 205, 120),
                        _ => Color32::from_rgb(147, 225, 245),
                    };
                    ui.group(|ui| {
                        ui.label(
                            RichText::new(format!("{} [{}]", issue.check, issue.severity))
                                .strong()
                                .color(severity_color),
                        );
                        ui.label(&issue.path);
                        ui.label(&issue.details);
                        ui.label(format!("Fix: {}", issue.fix_hint));
                    });
                }
            });
        } else {
            ui.label("Run integrity checks to get corruption and broken-app findings.");
        }
    }

    fn render_tools_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading(RichText::new("Tool Deck").size(24.0));

        ui.horizontal_wrapped(|ui| {
            if ui.button("Open Quarantine Folder").clicked() {
                self.open_path_in_explorer(storage::quarantine_dir());
            }
            if ui.button("Open App Data Folder").clicked() {
                self.open_path_in_explorer(storage::app_data_dir());
            }
            if ui.button("Clear History").clicked() {
                self.clear_history();
            }
        });

        ui.horizontal_wrapped(|ui| {
            ui.label("Export file:");
            ui.text_edit_singleline(&mut self.export_file_name);
            if ui.button("Export JSON Report").clicked() {
                self.export_report();
            }
        });

        ui.add_space(8.0);
        let quarantine_items = cleanup::list_quarantine();
        let unresolved: Vec<_> = quarantine_items.iter().filter(|q| !q.restored).collect();

        ui.group(|ui| {
            ui.label(RichText::new("Quarantine Browser").strong());
            ui.label(format!("Unrestored items: {}", unresolved.len()));
            if unresolved.is_empty() {
                ui.label("No pending quarantine items.");
            } else {
                let max_idx = unresolved.len().saturating_sub(1);
                if self.selected_quarantine_idx > max_idx {
                    self.selected_quarantine_idx = max_idx;
                }
                ui.horizontal_wrapped(|ui| {
                    ui.label("Target index:");
                    ui.add(egui::DragValue::new(&mut self.selected_quarantine_idx).range(0..=max_idx));
                    if ui.button("Restore Selected").clicked() {
                        self.restore_selected_quarantine();
                    }
                });
                if let Some(item) = unresolved.get(self.selected_quarantine_idx) {
                    ui.label(format!("Original: {}", item.original_path.display()));
                    ui.label(format!("Quarantine: {}", item.quarantined_path.display()));
                    ui.label(format!("At: {}", item.timestamp));
                }
            }
        });
    }
}

impl App for CleanerApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        self.apply_theme(ctx);
        self.render_top_bar(ctx);
        self.render_side_tabs(ctx);

        CentralPanel::default().show(ctx, |ui| {
            Frame::none()
                .fill(Color32::from_rgb(12, 30, 33))
                .stroke(Stroke::new(1.0, Color32::from_rgb(81, 180, 165)))
                .inner_margin(12.0)
                .show(ui, |ui| match self.tab {
                    UiTab::Dashboard => self.render_dashboard_tab(ui),
                    UiTab::Cleaner => self.render_cleaner_tab(ui),
                    UiTab::Health => self.render_health_tab(ui),
                    UiTab::Integrity => self.render_integrity_tab(ui),
                    UiTab::Automation => self.render_automation_tab(ui),
                    UiTab::Tools => self.render_tools_tab(ui),
                });
        });
    }
}

fn main() {
    let options = NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1520.0, 920.0]),
        ..Default::default()
    };

    let _ = eframe::run_native(
        "Nexus PC Control Surface",
        options,
        Box::new(|_cc| Ok(Box::new(CleanerApp::default()))),
    );
}
