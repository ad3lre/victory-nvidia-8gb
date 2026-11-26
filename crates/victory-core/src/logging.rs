//! One tracing setup for every binary.
//!
//! Filter with `VICTORY_LOG` (falls back to `RUST_LOG`), e.g.
//! `VICTORY_LOG=victory_loader=trace,info`. Debug and trace output exist
//! only in builds with the `diagnostics` feature; a plain release build
//! logs info and above whatever the filter asks for.
//!
//! By default a copy of the log is also appended under the Victory data
//! directory (`…/data/logs/victory-runtime.log`) so telemetry can upload it.

use std::fs::OpenOptions;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use tracing_subscriber::EnvFilter;
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::fmt::writer::MakeWriterExt;
use tracing_subscriber::layer::SubscriberExt;
// MakeWriterExt provides `.and(...)` for tee'ing stderr + file.
use tracing_subscriber::util::SubscriberInitExt;

/// The most verbose level this build can log.
const CAP: LevelFilter = if cfg!(feature = "diagnostics") {
    LevelFilter::TRACE
} else {
    LevelFilter::INFO
};

static LOG_PATH: OnceLock<PathBuf> = OnceLock::new();

/// Path of the runtime log file, if logging was initialized with one.
pub fn runtime_log_path() -> Option<&'static PathBuf> {
    LOG_PATH.get()
}

fn default_log_path() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("VICTORY_LOG_FILE") {
        let path = PathBuf::from(path);
        if !path.as_os_str().is_empty() {
            return Some(path);
        }
    }
    let base = std::env::var_os("LOCALAPPDATA")
        .or_else(|| std::env::var_os("XDG_DATA_HOME"))
        .map(PathBuf::from)?;
    let dir = base.join("victory").join("data").join("logs");
    let _ = std::fs::create_dir_all(&dir);
    Some(dir.join("victory-runtime.log"))
}

pub fn init(default_level: &str) {
    let filter = std::env::var("VICTORY_LOG")
        .or_else(|_| std::env::var("RUST_LOG"))
        .ok()
        .and_then(|v| EnvFilter::try_new(v).ok())
        .unwrap_or_else(|| EnvFilter::new(default_level));
    let file = default_log_path().and_then(|path| {
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .ok()
            .map(|file| {
                let _ = LOG_PATH.set(path);
                Arc::new(Mutex::new(file))
            })
    });
    let _ = match file {
        Some(file) => {
            let file_writer = {
                let file = Arc::clone(&file);
                move || FileGuard(Arc::clone(&file))
            };
            tracing_subscriber::registry()
                .with(CAP)
                .with(filter)
                .with(
                    tracing_subscriber::fmt::layer()
                        .with_target(true)
                        .with_writer(std::io::stderr.and(file_writer)),
                )
                .try_init()
        }
        None => tracing_subscriber::registry()
            .with(CAP)
            .with(filter)
            .with(
                tracing_subscriber::fmt::layer()
                    .with_target(true)
                    .with_writer(std::io::stderr),
            )
            .try_init(),
    };
}

struct FileGuard(Arc<Mutex<std::fs::File>>);

impl std::io::Write for FileGuard {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let mut file = self.0.lock().map_err(|_| std::io::Error::other("log file lock"))?;
        file.write_all(buf)?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        let mut file = self.0.lock().map_err(|_| std::io::Error::other("log file lock"))?;
        file.flush()
    }
}
