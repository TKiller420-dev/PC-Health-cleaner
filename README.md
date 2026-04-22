# PC Health Cleaner (Nexus Control Panel)

A fast desktop prototype built in Rust + egui for:
- duplicate file detection (hash-based)
- leftover cleanup targeting (archive extraction leftovers, installers, temp/backup artifacts)
- deep extensive health diagnostics (disk, memory, CPU, temp usage, process bloat)
- control-panel style multi-tab operations center

## Why this stack
- Rust: high-performance systems language with low overhead
- egui/eframe: native desktop UI with immediate rendering and responsive interaction

## Features
- Semicolon-separated target roots (example: `C:/Users/you/Desktop; C:/Users/you/Downloads`)
- UI overhaul with side navigation modules (`Dashboard`, `Cleaner`, `Health Lab`, `Automation Core`, `Tool Deck`)
- Theme engine with 3 visual modes (`Matrix`, `Amber CRT`, `Ice Core`)
- Root preset buttons (Balanced, Downloads, Desktop, Documents, Temp, Media)
- Duplicate scanner:
  - size bucketing, then BLAKE3 content hashing
  - duplicate group reporting with estimated wasted space
  - sortable duplicate prioritization (waste/copies/path)
- Leftover detector:
  - setup executables (`*setup*.exe`)
  - temp and backup leftovers (`.tmp`, `.old`, `.bak`)
  - extraction leftovers (`extract`, `unpack` folder patterns)
  - likely archive+extracted pairs by stem matching
- Health checks:
  - quick mode: disk capacity, memory pressure, CPU load, temp storage
  - deep mode: includes process bloat + startup/services audit
- One-click cleanup batches with selectable mode
- Quarantine + restore flow
- Quarantine browser with targeted restore by index
- JSON report export
- History trend panel
- Ignore rules for sensitive paths and extensions
- Scheduled run support with due checks
- Candidate search filter and risk gauge
- Cleanup safety lock phrase (`CONFIRM`) for destructive modes
- File type breakdown panel (top extensions)
- Quick actions to open app/quarantine directories and clear history
- Snapshot delta between latest runs for duplicates/leftovers/waste
- Operator notes pad included in exported report payload

## Run
1. Install Rust from https://rustup.rs
2. From project root:

```powershell
cargo run --release
```

## Safety
This prototype only scans and reports candidates. It does not auto-delete files.
Review results before removing anything.

## 15 New Tools/Upgrades In This Overhaul
1. Multi-tab control panel layout
2. Theme presets (Matrix, Amber CRT, Ice Core)
3. Root preset applier for common folders
4. Duplicate sort tools (waste/copies/path)
5. Candidate path search filter
6. Candidate risk scoring gauge
7. Cleanup preview (candidate count + reclaim estimate)
8. Safety lock phrase for destructive cleanup
9. One-click cleanup mode switcher
10. Quarantine browser with targeted restore
11. Quick folder launch tools (quarantine/appdata)
12. History clear tool
13. File type breakdown analytics panel
14. Snapshot delta view between recent runs
15. Operator notes integrated into report export

## 20 New Integrity Ideas/Upgrades/Tools/Checks
1. Integrity Lab module/tab dedicated to corruption and broken-app analysis
2. Deep integrity mode toggle for expanded checks
3. Metadata access failure check (critical)
4. Zero-byte file corruption check
5. Partial download artifact check (`.part`, `.crdownload`, etc.)
6. Tiny executable/library anomaly check
7. PE header validity check for `.exe` and `.dll`
8. ZIP archive index/readability integrity check
9. Config parse check for JSON-like config files
10. Sibling version clash check (same stem, inconsistent sizes)
11. Orphan app folder check (DLL-heavy folders missing EXE)
12. Startup entry saturation check in startup folders
13. Integrity score calculation (0-100)
14. Severity bucket counters (critical/warning/info)
15. Baseline score capture and delta display
16. Integrity issue filter (path/check/details text)
17. Critical-only filter mode
18. Integrity report JSON export
19. Integrity report CSV export
20. Auto-generated repair playbook + selected issue folder open/ignore helpers

## Local Data Location
- `%APPDATA%/NexusPcCleaner/config.json`
- `%APPDATA%/NexusPcCleaner/history.json`
- `%APPDATA%/NexusPcCleaner/quarantine/`
- `%APPDATA%/NexusPcCleaner/quarantine_index.json`
