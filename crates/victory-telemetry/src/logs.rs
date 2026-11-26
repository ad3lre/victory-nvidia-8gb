//! Bounded discovery of Victory and title log/crash artifacts for upload.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

const MAX_FILES: usize = 12;
const MAX_FILE_BYTES: u64 = 4 * 1024 * 1024;
const MAX_TOTAL_BYTES: u64 = 16 * 1024 * 1024;
const LOOKBACK: Duration = Duration::from_secs(6 * 60 * 60);

/// Safe remote file name and local path, newest first, within size caps.
pub fn collect(roots: &[PathBuf], started: SystemTime) -> Vec<(String, PathBuf)> {
    let cutoff = started
        .checked_sub(LOOKBACK)
        .unwrap_or(SystemTime::UNIX_EPOCH);
    let mut found = Vec::new();
    for root in roots {
        push_root(&mut found, root, cutoff);
    }
    found.sort_by_key(|a| std::cmp::Reverse(a.1));
    let mut out = Vec::new();
    let mut used_names = std::collections::BTreeSet::new();
    let mut total = 0u64;
    for (path, _mtime, size) in found {
        if out.len() >= MAX_FILES || total >= MAX_TOTAL_BYTES {
            break;
        }
        if size == 0 || size > MAX_FILE_BYTES || total + size > MAX_TOTAL_BYTES {
            continue;
        }
        let Some(name) = safe_name(&path, &used_names) else {
            continue;
        };
        used_names.insert(name.clone());
        total += size;
        out.push((name, path));
    }
    out
}

fn push_root(into: &mut Vec<(PathBuf, SystemTime, u64)>, root: &Path, cutoff: SystemTime) {
    if root.is_file() {
        if interesting_file(root) {
            push_file(into, root, cutoff);
        }
        return;
    }
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() && interesting_file(&path) {
            push_file(into, &path, cutoff);
        }
    }
}

fn push_file(into: &mut Vec<(PathBuf, SystemTime, u64)>, path: &Path, cutoff: SystemTime) {
    let Ok(meta) = path.metadata() else {
        return;
    };
    let mtime = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
    if mtime < cutoff {
        return;
    }
    into.push((path.to_path_buf(), mtime, meta.len()));
}

fn interesting_file(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    let lower = name.to_ascii_lowercase();
    if lower == "crashes.log" || lower.ends_with(".crash.log") {
        return true;
    }
    if lower.ends_with(".dmp") {
        return true;
    }
    if !(lower.ends_with(".log") || lower.ends_with(".txt")) {
        return false;
    }
    lower.contains("victory")
        || lower == "victory-runtime.log"
        || lower.contains("crash")
        || lower.contains("breakpad")
        || lower.contains("gta")
        || lower.contains("rockstar")
        || lower.contains("rage")
        || lower.contains("script")
        || lower.contains("error")
        || lower.starts_with("game")
}

fn safe_name(path: &Path, used: &std::collections::BTreeSet<String>) -> Option<String> {
    let raw = path.file_name()?.to_str()?;
    let mut cleaned: String = raw
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if cleaned.is_empty() || cleaned.starts_with('.') {
        return None;
    }
    if cleaned.len() > 120 {
        cleaned.truncate(120);
    }
    if !used.contains(&cleaned) {
        return Some(cleaned);
    }
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("log");
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| format!(".{s}"))
        .unwrap_or_default();
    let digest = &blake3::hash(path.to_string_lossy().as_bytes()).to_hex()[..8];
    let mut alt = format!("{stem}-{digest}{ext}");
    alt = alt
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if alt.len() > 120 {
        alt.truncate(120);
    }
    (!used.contains(&alt)).then_some(alt)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::SystemTime;

    #[test]
    fn collects_crash_and_victory_logs_within_caps() {
        let dir = tempfile::tempdir().unwrap();
        let crashes = dir.path().join("crashes.log");
        fs::write(&crashes, b"VIctory: unhandled exception\n").unwrap();
        let title_temp = dir.path().join("temp");
        fs::create_dir(&title_temp).unwrap();
        fs::write(
            title_temp.join("victory-20261126.crash.log"),
            b"BacktraceBreakpad\n",
        )
        .unwrap();
        fs::write(title_temp.join("readme.txt"), b"skip").unwrap();
        fs::write(title_temp.join("game_error.log"), b"script fault\n").unwrap();
        let got = collect(
            &[crashes.clone(), title_temp.clone()],
            SystemTime::now(),
        );
        let names: Vec<_> = got.iter().map(|(n, _)| n.as_str()).collect();
        assert!(names.contains(&"crashes.log"));
        assert!(names.iter().any(|n| n.contains("crash")));
        assert!(names.iter().any(|n| n.contains("game_error")));
        assert!(!names.iter().any(|n| n.contains("readme")));
    }

    #[test]
    fn safe_names_are_collision_resistant() {
        let mut used = std::collections::BTreeSet::new();
        let a = PathBuf::from("alpha").join("crashes.log");
        let b = PathBuf::from("beta").join("crashes.log");
        let first = safe_name(&a, &used).unwrap();
        used.insert(first.clone());
        let second = safe_name(&b, &used).unwrap();
        assert_eq!(first, "crashes.log");
        assert_ne!(first, second);
        assert!(second.contains("crashes"));
    }
}
