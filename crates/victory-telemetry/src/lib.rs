//! Anonymous performance reports for adel VIctory.
//!
//! On by default, with `VICTORY_TELEMETRY=0` (or `[telemetry] enabled = false`,
//! or `victory run --no-telemetry`) as the opt-out. Every run writes a full
//! local bundle before attempting an HTTPS upload, so a network failure never
//! loses the data and never affects the game. Heavily compressed game-window
//! JPEGs are also captured every ten seconds for the first twenty minutes, and
//! bounded Victory/title crash and game logs are uploaded during and at the
//! end of the run. Failed uploads are retried; local copies are always kept.
//! No game packages, usernames, or network addresses are collected; log text
//! is redacted on the server.
//!
//! On Windows the recorder prefers a **sibling agent process** (a re-entry of
//! the same `victory` executable) so uploads survive GPU TDRs that kill the
//! game. Shared memory carries FPS samples; a named event asks the agent to
//! finish on a clean exit. If the agent cannot be spawned, an in-process
//! thread is used as a fallback.

// On non-Windows hosts only the portable maths and tests are compiled: there
// is no recorder to call them, so the Windows-side plumbing is intentionally
// unused there.
#![cfg_attr(not(windows), allow(dead_code, unused_imports))]

pub mod events;
pub mod json;
mod logs;
pub mod metrics;
mod summary;
#[cfg(windows)]
mod windows;

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use json::Json;
use summary::Sample;

/// Where reports are published. The only network destination in the crate.
pub const ENDPOINT: &str = "https://rxnd.de/vi/v1";

/// Env var naming the agent session JSON; set on the sibling process only.
#[cfg(windows)]
const AGENT_ENV: &str = "VICTORY_TELEMETRY_AGENT";

/// How long a clean exit waits for the agent before returning (agent may keep
/// uploading after the game process is gone).
#[cfg(windows)]
const FINISH_WAIT: Duration = Duration::from_secs(5);

/// Everything the recorder needs that the app knows and the host does not.
#[derive(Clone, Debug)]
pub struct Options {
    /// `<data>/telemetry`: outside the repository, created by the caller.
    pub dir: PathBuf,
    /// BLAKE3 hash of the title executable the run mapped.
    pub title_hash: String,
    /// Console model the title was given.
    pub console: String,
    /// Explicit opt-out from the CLI or the config file.
    pub disabled: bool,
    /// Roots scanned at the end of the run for crash/game logs (for example
    /// `crashes.log` and the title `T:` temp directory).
    pub log_roots: Vec<PathBuf>,
}

/// Owns the recorder (sidecar agent or in-process fallback). `finish` asks it
/// to flush; dropping does the same for early returns on the run path.
pub struct Run {
    #[cfg(windows)]
    inner: RunInner,
    #[cfg(not(windows))]
    _private: (),
}

#[cfg(windows)]
enum RunInner {
    Thread {
        stop: Option<Sender<()>>,
        thread: Option<JoinHandle<()>>,
    },
    Agent {
        child: Option<std::process::Child>,
        stop_event: Option<windows::StopEvent>,
    },
}

impl Run {
    pub fn finish(&mut self) {
        #[cfg(windows)]
        match &mut self.inner {
            RunInner::Thread { stop, thread } => {
                if let Some(stop) = stop.take() {
                    let _ = stop.send(());
                }
                if let Some(thread) = thread.take()
                    && thread.join().is_err()
                {
                    tracing::warn!("telemetry recorder thread panicked");
                }
            }
            RunInner::Agent { child, stop_event } => {
                if let Some(event) = stop_event.take() {
                    event.signal();
                }
                if let Some(mut child) = child.take() {
                    let deadline = Instant::now() + FINISH_WAIT;
                    loop {
                        match child.try_wait() {
                            Ok(Some(_)) => break,
                            Ok(None) if Instant::now() < deadline => {
                                std::thread::sleep(Duration::from_millis(50));
                            }
                            Ok(None) | Err(_) => {
                                // Leave the agent running; it finalizes when
                                // this process exits (or already saw the event).
                                break;
                            }
                        }
                    }
                }
            }
        }
    }
}

impl Drop for Run {
    fn drop(&mut self) {
        self.finish();
    }
}

/// Starts one recorder for this process. Returns `None` when opted out or when
/// the recorder could not start; telemetry must never fail a game launch.
/// Prefers a sibling agent process; falls back to an in-process thread.
#[cfg(windows)]
pub fn start(options: Options) -> Option<Run> {
    if options.disabled || !enabled() {
        return None;
    }
    match spawn_agent(&options) {
        Ok(run) => Some(run),
        Err(error) => {
            tracing::warn!(
                error = %error,
                "telemetry agent spawn failed; falling back to in-process recorder"
            );
            start_in_process(options)
        }
    }
}

/// Non-Windows builds have no host to measure: still compile the maths/tests,
/// but never start a recorder or contact a server.
#[cfg(not(windows))]
pub fn start(_options: Options) -> Option<Run> {
    None
}

/// Sibling-agent entry: when `VICTORY_TELEMETRY_AGENT` names a session file,
/// run the recorder until the game exits and return `true`. Otherwise `false`
/// so the normal CLI can continue.
#[cfg(windows)]
pub fn agent_main() -> bool {
    let Some(path) = std::env::var_os(AGENT_ENV) else {
        return false;
    };
    // SAFETY: single-threaded at process start; clear so we never re-enter.
    unsafe { std::env::remove_var(AGENT_ENV) };
    if let Err(error) = run_agent(Path::new(&path)) {
        tracing::warn!(error = %error, "telemetry agent stopped");
    }
    true
}

#[cfg(not(windows))]
pub fn agent_main() -> bool {
    false
}

fn enabled() -> bool {
    match std::env::var_os("VICTORY_TELEMETRY") {
        None => true,
        Some(value) => !matches!(value.to_str(), Some("0" | "false" | "False" | "FALSE")),
    }
}

#[cfg(windows)]
fn start_in_process(options: Options) -> Option<Run> {
    let game_pid = std::process::id();
    let id = run_id();
    let started = iso_utc();
    let (stop, rx) = mpsc::channel();
    let thread = std::thread::Builder::new()
        .name("victory-telemetry".into())
        .spawn(move || {
            record(
                options,
                RecordIds {
                    run_id: id,
                    started,
                    game_pid,
                },
                StopSource::Channel(rx),
            );
        })
        .ok()?;
    Some(Run {
        inner: RunInner::Thread {
            stop: Some(stop),
            thread: Some(thread),
        },
    })
}

#[cfg(windows)]
fn spawn_agent(options: &Options) -> Result<Run, String> {
    let game_pid = std::process::id();
    let id = run_id();
    let started = iso_utc();
    let shm_name = windows::metrics_mapping_name(game_pid);
    let stop_name = windows::stop_event_name(&id);
    metrics::create_shared(&shm_name)?;
    let stop_event = windows::StopEvent::create(&stop_name)?;
    let session_path = options.dir.join(format!("agent-{id}.json"));
    write_session(
        &session_path,
        &Session {
            run_id: id.clone(),
            game_pid,
            dir: options.dir.clone(),
            title_hash: options.title_hash.clone(),
            console: options.console.clone(),
            started_at: started,
            shm_name,
            stop_event: stop_name,
            log_roots: options.log_roots.clone(),
        },
    )?;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let child = std::process::Command::new(exe)
        .env(AGENT_ENV, &session_path)
        .spawn()
        .map_err(|e| e.to_string())?;
    tracing::info!(run = %id, pid = child.id(), "telemetry agent started");
    Ok(Run {
        inner: RunInner::Agent {
            child: Some(child),
            stop_event: Some(stop_event),
        },
    })
}

#[cfg(windows)]
struct Session {
    run_id: String,
    game_pid: u32,
    dir: PathBuf,
    title_hash: String,
    console: String,
    started_at: String,
    shm_name: String,
    stop_event: String,
    log_roots: Vec<PathBuf>,
}

#[cfg(windows)]
fn write_session(path: &Path, session: &Session) -> Result<(), String> {
    let roots = session
        .log_roots
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("\n");
    let body = Json::Obj(vec![
        ("run_id", Json::string(&session.run_id)),
        ("game_pid", Json::string(session.game_pid.to_string())),
        ("dir", Json::string(session.dir.to_string_lossy())),
        ("title_hash", Json::string(&session.title_hash)),
        ("console", Json::string(&session.console)),
        ("started_at", Json::string(&session.started_at)),
        ("shm_name", Json::string(&session.shm_name)),
        ("stop_event", Json::string(&session.stop_event)),
        ("log_roots", Json::string(roots)),
    ])
    .to_string();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, body).map_err(|e| e.to_string())
}

#[cfg(windows)]
fn read_session(path: &Path) -> Result<Session, String> {
    let body = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let require = |key: &str| {
        json::find_string(&body, key).ok_or_else(|| format!("session missing {key}"))
    };
    let game_pid = require("game_pid")?
        .parse::<u32>()
        .map_err(|_| "session game_pid is not a u32".to_string())?;
    let log_roots = require("log_roots")?
        .split('\n')
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .collect();
    Ok(Session {
        run_id: require("run_id")?,
        game_pid,
        dir: PathBuf::from(require("dir")?),
        title_hash: require("title_hash")?,
        console: require("console")?,
        started_at: require("started_at")?,
        shm_name: require("shm_name")?,
        stop_event: require("stop_event")?,
        log_roots,
    })
}

#[cfg(windows)]
fn run_agent(session_path: &Path) -> Result<(), String> {
    let session = read_session(session_path)?;
    // Retry briefly: the game creates the mapping/event then spawns us; a
    // fast TDR must not win the race before we take our own handles (which
    // keep the objects alive after the game dies).
    let mut last = String::new();
    for attempt in 1..=30 {
        match metrics::open_shared(&session.shm_name) {
            Ok(()) => {
                last.clear();
                break;
            }
            Err(error) => {
                last = error;
                std::thread::sleep(Duration::from_millis(50 * u64::from(attempt.min(10))));
            }
        }
    }
    if !last.is_empty() {
        return Err(last);
    }
    let stop = {
        let mut opened = None;
        let mut last = String::new();
        for attempt in 1..=30 {
            match windows::StopEvent::open(&session.stop_event) {
                Ok(event) => {
                    opened = Some(event);
                    break;
                }
                Err(error) => {
                    last = error;
                    std::thread::sleep(Duration::from_millis(50 * u64::from(attempt.min(10))));
                }
            }
        }
        opened.ok_or(last)?
    };
    let game = windows::ProcessWatch::open(session.game_pid)?;
    let options = Options {
        dir: session.dir,
        title_hash: session.title_hash,
        console: session.console,
        disabled: false,
        log_roots: session.log_roots,
    };
    record(
        options,
        RecordIds {
            run_id: session.run_id,
            started: session.started_at,
            game_pid: session.game_pid,
        },
        StopSource::Agent { game, stop },
    );
    Ok(())
}

#[cfg(windows)]
struct RecordIds {
    run_id: String,
    started: String,
    game_pid: u32,
}

#[cfg(windows)]
enum StopSource {
    Channel(Receiver<()>),
    Agent {
        game: windows::ProcessWatch,
        stop: windows::StopEvent,
    },
}

#[cfg(windows)]
impl StopSource {
    /// Wait briefly; returns true when the recorder should stop.
    fn wait_tick(&self) -> bool {
        match self {
            Self::Channel(rx) => match rx.recv_timeout(Duration::from_millis(100)) {
                Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => true,
                Err(mpsc::RecvTimeoutError::Timeout) => false,
            },
            Self::Agent { game, stop } => {
                if stop.is_signaled() || game.is_exited() {
                    return true;
                }
                let _ = game.wait_ms(100);
                stop.is_signaled() || game.is_exited()
            }
        }
    }
}

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    (y + i64::from(m <= 2), m, d)
}

fn iso_utc() -> String {
    let secs = unix_seconds();
    let (y, m, d) = civil_from_days((secs / 86_400) as i64);
    let day = secs % 86_400;
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        day / 3600,
        (day % 3600) / 60,
        day % 60
    )
}

fn run_id() -> String {
    let stamp = iso_utc().replace(['-', ':', 'T', 'Z'], "");
    let salt = format!("{:?}:{}", SystemTime::now(), std::process::id());
    let suffix = &blake3::hash(salt.as_bytes()).to_hex()[..16];
    format!("{stamp}-{suffix}")
}

fn git_sha() -> &'static str {
    env!("VICTORY_TELEMETRY_GIT_SHA")
}

/// Active `VICTORY_*` switches (values truncated; path-like values redacted).
fn launch_fingerprint() -> Json {
    let mut switches = Vec::new();
    let mut vars: Vec<(String, String)> = std::env::vars()
        .filter(|(key, _)| key.starts_with("VICTORY_"))
        .collect();
    vars.sort_by(|a, b| a.0.cmp(&b.0));
    for (key, value) in vars.into_iter().take(80) {
        if matches!(
            key.as_str(),
            "VICTORY_TELEMETRY_AGENT" | "VICTORY_TELEMETRY"
        ) {
            continue;
        }
        let value = if value.contains('\\') || value.contains('/') || value.len() > 120 {
            "<path>".to_owned()
        } else {
            value.chars().take(80).collect()
        };
        switches.push(Json::Obj(vec![
            ("name", Json::string(key)),
            ("value", Json::string(value)),
        ]));
    }
    Json::Obj(vec![("switches", Json::Arr(switches))])
}

fn notice(dir: &std::path::Path) {
    let path = dir.join("NOTICE.txt");
    if path.exists() {
        return;
    }
    let _ = std::fs::write(
        path,
        "adel VIctory sends anonymous performance reports to https://rxnd.de/vi \
         so frame-rate regressions can be compared across builds. A sibling \
         recorder process keeps uploading if the game crashes. Reports contain \
         PC specs, driver versions, screen size, temperatures where available, \
         the exact FPS timeline with Victory frame-budget counters, host memory \
         pressure, heavily compressed game-window screenshots every ten seconds for \
         the first twenty minutes, the Victory runtime log, and bounded Victory/title \
         crash and game log files from the run. They contain no game packages, usernames \
         or network addresses; embedded paths in logs are redacted on the server. \
         Your machine has an anonymous random key so runs from different PCs can \
         be told apart.\n\n\
         Opt out with VICTORY_TELEMETRY=0, [telemetry] enabled = false in \
         victory.toml, or victory run --no-telemetry. Local copies of every report \
         stay in this directory.\n",
    );
}

/// First twenty minutes: one JPEG every ten seconds (`001.jpg` … `120.jpg`).
const SCREENSHOT_WINDOW: Duration = Duration::from_secs(20 * 60);
const SCREENSHOT_INTERVAL_SECS: u64 = 10;
const UPLOAD_ATTEMPTS: u32 = 3;

#[cfg(windows)]
fn load_token(dir: &std::path::Path, machine_id: &str) -> Option<String> {
    let path = dir.join("token");
    if let Ok(existing) = std::fs::read_to_string(&path) {
        let token = existing.trim();
        if !token.is_empty() {
            return Some(token.to_owned());
        }
    }
    let alias: String = std::env::var("VICTORY_TELEMETRY_ALIAS")
        .unwrap_or_default()
        .chars()
        .take(64)
        .collect();
    for attempt in 1..=UPLOAD_ATTEMPTS {
        match windows::register(machine_id, &alias) {
            Ok(token) => {
                let _ = std::fs::write(&path, &token);
                return Some(token);
            }
            Err(error) => {
                tracing::warn!(
                    error = %error,
                    attempt,
                    "telemetry registration failed; will retry or keep local reports only"
                );
                if attempt < UPLOAD_ATTEMPTS {
                    std::thread::sleep(Duration::from_millis(400 * u64::from(attempt)));
                }
            }
        }
    }
    None
}

#[cfg(windows)]
fn ensure_token(dir: &std::path::Path, machine_id: &str, token: &mut Option<String>) {
    if token.is_none() {
        *token = load_token(dir, machine_id);
    }
}

#[cfg(windows)]
fn http_ok(status: u16) -> bool {
    (200..300).contains(&status)
}

#[cfg(windows)]
fn fatal_upload_status(status: u16) -> bool {
    matches!(status, 400 | 401 | 403 | 404 | 413)
}

#[cfg(windows)]
fn post_ok(path: &str, body: &str, token: Option<&str>, what: &str) -> bool {
    post_result(|| windows::post(path, body, token), what)
}

#[cfg(windows)]
fn ingest_ok(path: &str, body: &str, token: Option<&str>, what: &str) -> bool {
    post_result(|| windows::post_ingest(path, body, token), what)
}

#[cfg(windows)]
fn post_result(
    mut send: impl FnMut() -> Result<(u16, String), String>,
    what: &str,
) -> bool {
    for attempt in 1..=UPLOAD_ATTEMPTS {
        match send() {
            Ok((status, _)) if http_ok(status) => return true,
            Ok((status, response)) => {
                tracing::warn!(
                    status,
                    response = %response,
                    attempt,
                    what,
                    "telemetry upload rejected; local copy kept"
                );
                if fatal_upload_status(status) {
                    return false;
                }
            }
            Err(error) => {
                tracing::warn!(
                    error = %error,
                    attempt,
                    what,
                    "telemetry upload failed; local copy kept"
                );
            }
        }
        if attempt < UPLOAD_ATTEMPTS {
            std::thread::sleep(Duration::from_millis(350 * u64::from(attempt)));
        }
    }
    false
}

#[cfg(windows)]
fn make_ingest_bundle(
    id: &str,
    machine_id: &str,
    started: &str,
    fork: &Json,
    manifest: &Json,
    samples: &[Sample],
    /// When set, only these samples are sent and the server appends them.
    partial: bool,
) -> Json {
    let summary = summary::compute(samples);
    let mut fields = vec![
        ("schema", Json::Int(1)),
        ("run_id", Json::string(id)),
        ("machine_id", Json::string(machine_id)),
        ("started_at", Json::string(started)),
        ("fork", fork.clone()),
        ("manifest", manifest.clone()),
        (
            "samples",
            Json::Arr(samples.iter().map(Sample::to_json).collect()),
        ),
        ("events", Json::Arr(events::all())),
        ("summary", summary.to_json()),
    ];
    if partial {
        fields.push(("mode", Json::string("partial")));
    }
    Json::Obj(fields)
}

#[cfg(windows)]
fn post_jpeg_ok(path: &str, jpeg: &[u8], token: &str, what: &str) -> bool {
    post_jpeg_ok_until(path, jpeg, token, what, None)
}

#[cfg(windows)]
fn post_jpeg_ok_until(
    path: &str,
    jpeg: &[u8],
    token: &str,
    what: &str,
    deadline: Option<Instant>,
) -> bool {
    for attempt in 1..=UPLOAD_ATTEMPTS {
        if deadline.is_some_and(|d| Instant::now() >= d) {
            tracing::warn!(what, "telemetry screenshot upload skipped; flush deadline passed");
            return false;
        }
        match windows::post_jpeg(path, jpeg, token) {
            Ok((status, _)) if http_ok(status) => return true,
            Ok((status, response)) => {
                tracing::warn!(
                    status,
                    response = %response,
                    attempt,
                    what,
                    "telemetry screenshot rejected; local copy kept"
                );
                if fatal_upload_status(status) {
                    return false;
                }
            }
            Err(error) => {
                tracing::warn!(
                    error = %error,
                    attempt,
                    what,
                    "telemetry screenshot upload failed; local copy kept"
                );
            }
        }
        if attempt < UPLOAD_ATTEMPTS {
            std::thread::sleep(Duration::from_millis(350 * u64::from(attempt)));
        }
    }
    false
}

#[cfg(windows)]
fn post_bytes_ok(path: &str, body: &[u8], content_type: &str, token: &str, what: &str) -> bool {
    for attempt in 1..=UPLOAD_ATTEMPTS {
        match windows::post_bytes(path, body, content_type, token) {
            Ok((status, _)) if http_ok(status) => return true,
            Ok((status, response)) => {
                tracing::warn!(
                    status,
                    response = %response,
                    attempt,
                    what,
                    "telemetry log rejected; local copy kept"
                );
                if fatal_upload_status(status) {
                    return false;
                }
            }
            Err(error) => {
                tracing::warn!(
                    error = %error,
                    attempt,
                    what,
                    "telemetry log upload failed; local copy kept"
                );
            }
        }
        if attempt < UPLOAD_ATTEMPTS {
            std::thread::sleep(Duration::from_millis(350 * u64::from(attempt)));
        }
    }
    false
}

/// Shot index 1..=120 for elapsed times in `[0, 1200)` seconds (one every 10 s).
fn screenshot_slot(elapsed: Duration) -> Option<u8> {
    if elapsed >= SCREENSHOT_WINDOW {
        return None;
    }
    u8::try_from(elapsed.as_secs() / SCREENSHOT_INTERVAL_SECS + 1).ok()
}

fn shot_name(slot: u8) -> String {
    format!("{slot:03}.jpg")
}

#[cfg(windows)]
fn maybe_capture_screenshot(
    dir: &std::path::Path,
    run_id: &str,
    game_pid: u32,
    token: Option<&str>,
    elapsed: Duration,
    next_slot: &mut u8,
    pending: &mut Vec<(String, PathBuf)>,
) {
    let Some(slot) = screenshot_slot(elapsed) else {
        return;
    };
    if slot < *next_slot {
        return;
    }
    let Some(jpeg) = windows::capture_game_jpeg(game_pid) else {
        // Keep the slot open so alt-tab / black swapchain can retry.
        return;
    };
    let name = shot_name(slot);
    let shots = dir.join(format!("screenshots-{run_id}"));
    let _ = std::fs::create_dir_all(&shots);
    let path = shots.join(&name);
    if std::fs::write(&path, &jpeg).is_err() {
        return;
    }
    *next_slot = slot.saturating_add(1);
    let Some(token) = token else {
        pending.push((name, path));
        return;
    };
    let remote = format!("/vi/v1/runs/{run_id}/screenshots/{name}");
    if !post_jpeg_ok(&remote, &jpeg, token, &name) {
        pending.push((name, path));
    }
}

#[cfg(windows)]
fn flush_pending_screenshots(
    run_id: &str,
    token: &str,
    pending: &mut Vec<(String, PathBuf)>,
    deadline: Instant,
) {
    let mut i = 0;
    while i < pending.len() {
        if Instant::now() >= deadline {
            tracing::warn!(
                left = pending.len() - i,
                "telemetry screenshot flush budget exhausted; remaining stay local"
            );
            break;
        }
        let (name, path) = &pending[i];
        let Ok(jpeg) = std::fs::read(path) else {
            pending.remove(i);
            continue;
        };
        let remote = format!("/vi/v1/runs/{run_id}/screenshots/{name}");
        if post_jpeg_ok_until(&remote, &jpeg, token, name, Some(deadline)) {
            pending.remove(i);
        } else {
            i += 1;
            std::thread::sleep(Duration::from_millis(200));
        }
    }
}

#[cfg(windows)]
fn record(options: Options, ids: RecordIds, stop: StopSource) {
    let dir = options.dir;
    notice(&dir);
    // Older installs derived this ID from CPUID, which can vary with the
    // CPU running the thread. Recover the ID bound to an existing token.
    let identity_path = dir.join("machine_id");
    let saved = std::fs::read_to_string(&identity_path)
        .ok()
        .filter(|id| (16..=128).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_hexdigit()));
    let machine_id = saved.unwrap_or_else(|| {
        let token = std::fs::read_to_string(dir.join("token")).ok();
        let recovered = token
            .as_deref()
            .and_then(|t| windows::token_machine_id(t.trim()));
        let id = recovered.clone().unwrap_or_else(windows::machine_id);
        if token.is_none() || recovered.is_some() {
            let _ = std::fs::write(&identity_path, &id);
        }
        id
    });
    let mut token = load_token(&dir, &machine_id);
    let id = ids.run_id;
    let started = ids.started;
    let game_pid = ids.game_pid;
    let specs = windows::specs();
    let (width, height) = windows::display();
    let fork = Json::Obj(vec![
        ("name", Json::string("adel")),
        ("git_sha", Json::string(git_sha())),
        ("version", Json::string(env!("CARGO_PKG_VERSION"))),
    ]);
    let manifest = Json::Obj(vec![
        (
            "os",
            Json::Obj(vec![
                ("name", Json::string(specs.os_name)),
                ("version", Json::string(specs.os_build)),
            ]),
        ),
        (
            "cpu",
            Json::Obj(vec![
                ("name", Json::string(specs.cpu_name)),
                ("cores", Json::Int(u64::from(specs.cpu_cores))),
            ]),
        ),
        (
            "gpu",
            Json::Obj(vec![
                ("name", Json::string(specs.gpu_name)),
                ("driver", Json::string(specs.gpu_driver)),
                ("vram_mb", Json::Int(specs.vram_mb)),
                (
                    "temp_note",
                    Json::string("NVAPI temperature reader not enabled in v1"),
                ),
            ]),
        ),
        (
            "display",
            Json::Obj(vec![
                ("width", Json::Int(u64::from(width))),
                ("height", Json::Int(u64::from(height))),
            ]),
        ),
        ("ram_total_mb", Json::Int(specs.ram_total_mb)),
        (
            "title",
            Json::Obj(vec![
                ("hash", Json::string(options.title_hash)),
                ("console", Json::string(options.console)),
            ]),
        ),
        ("launch", launch_fingerprint()),
    ]);

    let local_manifest = Json::Obj(vec![
        ("schema", Json::Int(1)),
        ("run_id", Json::string(&id)),
        ("machine_id", Json::string(&machine_id)),
        ("started_at", Json::string(&started)),
        ("fork", fork.clone()),
        ("manifest", manifest.clone()),
    ]);
    let _ = std::fs::write(
        dir.join(format!("manifest-{id}.json")),
        local_manifest.to_string(),
    );

    // Manifest rides along on ping so OS/CPU/GPU land even if the title
    // crashes before a final ingest (common with TDR / device-removed).
    let ping = Json::Obj(vec![
        ("schema", Json::Int(1)),
        ("run_id", Json::string(&id)),
        ("machine_id", Json::string(&machine_id)),
        ("fork", Json::string("adel")),
        ("git_sha", Json::string(git_sha())),
        ("version", Json::string(env!("CARGO_PKG_VERSION"))),
        ("started_at", Json::string(&started)),
        ("manifest", manifest.clone()),
    ]);
    let mut pinged = false;
    let mut ingested = false;
    if let Some(tok) = token.as_deref() {
        pinged = post_ok("/vi/v1/ping", &ping.to_string(), Some(tok), "ping");
        // Empty-sample ingest publishes hardware immediately and creates the
        // completed-run row the dashboard charts from.
        let early = make_ingest_bundle(&id, &machine_id, &started, &fork, &manifest, &[], false);
        ingested = ingest_ok("/vi/v1/ingest", &early.to_string(), Some(tok), "ingest-early");
    }

    let samples_path = dir.join(format!("samples-{id}.jsonl"));
    let file = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&samples_path);
    let Ok(file) = file else {
        tracing::warn!("telemetry samples file could not be opened; no local report");
        return;
    };
    let mut writer = std::io::BufWriter::new(file);
    let mut samples: Vec<Sample> = Vec::new();
    let mut cursor = metrics::Cursor::default();
    let began = Instant::now();
    let mut next_shot = 1u8;
    let mut pending_shots: Vec<(String, PathBuf)> = Vec::new();
    let mut uploaded_logs = std::collections::BTreeSet::new();
    maybe_capture_screenshot(
        &dir,
        &id,
        game_pid,
        token.as_deref(),
        Duration::ZERO,
        &mut next_shot,
        &mut pending_shots,
    );
    let mut last_log_scan = Instant::now() - Duration::from_secs(30);
    let mut last_token_retry = Instant::now();
    let mut last_ingest = Instant::now();
    let mut last_ingest_len = 0usize;
    loop {
        if stop.wait_tick() {
            break;
        }
        if token.is_none() && last_token_retry.elapsed() >= Duration::from_secs(45) {
            ensure_token(&dir, &machine_id, &mut token);
            last_token_retry = Instant::now();
            if !pinged
                && let Some(tok) = token.as_deref()
            {
                pinged = post_ok("/vi/v1/ping", &ping.to_string(), Some(tok), "ping");
            }
            if !ingested
                && let Some(tok) = token.as_deref()
            {
                let early = make_ingest_bundle(&id, &machine_id, &started, &fork, &manifest, &[], false);
                ingested = ingest_ok("/vi/v1/ingest", &early.to_string(), Some(tok), "ingest-early");
            }
        }
        maybe_capture_screenshot(
            &dir,
            &id,
            game_pid,
            token.as_deref(),
            began.elapsed(),
            &mut next_shot,
            &mut pending_shots,
        );
        if last_log_scan.elapsed() >= Duration::from_secs(30) {
            if let Some(tok) = token.as_deref() {
                if !pending_shots.is_empty() {
                    flush_pending_screenshots(
                        &id,
                        tok,
                        &mut pending_shots,
                        Instant::now() + Duration::from_secs(4),
                    );
                }
                upload_logs(
                    &dir,
                    &id,
                    tok,
                    &options.log_roots,
                    began,
                    &mut uploaded_logs,
                );
            }
            last_log_scan = Instant::now();
        }
        // Mid-run ingest so FPS/system survive GPU TDRs that kill the process
        // before the final flush (every ~60s once new samples exist).
        if last_ingest.elapsed() >= Duration::from_secs(60)
            && samples.len() > last_ingest_len
            && let Some(tok) = token.as_deref()
        {
            let fresh = &samples[last_ingest_len..];
            let bundle = make_ingest_bundle(
                &id,
                &machine_id,
                &started,
                &fork,
                &manifest,
                fresh,
                true,
            );
            if ingest_ok("/vi/v1/ingest", &bundle.to_string(), Some(tok), "ingest-partial") {
                ingested = true;
                last_ingest_len = samples.len();
            }
            last_ingest = Instant::now();
        }
        let windows = metrics::drain(&mut cursor);
        let host = windows::rates(game_pid);
        let (cpu_temp_c, cpu_temp_reason, gpu_temp_c, gpu_temp_reason) = windows::temperatures();
        for metrics in windows {
            let sample = Sample {
                t_ms: began.elapsed().as_secs_f64() * 1000.0,
                fps: metrics.fps,
                ms: metrics.ms,
                draws: metrics.draws,
                cpu_ms: metrics.cpu_ms,
                gpu_ms: metrics.gpu_ms,
                after_present_ms: metrics.after_present_ms,
                until_submit_ms: metrics.until_submit_ms,
                watch_ms: metrics.watch_ms,
                guard_ms: metrics.guard_ms,
                guard_faults: metrics.guard_faults,
                cpu_pct: host.cpu_pct,
                gpu_pct: host.gpu_pct,
                ram_mb: host.ram_mb,
                ram_avail_mb: host.ram_avail_mb,
                commit_avail_mb: host.commit_avail_mb,
                page_faults: host.page_faults,
                proc_user_ms: host.proc_user_ms,
                proc_kernel_ms: host.proc_kernel_ms,
                cpu_temp_c,
                cpu_temp_reason: cpu_temp_reason.clone(),
                gpu_temp_c,
                gpu_temp_reason: gpu_temp_reason.clone(),
            };
            let _ = std::io::Write::write_all(
                &mut writer,
                format!("{}\n", sample.to_json()).as_bytes(),
            );
            samples.push(sample);
        }
        if !samples.is_empty() && samples.len().is_multiple_of(5) {
            let _ = std::io::Write::flush(&mut writer);
        }
    }
    let _ = std::io::Write::flush(&mut writer);

    let summary = summary::compute(&samples);
    let _ = std::fs::write(
        dir.join(format!("summary-{id}.json")),
        summary.to_json().to_string(),
    );
    let bundle = make_ingest_bundle(&id, &machine_id, &started, &fork, &manifest, &samples, false);
    ensure_token(&dir, &machine_id, &mut token);
    let mut upload_notes = Vec::new();
    if let Some(tok) = token.as_deref() {
        if !pinged {
            pinged = post_ok("/vi/v1/ping", &ping.to_string(), Some(tok), "ping");
        }
        if !pinged {
            upload_notes.push("ping failed after retries".into());
        }
        if !ingest_ok("/vi/v1/ingest", &bundle.to_string(), Some(tok), "ingest") {
            upload_notes.push("ingest failed after retries".into());
        } else {
            ingested = true;
        }
        flush_pending_screenshots(
            &id,
            tok,
            &mut pending_shots,
            Instant::now() + Duration::from_secs(90),
        );
        if !pending_shots.is_empty() {
            upload_notes.push(format!(
                "{} screenshot(s) left local only",
                pending_shots.len()
            ));
        }
        upload_logs(
            &dir,
            &id,
            tok,
            &options.log_roots,
            began,
            &mut uploaded_logs,
        );
    } else {
        upload_notes.push("registration failed; reports stayed local".into());
        if !pending_shots.is_empty() {
            upload_notes.push(format!(
                "{} screenshot(s) never uploaded",
                pending_shots.len()
            ));
        }
    }
    if !upload_notes.is_empty() {
        let _ = std::fs::write(
            dir.join(format!("upload-status-{id}.txt")),
            upload_notes.join("\n") + "\n",
        );
        tracing::warn!(
            run = %id,
            notes = %upload_notes.join("; "),
            "telemetry upload incomplete; see upload-status file"
        );
    }
    tracing::info!(run = %id, samples = samples.len(), "telemetry report written to {}", dir.display());
}

#[cfg(windows)]
fn upload_logs(
    dir: &std::path::Path,
    run_id: &str,
    token: &str,
    roots: &[PathBuf],
    began: Instant,
    already: &mut std::collections::BTreeSet<String>,
) {
    let started = SystemTime::now()
        .checked_sub(began.elapsed())
        .unwrap_or(SystemTime::UNIX_EPOCH);
    let files = logs::collect(roots, started);
    if files.is_empty() {
        return;
    }
    let local = dir.join(format!("logs-{run_id}"));
    let _ = std::fs::create_dir_all(&local);
    for (name, path) in files {
        let key = format!("{name}:{}", path.metadata().map(|m| m.len()).unwrap_or(0));
        if already.contains(&key) {
            continue;
        }
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let _ = std::fs::write(local.join(&name), &bytes);
        let remote = format!("/vi/v1/runs/{run_id}/logs/{name}");
        let content_type = if name.to_ascii_lowercase().ends_with(".dmp") {
            "application/octet-stream"
        } else {
            "text/plain; charset=utf-8"
        };
        if post_bytes_ok(&remote, &bytes, content_type, token, &name) {
            already.insert(key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_id_is_server_compatible() {
        let id = run_id();
        assert!(id.len() <= 64);
        assert!(
            id.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        );
    }

    #[test]
    fn utc_stamp_is_well_formed() {
        let stamp = iso_utc();
        assert_eq!(stamp.len(), 20);
        assert!(stamp.ends_with('Z'));
        assert_eq!(stamp.as_bytes()[4], b'-');
    }

    #[test]
    fn civil_dates_cross_year_boundary() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(18_262), (2020, 1, 1));
    }

    #[test]
    fn screenshot_slots_cover_first_twenty_minutes() {
        assert_eq!(screenshot_slot(Duration::from_secs(0)), Some(1));
        assert_eq!(screenshot_slot(Duration::from_secs(9)), Some(1));
        assert_eq!(screenshot_slot(Duration::from_secs(10)), Some(2));
        assert_eq!(screenshot_slot(Duration::from_secs(599)), Some(60));
        assert_eq!(screenshot_slot(Duration::from_secs(600)), Some(61));
        assert_eq!(screenshot_slot(Duration::from_secs(1199)), Some(120));
        assert_eq!(screenshot_slot(Duration::from_secs(1200)), None);
        assert_eq!(shot_name(1), "001.jpg");
        assert_eq!(shot_name(120), "120.jpg");
    }

    #[test]
    fn session_json_round_trips_string_fields() {
        let roots = ["/tmp/a", "/tmp/b"].join("\n");
        let body = Json::Obj(vec![
            ("run_id", Json::string("20261126120000-abcd")),
            ("game_pid", Json::string("4242")),
            ("dir", Json::string(r"C:\Users\me\telemetry")),
            ("title_hash", Json::string("deadbeef")),
            ("console", Json::string("Xbox Series X")),
            ("started_at", Json::string("2026-11-26T12:00:00Z")),
            ("shm_name", Json::string(r"Local\VictoryTelemMetrics-4242")),
            ("stop_event", Json::string(r"Local\VictoryTelemStop-20261126120000-abcd")),
            ("log_roots", Json::string(roots)),
        ])
        .to_string();
        assert_eq!(
            json::find_string(&body, "game_pid").as_deref(),
            Some("4242")
        );
        assert_eq!(
            json::find_string(&body, "dir").as_deref(),
            Some(r"C:\Users\me\telemetry")
        );
        let roots = json::find_string(&body, "log_roots").unwrap();
        assert_eq!(roots.lines().count(), 2);
    }
}
