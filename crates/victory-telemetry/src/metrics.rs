//! Lock-free hand-off from the present path to the recorder.
//!
//! The GPU meter calls [`publish`] once per one-second window, on any build,
//! whether or not telemetry or the overlay is enabled. A small ring keeps
//! every window until the recorder drains it, so a scheduling hiccup cannot
//! duplicate or drop a point in the published FPS graph.
//!
//! On Windows the game can also mirror the ring into a named shared-memory
//! mapping so a sibling telemetry agent process can drain samples after the
//! title crashes.

use std::sync::atomic::{AtomicU64, Ordering};

/// Windows retained in the ring. At one per second that is over 17 minutes;
/// the recorder drains it every second, so it only matters if that thread is
/// delayed.
pub const RING: usize = 1024;

static FPS: [AtomicU64; RING] = [const { AtomicU64::new(0) }; RING];
static MS: [AtomicU64; RING] = [const { AtomicU64::new(0) }; RING];
static DRAWS: [AtomicU64; RING] = [const { AtomicU64::new(0) }; RING];
static CPU_MS: [AtomicU64; RING] = [const { AtomicU64::new(0) }; RING];
static GPU_MS: [AtomicU64; RING] = [const { AtomicU64::new(0) }; RING];
/// Publication marker: a slot is readable when its sequence matches.
static SEQ: [AtomicU64; RING] = [const { AtomicU64::new(0) }; RING];
static NEXT: AtomicU64 = AtomicU64::new(1);

/// One second of the game's frame work.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Metrics {
    pub fps: f64,
    pub ms: f64,
    pub draws: f64,
    /// Milliseconds per frame spent preparing submissions on the CPU.
    pub cpu_ms: f64,
    /// Milliseconds per frame the GPU spent, from submission counters.
    pub gpu_ms: f64,
}

fn scaled(value: f64) -> u64 {
    if value.is_finite() && value >= 0.0 {
        (value * 100.0) as u64
    } else {
        0
    }
}

fn unscaled(value: u64) -> f64 {
    value as f64 / 100.0
}

/// Called by the GPU meter after each one-second window. Cheap and non-fatal.
pub fn publish(fps: f64, ms: f64, draws: f64, cpu_ms: f64, gpu_ms: f64) {
    let seq = NEXT.fetch_add(1, Ordering::Relaxed);
    let index = (seq % RING as u64) as usize;
    FPS[index].store(scaled(fps), Ordering::Relaxed);
    MS[index].store(scaled(ms), Ordering::Relaxed);
    DRAWS[index].store(scaled(draws), Ordering::Relaxed);
    CPU_MS[index].store(scaled(cpu_ms), Ordering::Relaxed);
    GPU_MS[index].store(scaled(gpu_ms), Ordering::Relaxed);
    // Release pairs with the reader's Acquire load, publishing all five values.
    SEQ[index].store(seq, Ordering::Release);
    #[cfg(windows)]
    shm::mirror_publish(seq, index, fps, ms, draws, cpu_ms, gpu_ms);
}

/// A reader's position in the ring. Start at zero to see everything published
/// so far; keep the same cursor across drains to get each window exactly once.
#[derive(Clone, Copy, Debug, Default)]
pub struct Cursor(u64);

fn read(index: usize) -> Metrics {
    Metrics {
        fps: unscaled(FPS[index].load(Ordering::Relaxed)),
        ms: unscaled(MS[index].load(Ordering::Relaxed)),
        draws: unscaled(DRAWS[index].load(Ordering::Relaxed)),
        cpu_ms: unscaled(CPU_MS[index].load(Ordering::Relaxed)),
        gpu_ms: unscaled(GPU_MS[index].load(Ordering::Relaxed)),
    }
}

/// Highest sequence fully published at this instant.
fn published() -> u64 {
    (0..RING)
        .map(|index| SEQ[index].load(Ordering::Acquire))
        .fold(0u64, u64::max)
}

/// Every window published since `cursor`, in order. Slots overwritten before
/// the reader got to them are skipped rather than read as stale data.
pub fn drain(cursor: &mut Cursor) -> Vec<Metrics> {
    #[cfg(windows)]
    if let Some(got) = shm::drain_shared(cursor) {
        return got;
    }
    let latest = published();
    let mut out = Vec::new();
    for seq in cursor.0.saturating_add(1)..=latest {
        if seq + RING as u64 <= latest {
            // This slot has already been reused.
            continue;
        }
        let index = (seq % RING as u64) as usize;
        if SEQ[index].load(Ordering::Acquire) == seq {
            out.push(read(index));
        }
    }
    cursor.0 = latest;
    out
}

/// The most recent published window, if the game has presented one.
pub fn snapshot() -> Option<Metrics> {
    let seq = published();
    if seq == 0 {
        return None;
    }
    Some(read((seq % RING as u64) as usize))
}

/// Create the named shared-memory ring the sidecar agent will drain.
#[cfg(windows)]
pub fn create_shared(name: &str) -> Result<(), String> {
    shm::create_publisher(name)
}

/// Open an existing shared-memory ring for the sidecar agent.
#[cfg(windows)]
pub fn open_shared(name: &str) -> Result<(), String> {
    shm::attach_reader(name)
}

#[cfg(windows)]
mod shm {
    use super::{Cursor, Metrics, RING, scaled, unscaled};
    use std::ptr;
    use std::sync::OnceLock;
    use std::sync::atomic::{AtomicU64, Ordering};
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Memory::{
        CreateFileMappingW, FILE_MAP_ALL_ACCESS, MapViewOfFile, OpenFileMappingW,
        PAGE_READWRITE, UnmapViewOfFile,
    };

    /// Layout: `next` then RING slots of (fps, ms, draws, cpu_ms, gpu_ms, seq).
    const WORDS: usize = 1 + RING * 6;
    const BYTES: usize = WORDS * 8;

    struct Mapping {
        handle: HANDLE,
        view: *mut AtomicU64,
    }

    // SAFETY: the mapping is process-wide and only accessed through atomics.
    unsafe impl Send for Mapping {}
    unsafe impl Sync for Mapping {}

    impl Drop for Mapping {
        fn drop(&mut self) {
            // SAFETY: view/handle were created as a pair and are unique to this Mapping.
            unsafe {
                if !self.view.is_null() {
                    UnmapViewOfFile(self.view.cast());
                }
                if !self.handle.is_null() {
                    CloseHandle(self.handle);
                }
            }
        }
    }

    static PUBLISHER: OnceLock<Mapping> = OnceLock::new();
    static READER: OnceLock<Mapping> = OnceLock::new();

    fn wide(name: &str) -> Vec<u16> {
        name.encode_utf16().chain([0]).collect()
    }

    fn slot(view: *mut AtomicU64, index: usize) -> *mut AtomicU64 {
        // SAFETY: caller keeps the mapping alive; index is in 0..RING.
        unsafe { view.add(1 + index * 6) }
    }

    fn store_slot(
        view: *mut AtomicU64,
        index: usize,
        seq: u64,
        fps: f64,
        ms: f64,
        draws: f64,
        cpu_ms: f64,
        gpu_ms: f64,
    ) {
        let base = slot(view, index);
        // SAFETY: six consecutive AtomicU64 words per slot.
        unsafe {
            (*base.add(0)).store(scaled(fps), Ordering::Relaxed);
            (*base.add(1)).store(scaled(ms), Ordering::Relaxed);
            (*base.add(2)).store(scaled(draws), Ordering::Relaxed);
            (*base.add(3)).store(scaled(cpu_ms), Ordering::Relaxed);
            (*base.add(4)).store(scaled(gpu_ms), Ordering::Relaxed);
            (*base.add(5)).store(seq, Ordering::Release);
        }
    }

    fn read_slot(view: *mut AtomicU64, index: usize) -> Metrics {
        let base = slot(view, index);
        // SAFETY: six consecutive AtomicU64 words per slot.
        unsafe {
            Metrics {
                fps: unscaled((*base.add(0)).load(Ordering::Relaxed)),
                ms: unscaled((*base.add(1)).load(Ordering::Relaxed)),
                draws: unscaled((*base.add(2)).load(Ordering::Relaxed)),
                cpu_ms: unscaled((*base.add(3)).load(Ordering::Relaxed)),
                gpu_ms: unscaled((*base.add(4)).load(Ordering::Relaxed)),
            }
        }
    }

    fn seq_at(view: *mut AtomicU64, index: usize) -> u64 {
        // SAFETY: seq is the sixth word of the slot.
        unsafe { (*slot(view, index).add(5)).load(Ordering::Acquire) }
    }

    fn published(view: *mut AtomicU64) -> u64 {
        (0..RING)
            .map(|index| seq_at(view, index))
            .fold(0u64, u64::max)
    }

    pub(super) fn create_publisher(name: &str) -> Result<(), String> {
        if PUBLISHER.get().is_some() {
            return Ok(());
        }
        let name_w = wide(name);
        // SAFETY: null file handle creates a pagefile-backed mapping; name is NUL-terminated.
        let handle = unsafe {
            CreateFileMappingW(
                INVALID_HANDLE_VALUE,
                ptr::null(),
                PAGE_READWRITE,
                0,
                BYTES as u32,
                name_w.as_ptr(),
            )
        };
        if handle.is_null() {
            return Err("CreateFileMappingW failed".into());
        }
        // SAFETY: handle is a valid mapping of BYTES.
        let view = unsafe { MapViewOfFile(handle, FILE_MAP_ALL_ACCESS, 0, 0, BYTES) };
        if view.is_null() {
            unsafe { CloseHandle(handle) };
            return Err("MapViewOfFile failed".into());
        }
        let view = view.cast::<AtomicU64>();
        // Zero the ring once at create so the agent does not see stale seqs.
        // SAFETY: freshly mapped private pagefile view of exactly WORDS atomics.
        unsafe {
            for i in 0..WORDS {
                (*view.add(i)).store(0, Ordering::Relaxed);
            }
            (*view).store(1, Ordering::Relaxed);
        }
        let mapping = Mapping { handle, view };
        PUBLISHER.set(mapping).map_err(|_| "publisher already set")?;
        Ok(())
    }

    pub(super) fn attach_reader(name: &str) -> Result<(), String> {
        if READER.get().is_some() {
            return Ok(());
        }
        let name_w = wide(name);
        // SAFETY: name is NUL-terminated; we only request read/write on an existing mapping.
        let handle = unsafe { OpenFileMappingW(FILE_MAP_ALL_ACCESS, 0, name_w.as_ptr()) };
        if handle.is_null() {
            return Err("OpenFileMappingW failed".into());
        }
        let view = unsafe { MapViewOfFile(handle, FILE_MAP_ALL_ACCESS, 0, 0, BYTES) };
        if view.is_null() {
            unsafe { CloseHandle(handle) };
            return Err("MapViewOfFile failed".into());
        }
        let mapping = Mapping {
            handle,
            view: view.cast(),
        };
        READER.set(mapping).map_err(|_| "reader already set")?;
        Ok(())
    }

    pub(super) fn mirror_publish(
        seq: u64,
        index: usize,
        fps: f64,
        ms: f64,
        draws: f64,
        cpu_ms: f64,
        gpu_ms: f64,
    ) {
        let Some(mapping) = PUBLISHER.get() else {
            return;
        };
        // Keep NEXT in sync for readers that inspect it; slots are authoritative.
        // SAFETY: mapping stays alive in OnceLock for process lifetime.
        unsafe {
            (*mapping.view).store(seq.saturating_add(1), Ordering::Relaxed);
        }
        store_slot(mapping.view, index, seq, fps, ms, draws, cpu_ms, gpu_ms);
    }

    pub(super) fn drain_shared(cursor: &mut Cursor) -> Option<Vec<Metrics>> {
        let mapping = READER.get()?;
        let latest = published(mapping.view);
        let mut out = Vec::new();
        for seq in cursor.0.saturating_add(1)..=latest {
            if seq + RING as u64 <= latest {
                continue;
            }
            let index = (seq % RING as u64) as usize;
            if seq_at(mapping.view, index) == seq {
                out.push(read_slot(mapping.view, index));
            }
        }
        cursor.0 = latest;
        Some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // The ring is a process-wide static, so these tests must not run together.
    static TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn drain_returns_each_window_once_in_order() {
        let _guard = TEST_LOCK.lock().unwrap();
        let start = published();
        let mut cursor = Cursor(start);
        publish(60.0, 16.67, 100.0, 3.0, 12.0);
        publish(30.0, 33.33, 200.0, 4.0, 25.0);
        let got = drain(&mut cursor);
        assert_eq!(got.len(), 2);
        assert!((got[0].fps - 60.0).abs() < 1e-9);
        assert!((got[1].fps - 30.0).abs() < 1e-9);
        assert!((got[1].gpu_ms - 25.0).abs() < 1e-9);
        assert!(drain(&mut cursor).is_empty());
    }

    #[test]
    fn scales_to_hundredths_and_drops_bad_values() {
        let _guard = TEST_LOCK.lock().unwrap();
        publish(59.875, 16.687, 1200.5, 3.25, 12.75);
        let value = snapshot().unwrap();
        assert!((value.fps - 59.87).abs() < 1e-9);
        assert!((value.ms - 16.68).abs() < 1e-9);
        assert!((value.draws - 1200.5).abs() < 1e-9);
        publish(f64::NAN, f64::INFINITY, -1.0, 0.0, 0.0);
        let value = snapshot().unwrap();
        assert_eq!(value.fps, 0.0);
        assert_eq!(value.draws, 0.0);
    }

    #[test]
    fn ring_wrap_keeps_the_newest_windows() {
        let _guard = TEST_LOCK.lock().unwrap();
        let start = published();
        for i in 0..(RING + 3) as u64 {
            publish(i as f64, 1.0, 0.0, 0.0, 0.0);
        }
        let mut cursor = Cursor(start);
        let got = drain(&mut cursor);
        assert_eq!(got.len(), RING);
        assert!((got[0].fps - 3.0).abs() < 1e-9);
        assert!((got[RING - 1].fps - (RING + 2) as f64).abs() < 1e-9);
    }

    #[cfg(windows)]
    #[test]
    fn shared_ring_round_trips_through_named_mapping() {
        let _guard = TEST_LOCK.lock().unwrap();
        let name = format!(
            "Local\\VictoryTelemMetricsTest-{}",
            std::process::id()
        );
        create_shared(&name).expect("create");
        // Publisher mirror uses the same process; also attach as reader.
        open_shared(&name).expect("open");
        let start = {
            let mut c = Cursor::default();
            let _ = drain(&mut c);
            c
        };
        let mut cursor = start;
        // Drain any prior static-only noise from other tests, then publish.
        publish(42.0, 20.0, 10.0, 1.0, 2.0);
        publish(21.0, 40.0, 20.0, 2.0, 4.0);
        let got = drain(&mut cursor);
        assert!(
            got.len() >= 2,
            "expected at least the two shared publishes, got {}",
            got.len()
        );
        let last_two = &got[got.len() - 2..];
        assert!((last_two[0].fps - 42.0).abs() < 1e-9);
        assert!((last_two[1].fps - 21.0).abs() < 1e-9);
    }
}
