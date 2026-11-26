//! Small ring of structured run events (device removed, etc.) for ingest.
//!
//! The game process records events here; a sibling telemetry agent process
//! cannot see this ring, so [`note`] also appends JSON lines to
//! `victory-events.jsonl` next to the runtime log (or under LOCALAPPDATA)
//! for the agent to merge into ingest.

use std::path::PathBuf;
use std::sync::Mutex;

use crate::json::Json;

#[derive(Clone, Debug)]
struct Event {
    t_ms: f64,
    kind: String,
    detail: String,
}

static STARTED: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
static EVENTS: Mutex<Vec<Event>> = Mutex::new(Vec::new());
static EVENT_LOG: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

fn clock_ms() -> f64 {
    STARTED
        .get_or_init(std::time::Instant::now)
        .elapsed()
        .as_secs_f64()
        * 1000.0
}

/// Fault-class events keep a long detail (DRED / debug-layer text); others stay short.
fn detail_budget(kind: &str) -> usize {
    if kind == "device_removed" || kind == "gpu_failure" || kind == "crash" {
        16_384
    } else {
        512
    }
}

/// Optional override for where `victory-events.jsonl` is written (telemetry dir).
pub fn set_event_log_path(path: PathBuf) {
    let _ = EVENT_LOG.set(path);
}

fn default_event_log_path() -> Option<PathBuf> {
    if let Some(path) = EVENT_LOG.get() {
        return Some(path.clone());
    }
    #[cfg(windows)]
    {
        let base = std::env::var_os("LOCALAPPDATA")?;
        let dir = PathBuf::from(base).join("victory").join("data").join("logs");
        let _ = std::fs::create_dir_all(&dir);
        return Some(dir.join("victory-events.jsonl"));
    }
    #[cfg(not(windows))]
    {
        None
    }
}

fn append_event_log(kind: &str, detail: &str, t_ms: f64) {
    use std::io::Write;
    let Some(path) = default_event_log_path() else {
        return;
    };
    let escaped = |s: &str| {
        s.chars()
            .flat_map(|c| match c {
                '\\' => "\\\\".chars().collect::<Vec<_>>(),
                '"' => "\\\"".chars().collect(),
                '\n' => "\\n".chars().collect(),
                '\r' => "\\r".chars().collect(),
                '\t' => "\\t".chars().collect(),
                c if c.is_control() => format!("\\u{:04x}", c as u32).chars().collect(),
                c => vec![c],
            })
            .collect::<String>()
    };
    let line = format!(
        "{{\"t_ms\":{t_ms:.3},\"kind\":\"{}\",\"detail\":\"{}\",\"pid\":{}}}\n",
        escaped(kind),
        escaped(detail),
        std::process::id()
    );
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = f.write_all(line.as_bytes());
    }
}

/// Record a discrete fault/exit-class event for the current run.
pub fn note(kind: &str, detail: &str) {
    let kind = kind.chars().take(64).collect::<String>();
    let budget = detail_budget(&kind);
    let detail = detail.chars().take(budget).collect::<String>();
    if kind.is_empty() {
        return;
    }
    let t_ms = clock_ms();
    append_event_log(&kind, &detail, t_ms);
    if let Ok(mut events) = EVENTS.lock() {
        if events.len() >= 128 {
            events.remove(0);
        }
        events.push(Event {
            t_ms,
            kind,
            detail,
        });
    }
}

/// All events recorded so far this process (not cleared — partial ingest
/// must resend the full list because the server replaces `events_json`).
pub fn all() -> Vec<Json> {
    let events = EVENTS.lock().map(|g| g.clone()).unwrap_or_default();
    events
        .into_iter()
        .map(|e| {
            Json::Obj(vec![
                ("t_ms", Json::number(e.t_ms)),
                ("kind", Json::string(e.kind)),
                ("detail", Json::string(e.detail)),
            ])
        })
        .collect()
}

/// Merge in-process events with any `victory-events.jsonl` lines found under
/// `roots` (agent process: game wrote the file, we did not share the ring).
pub fn all_with_files(roots: &[std::path::PathBuf]) -> Vec<Json> {
    let mut out = all();
    let mut seen = std::collections::BTreeSet::new();
    for e in &out {
        if let Json::Obj(fields) = e {
            let kind = fields.iter().find(|(k, _)| *k == "kind").and_then(|(_, v)| match v {
                Json::Str(s) => Some(s.as_str()),
                _ => None,
            });
            let detail = fields.iter().find(|(k, _)| *k == "detail").and_then(|(_, v)| match v {
                Json::Str(s) => Some(s.as_str()),
                _ => None,
            });
            if let (Some(k), Some(d)) = (kind, detail) {
                seen.insert(format!("{k}\n{d}"));
            }
        }
    }
    for root in roots {
        for path in event_log_candidates(root) {
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            for line in text.lines().rev().take(64) {
                let Ok(v) = parse_event_line(line) else {
                    continue;
                };
                let key = format!("{}\n{}", v.0, v.1);
                if seen.insert(key) {
                    out.push(Json::Obj(vec![
                        ("t_ms", Json::number(v.2)),
                        ("kind", Json::string(v.0)),
                        ("detail", Json::string(v.1)),
                    ]));
                }
            }
        }
    }
    out
}

fn event_log_candidates(root: &std::path::Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if root.is_file() {
        if root
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.eq_ignore_ascii_case("victory-events.jsonl"))
        {
            out.push(root.to_path_buf());
        }
        if let Some(dir) = root.parent() {
            let sib = dir.join("victory-events.jsonl");
            if sib.is_file() {
                out.push(sib);
            }
        }
        return out;
    }
    let direct = root.join("victory-events.jsonl");
    if direct.is_file() {
        out.push(direct);
    }
    let logs = root.join("logs").join("victory-events.jsonl");
    if logs.is_file() {
        out.push(logs);
    }
    out
}

fn parse_event_line(line: &str) -> Result<(String, String, f64), ()> {
    // Minimal parse: {"t_ms":N,"kind":"...","detail":"...","pid":N}
    let kind = json_string_field(line, "kind").ok_or(())?;
    let detail = json_string_field(line, "detail").ok_or(())?;
    let t_ms = json_number_field(line, "t_ms").unwrap_or(0.0);
    Ok((kind, detail, t_ms))
}

fn json_string_field(line: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\":\"");
    let start = line.find(&needle)? + needle.len();
    let bytes = line.as_bytes();
    let mut out = String::new();
    let mut i = start;
    while i < bytes.len() {
        match bytes[i] {
            b'"' => return Some(out),
            b'\\' if i + 1 < bytes.len() => {
                match bytes[i + 1] {
                    b'n' => out.push('\n'),
                    b'r' => out.push('\r'),
                    b't' => out.push('\t'),
                    b'"' => out.push('"'),
                    b'\\' => out.push('\\'),
                    b'u' if i + 5 < bytes.len() => {
                        let hex = std::str::from_utf8(&bytes[i + 2..i + 6]).ok()?;
                        let code = u32::from_str_radix(hex, 16).ok()?;
                        out.push(char::from_u32(code).unwrap_or('\u{fffd}'));
                        i += 6;
                        continue;
                    }
                    c => out.push(c as char),
                }
                i += 2;
            }
            c => {
                out.push(c as char);
                i += 1;
            }
        }
    }
    None
}

fn json_number_field(line: &str, key: &str) -> Option<f64> {
    let needle = format!("\"{key}\":");
    let start = line.find(&needle)? + needle.len();
    let rest = line[start..].trim_start();
    let end = rest
        .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-' || c == '+' || c == 'e' || c == 'E'))
        .unwrap_or(rest.len());
    rest[..end].parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_removed_keeps_long_detail() {
        let long = "x".repeat(9000);
        note("device_removed", &long);
        let all = all();
        let last = all.last().expect("event");
        let Json::Obj(fields) = last else { panic!("obj") };
        let detail = fields
            .iter()
            .find(|(k, _)| *k == "detail")
            .and_then(|(_, v)| match v {
                Json::Str(s) => Some(s.clone()),
                _ => None,
            })
            .unwrap();
        assert!(detail.len() >= 8000, "{}", detail.len());
    }

    #[test]
    fn parses_event_log_line() {
        let line = r#"{"t_ms":12.5,"kind":"device_removed","detail":"host GPU failure: nope\nline2","pid":42}"#;
        let (k, d, t) = parse_event_line(line).unwrap();
        assert_eq!(k, "device_removed");
        assert!(d.contains("nope"));
        assert!(d.contains("line2"));
        assert!((t - 12.5).abs() < 0.01);
    }
}
