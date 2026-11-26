//! Small ring of structured run events (device removed, etc.) for ingest.

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

fn clock_ms() -> f64 {
    STARTED
        .get_or_init(std::time::Instant::now)
        .elapsed()
        .as_secs_f64()
        * 1000.0
}

/// Record a discrete fault/exit-class event for the current run.
pub fn note(kind: &str, detail: &str) {
    let kind = kind.chars().take(64).collect::<String>();
    let detail = detail.chars().take(512).collect::<String>();
    if kind.is_empty() {
        return;
    }
    if let Ok(mut events) = EVENTS.lock() {
        if events.len() >= 64 {
            events.remove(0);
        }
        events.push(Event {
            t_ms: clock_ms(),
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
