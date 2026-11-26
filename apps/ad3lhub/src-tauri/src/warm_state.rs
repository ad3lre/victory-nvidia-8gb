//! Smart shader warm, same idea as AnyXboxX's launcher:
//! fingerprint victory.exe + VICTORY_* env + game archives; skip warm when current.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

pub const MARKER_FILE: &str = "warm-state.txt";
const NOT_IN_KEY: &[&str] = &["VICTORY_GAME_DIR", "VICTORY_LOG", "VICTORY_DATA_DIR"];

pub fn marker_path(cache_dir: &Path) -> PathBuf {
    cache_dir.join(MARKER_FILE)
}

fn fnv1a(text: &str) -> u64 {
    text.bytes()
        .fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
        })
}

pub fn build_id(exe: &Path) -> String {
    match std::fs::metadata(exe) {
        Ok(meta) => {
            let secs = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_secs());
            format!("{}:{secs}", meta.len())
        }
        Err(_) => "not-built".into(),
    }
}

pub fn env_key(env: &BTreeMap<String, String>) -> String {
    let mut effective: BTreeMap<&str, String> = BTreeMap::new();
    for (key, value) in env {
        if key.starts_with("VICTORY_") && !NOT_IN_KEY.contains(&key.as_str()) {
            effective.insert(key.as_str(), value.clone());
        }
    }
    effective
        .into_iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn archive_identities(archives: &[PathBuf]) -> Vec<String> {
    archives
        .iter()
        .map(|path| {
            let name = path
                .file_name()
                .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
            match std::fs::metadata(path) {
                Ok(meta) => {
                    let secs = meta
                        .modified()
                        .ok()
                        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                        .map_or(0, |d| d.as_secs());
                    format!("{name}:{}:{secs}", meta.len())
                }
                Err(_) => format!("{name}:missing"),
            }
        })
        .collect()
}

pub fn fingerprint(build: &str, env: &BTreeMap<String, String>, archives: &[String]) -> String {
    let text = format!(
        "build={build}\nenv:\n{}\narchives:\n{}",
        env_key(env),
        archives.join("\n")
    );
    format!("{:016x}", fnv1a(&text))
}

pub fn needs_warm(always: bool, marker: Option<&str>, fingerprint: &str) -> bool {
    always || marker.map(str::trim) != Some(fingerprint)
}

pub fn read_marker(path: &Path) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

pub fn write_marker(path: &Path, fingerprint: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, format!("{fingerprint}\n"))
}

/// `.rpf` archives directly in the game folder (AnyXboxX warm_archives).
pub fn warm_archives(game_dir: &Path) -> Result<Vec<PathBuf>, String> {
    let entries = std::fs::read_dir(game_dir)
        .map_err(|e| format!("cannot read {}: {e}", game_dir.display()))?;
    let mut archives: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("rpf"))
        })
        .collect();
    if archives.is_empty() {
        return Err(format!(
            "no .rpf archives in {} — cannot warm shaders",
            game_dir.display()
        ));
    }
    archives.sort_by(|a, b| a.file_name().cmp(&b.file_name()));
    Ok(archives)
}
