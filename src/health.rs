use crate::models::{HealthMetric, HealthReport, RunMode};
use chrono::Local;
use std::path::Path;
use std::process::Command;
use sysinfo::{Disks, System};

fn score_disk_free_ratio(free: u64, total: u64) -> (u8, String) {
    if total == 0 {
        return (50, "Disk size unavailable".into());
    }
    let ratio = free as f64 / total as f64;
    if ratio > 0.30 {
        (95, format!("Healthy free space: {:.1}%", ratio * 100.0))
    } else if ratio > 0.15 {
        (75, format!("Moderate free space: {:.1}%", ratio * 100.0))
    } else if ratio > 0.08 {
        (45, format!("Low free space: {:.1}%", ratio * 100.0))
    } else {
        (20, format!("Critical free space: {:.1}%", ratio * 100.0))
    }
}

fn collect_temp_usage(temp_dir: &Path) -> u64 {
    walkdir::WalkDir::new(temp_dir)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| e.metadata().ok().map(|m| m.len()))
        .sum()
}

fn startup_entry_count() -> usize {
    let mut count = 0_usize;
    let appdata = std::env::var("APPDATA").unwrap_or_default();
    let startup_user = Path::new(&appdata)
        .join("Microsoft")
        .join("Windows")
        .join("Start Menu")
        .join("Programs")
        .join("Startup");
    if startup_user.exists() {
        count += walkdir::WalkDir::new(startup_user)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|e| e.file_type().is_file())
            .count();
    }

    let startup_all = Path::new("C:/ProgramData/Microsoft/Windows/Start Menu/Programs/StartUp");
    if startup_all.exists() {
        count += walkdir::WalkDir::new(startup_all)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|e| e.file_type().is_file())
            .count();
    }

    count
}

fn service_stats() -> Option<(usize, usize)> {
    let output = Command::new("sc")
        .args(["query", "state=", "all"])
        .output()
        .ok()?;
    let text = String::from_utf8(output.stdout).ok()?;
    let total = text.matches("SERVICE_NAME:").count();
    let running = text.matches("RUNNING").count();
    Some((total, running))
}

pub fn run_health_check(mode: RunMode) -> HealthReport {
    let mut sys = System::new_all();
    sys.refresh_all();

    let mut metrics = Vec::new();
    let mut warnings = Vec::new();
    let mut recommendations = Vec::new();

    let disks = Disks::new_with_refreshed_list();
    let (mut total_space, mut free_space) = (0_u64, 0_u64);
    for disk in disks.list() {
        total_space += disk.total_space();
        free_space += disk.available_space();
    }
    let (disk_score, disk_details) = score_disk_free_ratio(free_space, total_space);
    if disk_score < 50 {
        warnings.push("Low disk free space can cause severe performance drops.".into());
        recommendations.push("Clean large temp and duplicate files to recover space.".into());
    }
    metrics.push(HealthMetric {
        name: "Disk Capacity".into(),
        score: disk_score,
        details: disk_details,
    });

    let used_memory = sys.used_memory();
    let total_memory = sys.total_memory();
    let mem_ratio = if total_memory > 0 {
        used_memory as f64 / total_memory as f64
    } else {
        0.0
    };
    let memory_score = if mem_ratio < 0.55 {
        90
    } else if mem_ratio < 0.75 {
        70
    } else if mem_ratio < 0.90 {
        45
    } else {
        20
    };
    if memory_score < 50 {
        warnings.push("High RAM pressure detected.".into());
        recommendations.push("Disable heavy startup/background apps and reboot regularly.".into());
    }
    metrics.push(HealthMetric {
        name: "Memory Pressure".into(),
        score: memory_score,
        details: format!("Memory usage: {:.1}%", mem_ratio * 100.0),
    });

    let cpu_usage = sys.global_cpu_usage();
    let cpu_score = if cpu_usage < 35.0 {
        90
    } else if cpu_usage < 65.0 {
        70
    } else if cpu_usage < 85.0 {
        45
    } else {
        25
    };
    if cpu_score < 50 {
        warnings.push("Sustained CPU load is high.".into());
        recommendations.push("Investigate top CPU processes and remove noisy apps.".into());
    }
    metrics.push(HealthMetric {
        name: "CPU Load".into(),
        score: cpu_score,
        details: format!("Global CPU usage: {:.1}%", cpu_usage),
    });

    if let Ok(temp) = std::env::var("TEMP") {
        let temp_bytes = collect_temp_usage(Path::new(&temp));
        let temp_gb = temp_bytes as f64 / 1024.0 / 1024.0 / 1024.0;
        let temp_score = if temp_gb < 1.0 {
            90
        } else if temp_gb < 3.0 {
            70
        } else if temp_gb < 8.0 {
            45
        } else {
            20
        };
        if temp_score < 50 {
            warnings.push("Temporary file accumulation is heavy.".into());
            recommendations.push("Purge stale temporary folders and extraction leftovers.".into());
        }
        metrics.push(HealthMetric {
            name: "Temp Storage".into(),
            score: temp_score,
            details: format!("Approx temp usage: {:.2} GB", temp_gb),
        });
    }

    if mode == RunMode::Deep {
        let process_count = sys.processes().len();
        let proc_score = if process_count < 140 {
            90
        } else if process_count < 220 {
            70
        } else if process_count < 300 {
            45
        } else {
            25
        };
        if proc_score < 50 {
            warnings.push("Excessive process count may indicate startup bloat.".into());
            recommendations.push("Audit startup entries and remove unnecessary auto-runs.".into());
        }
        metrics.push(HealthMetric {
            name: "Process Bloat".into(),
            score: proc_score,
            details: format!("Running processes: {}", process_count),
        });

        let startup_count = startup_entry_count();
        let startup_score = if startup_count < 18 {
            92
        } else if startup_count < 35 {
            75
        } else if startup_count < 55 {
            48
        } else {
            25
        };
        if startup_score < 50 {
            warnings.push("Startup folder looks crowded and may delay boot time.".into());
            recommendations.push("Trim startup entries to improve boot responsiveness.".into());
        }
        metrics.push(HealthMetric {
            name: "Startup Entries".into(),
            score: startup_score,
            details: format!("Startup items found: {}", startup_count),
        });

        if let Some((total_services, running_services)) = service_stats() {
            let service_ratio = if total_services > 0 {
                running_services as f64 / total_services as f64
            } else {
                0.0
            };
            let service_score = if service_ratio < 0.28 {
                90
            } else if service_ratio < 0.42 {
                72
            } else if service_ratio < 0.55 {
                48
            } else {
                28
            };
            if service_score < 50 {
                warnings.push("High active service ratio may indicate system overhead.".into());
                recommendations.push("Audit non-essential services and set manual startup where safe.".into());
            }
            metrics.push(HealthMetric {
                name: "Service Load".into(),
                score: service_score,
                details: format!("Running/Total services: {}/{}", running_services, total_services),
            });
        }
    }

    let overall_score = if metrics.is_empty() {
        50
    } else {
        (metrics.iter().map(|m| m.score as u64).sum::<u64>() / metrics.len() as u64) as u8
    };

    HealthReport {
        generated_at: Local::now().to_rfc3339(),
        overall_score,
        metrics,
        warnings,
        recommendations,
    }
}
