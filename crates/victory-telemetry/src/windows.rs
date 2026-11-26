//! Windows-only plumbing: anonymous identity, host specs, opportunistic
//! sensors and HTTPS uploads through WinHTTP (no extra dependencies).

use std::ffi::c_void;
use std::sync::{Mutex, OnceLock};

use windows_sys::Win32::Foundation::{
    CloseHandle, FILETIME, HANDLE, HWND, RECT, WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows_sys::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC,
    DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC, GetDIBits, HDC, ReleaseDC, SRCCOPY, SelectObject,
};
use windows_sys::Win32::Networking::WinHttp::*;
use windows_sys::Win32::Storage::FileSystem::GetVolumeInformationW;
use windows_sys::Win32::System::Performance::*;
use windows_sys::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX};
use windows_sys::Win32::System::Registry::*;
use windows_sys::Win32::System::SystemInformation::{
    GetSystemInfo, GlobalMemoryStatusEx, MEMORYSTATUSEX, SYSTEM_INFO,
};
use windows_sys::Win32::System::Threading::{
    CreateEventW, GetCurrentProcess, GetCurrentProcessId, GetProcessTimes, GetSystemTimes,
    OpenEventW, OpenProcess, PROCESS_SYNCHRONIZE, SetEvent, WaitForSingleObject,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetClientRect, GetForegroundWindow, GetSystemMetrics, GetWindowThreadProcessId,
    IsWindowVisible, PW_RENDERFULLCONTENT, SM_CXSCREEN, SM_CYSCREEN,
};

#[link(name = "user32")]
unsafe extern "system" {
    fn PrintWindow(hwnd: HWND, hdcblt: HDC, nflags: u32) -> i32;
}

use crate::json::{Json, find_string};

const MB: f64 = 1024.0 * 1024.0;

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain([0]).collect()
}

fn registry_string(key: HKEY, path: &str, name: &str) -> Option<String> {
    let path = wide(path);
    let name = wide(name);
    let mut buf = [0u16; 256];
    let mut bytes = std::mem::size_of_val(&buf) as u32;
    // SAFETY: the buffer and lengths describe the same allocation.
    let rc = unsafe {
        RegGetValueW(
            key,
            path.as_ptr(),
            name.as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            buf.as_mut_ptr().cast(),
            &mut bytes,
        )
    };
    if rc != 0 {
        return None;
    }
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    let text = String::from_utf16_lossy(&buf[..len]).trim().to_owned();
    (!text.is_empty()).then_some(text)
}

/// A stable, anonymous machine key: CPU identity + Windows volume serial +
/// core count. No account name, path, serial of a peripheral, or MAC address.
pub fn machine_id() -> String {
    let mut seed = String::new();
    #[cfg(target_arch = "x86_64")]
    {
        let leaf0 = std::arch::x86_64::__cpuid(0);
        let max = leaf0.eax;
        if max >= 1 {
            let leaf1 = std::arch::x86_64::__cpuid(1);
            seed.push_str(&format!("{:08x}{:08x}", leaf1.eax, leaf1.ebx));
        }
        if max >= 0x8000_0004 {
            for leaf in [0x8000_0002, 0x8000_0003, 0x8000_0004] {
                let regs = std::arch::x86_64::__cpuid(leaf);
                seed.push_str(&format!(
                    "{:08x}{:08x}{:08x}{:08x}",
                    regs.eax, regs.ebx, regs.ecx, regs.edx
                ));
            }
        }
    }
    let root = wide("C:\\");
    let mut serial = 0u32;
    // SAFETY: only the serial out-parameter is requested; all other buffers are null.
    let ok = unsafe {
        GetVolumeInformationW(
            root.as_ptr(),
            std::ptr::null_mut(),
            0,
            &mut serial,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            0,
        )
    };
    if ok != 0 {
        seed.push_str(&format!(":{serial:08x}"));
    }
    // SAFETY: plain out-structure.
    let mut info: SYSTEM_INFO = unsafe { std::mem::zeroed() };
    // SAFETY: a plain out-parameter.
    unsafe { GetSystemInfo(&mut info) };
    seed.push_str(&format!(":{}", info.dwNumberOfProcessors));
    let digest = blake3::hash(seed.as_bytes());
    digest.to_hex()[..32].to_owned()
}

#[derive(Clone, Debug, Default)]
pub struct Specs {
    pub os_name: String,
    pub os_build: String,
    pub cpu_name: String,
    pub cpu_cores: u32,
    pub gpu_name: String,
    pub gpu_driver: String,
    pub vram_mb: u64,
    pub ram_total_mb: u64,
}

pub fn specs() -> Specs {
    let mut result = Specs {
        os_name: registry_string(
            HKEY_LOCAL_MACHINE,
            r"SOFTWARE\Microsoft\Windows NT\CurrentVersion",
            "ProductName",
        )
        .unwrap_or_default(),
        os_build: registry_string(
            HKEY_LOCAL_MACHINE,
            r"SOFTWARE\Microsoft\Windows NT\CurrentVersion",
            "DisplayVersion",
        )
        .unwrap_or_default(),
        cpu_name: registry_string(
            HKEY_LOCAL_MACHINE,
            r"HARDWARE\DESCRIPTION\System\CentralProcessor\0",
            "ProcessorNameString",
        )
        .unwrap_or_default(),
        ..Default::default()
    };
    // SAFETY: plain out-structure.
    let mut info: SYSTEM_INFO = unsafe { std::mem::zeroed() };
    // SAFETY: a plain out-parameter.
    unsafe { GetSystemInfo(&mut info) };
    result.cpu_cores = info.dwNumberOfProcessors;
    // SAFETY: plain out-structure with its length set.
    let mut memory: MEMORYSTATUSEX = unsafe { std::mem::zeroed() };
    memory.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
    // SAFETY: the length field matches the allocation.
    unsafe { GlobalMemoryStatusEx(&mut memory) };
    result.ram_total_mb = memory.ullTotalPhys / MB as u64;

    if let Ok(gpu) = gpu_description() {
        result.gpu_name = gpu.0;
        result.vram_mb = gpu.1 / (MB as u64);
    }
    result.gpu_driver = driver_version();
    result
}

fn gpu_description() -> Result<(String, u64), ()> {
    use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIFactory1};
    // SAFETY: a plain COM factory creation; the returned interface owns itself.
    let factory: IDXGIFactory1 = unsafe { CreateDXGIFactory1() }.map_err(|_| ())?;
    // SAFETY: index 0 is the default adapter; the interface is still alive.
    let adapter = unsafe { factory.EnumAdapters1(0) }.map_err(|_| ())?;
    // SAFETY: a plain description query on the live adapter.
    let desc = unsafe { adapter.GetDesc() }.map_err(|_| ())?;
    let len = desc
        .Description
        .iter()
        .position(|&c| c == 0)
        .unwrap_or(desc.Description.len());
    Ok((
        String::from_utf16_lossy(&desc.Description[..len])
            .trim()
            .to_owned(),
        desc.DedicatedVideoMemory as u64,
    ))
}

fn driver_version() -> String {
    const KEY: &str =
        r"SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}\0000";
    registry_string(HKEY_LOCAL_MACHINE, KEY, "DriverVersion").unwrap_or_default()
}

struct CpuTimes(u64, u64);

fn cpu_times() -> (u64, u64) {
    let zero = FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };
    let (mut idle, mut kernel, mut user) = (zero, zero, zero);
    // SAFETY: plain out-parameters.
    unsafe { GetSystemTimes(&mut idle, &mut kernel, &mut user) };
    let ft = |t: FILETIME| u64::from(t.dwHighDateTime) << 32 | u64::from(t.dwLowDateTime);
    (ft(idle), ft(kernel) + ft(user))
}

struct PdhCounter(*mut c_void, *mut c_void);

// SAFETY: the two handles are only used from the single recorder thread and
// are never cloned or dropped while a query is in progress.
unsafe impl Send for PdhCounter {}
unsafe impl Sync for PdhCounter {}

impl PdhCounter {
    fn new(path: &str) -> Option<Self> {
        let path = wide(path);
        let (mut query, mut counter) = (std::ptr::null_mut(), std::ptr::null_mut());
        // SAFETY: out handles and a NUL-terminated counter path.
        unsafe {
            if PdhOpenQueryW(std::ptr::null(), 0, &mut query) != 0 {
                return None;
            }
            if PdhAddEnglishCounterW(query, path.as_ptr(), 0, &mut counter) != 0 {
                PdhCloseQuery(query);
                return None;
            }
            PdhCollectQueryData(query);
        }
        Some(PdhCounter(query, counter))
    }

    fn values(&self) -> Option<Vec<f64>> {
        // SAFETY: the query and counter were created by `new` and kept open.
        unsafe {
            if PdhCollectQueryData(self.0) != 0 {
                return None;
            }
            let (mut size, mut count) = (0u32, 0u32);
            PdhGetFormattedCounterArrayW(
                self.1,
                PDH_FMT_DOUBLE,
                &mut size,
                &mut count,
                std::ptr::null_mut(),
            );
            if size == 0 {
                return None;
            }
            let mut buf = vec![0u64; (size as usize).div_ceil(8)];
            let items = buf.as_mut_ptr().cast::<PDH_FMT_COUNTERVALUE_ITEM_W>();
            if PdhGetFormattedCounterArrayW(self.1, PDH_FMT_DOUBLE, &mut size, &mut count, items)
                != 0
            {
                return None;
            }
            Some(
                (0..count as usize)
                    .map(|i| &*items.add(i))
                    .filter(|item| item.FmtValue.CStatus == 0 || item.FmtValue.CStatus == 1)
                    .map(|item| item.FmtValue.Anonymous.doubleValue)
                    .collect(),
            )
        }
    }
}

static CPU: OnceLock<Mutex<CpuTimes>> = OnceLock::new();
static GPU_3D: OnceLock<Option<PdhCounter>> = OnceLock::new();
static THERMAL: OnceLock<Option<PdhCounter>> = OnceLock::new();

/// Host counters sampled once a second alongside the game's frame window.
#[derive(Clone, Copy, Debug, Default)]
pub struct HostRates {
    pub cpu_pct: Option<f64>,
    pub gpu_pct: Option<f64>,
    pub ram_mb: Option<f64>,
    pub ram_avail_mb: Option<f64>,
    pub commit_avail_mb: Option<f64>,
    /// Page faults in the game process since the previous sample.
    pub page_faults: Option<f64>,
    /// Process user-mode CPU ms since the previous sample (all threads).
    pub proc_user_ms: Option<f64>,
    /// Process kernel-mode CPU ms since the previous sample (all threads).
    pub proc_kernel_ms: Option<f64>,
}

/// CPU/GPU load, process working set, host memory headroom, and process
/// user/kernel time. `target_pid` is the game process (the sidecar agent is a
/// different PID).
pub fn rates(target_pid: u32) -> HostRates {
    let (idle, total) = cpu_times();
    let state = CPU.get_or_init(|| Mutex::new(CpuTimes(0, 0)));
    let cpu = if let Ok(mut last) = state.lock() {
        let (di, dt) = (idle.saturating_sub(last.0), total.saturating_sub(last.1));
        *last = CpuTimes(idle, total);
        if dt > 0 {
            Some((dt - di.min(dt)) as f64 * 100.0 / dt as f64)
        } else {
            None
        }
    } else {
        None
    };

    let gpu = GPU_3D
        .get_or_init(|| PdhCounter::new(r"\GPU Engine(*engtype_3D)\Utilization Percentage"))
        .as_ref()
        .and_then(PdhCounter::values)
        .map(|values| values.into_iter().sum::<f64>().clamp(0.0, 100.0));

    let mut memory: MEMORYSTATUSEX = unsafe { std::mem::zeroed() };
    memory.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
    let (ram_avail_mb, commit_avail_mb) = if unsafe { GlobalMemoryStatusEx(&mut memory) } != 0 {
        (
            Some(memory.ullAvailPhys as f64 / MB),
            Some(memory.ullAvailPageFile as f64 / MB),
        )
    } else {
        (None, None)
    };

    static PROC: OnceLock<Mutex<(u64, u64, u64)>> = OnceLock::new();
    let mut ram = None;
    let mut page_faults = None;
    let mut proc_user_ms = None;
    let mut proc_kernel_ms = None;
    unsafe {
        // PROCESS_QUERY_LIMITED_INFORMATION is enough for GetProcessMemoryInfo /
        // GetProcessTimes.
        const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
        let handle = if target_pid == GetCurrentProcessId() {
            GetCurrentProcess()
        } else {
            OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, target_pid)
        };
        if !handle.is_null() {
            let mut pm = std::mem::zeroed::<PROCESS_MEMORY_COUNTERS_EX>();
            pm.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32;
            if GetProcessMemoryInfo(handle, std::ptr::from_mut(&mut pm).cast(), pm.cb) != 0 {
                ram = Some(pm.WorkingSetSize as f64 / MB);
                let faults = u64::from(pm.PageFaultCount);
                let state = PROC.get_or_init(|| Mutex::new((0, 0, 0)));
                if let Ok(mut last) = state.lock() {
                    page_faults = Some(faults.saturating_sub(last.0) as f64);
                    last.0 = faults;
                }
            }
            let zero = FILETIME {
                dwLowDateTime: 0,
                dwHighDateTime: 0,
            };
            let (mut created, mut exited, mut kernel, mut user) = (zero, zero, zero, zero);
            if GetProcessTimes(handle, &mut created, &mut exited, &mut kernel, &mut user) != 0 {
                let ft = |t: FILETIME| u64::from(t.dwHighDateTime) << 32 | u64::from(t.dwLowDateTime);
                // FILETIME is 100 ns units.
                let (user_100ns, kernel_100ns) = (ft(user), ft(kernel));
                let state = PROC.get_or_init(|| Mutex::new((0, 0, 0)));
                if let Ok(mut last) = state.lock() {
                    proc_user_ms = Some(user_100ns.saturating_sub(last.1) as f64 / 10_000.0);
                    proc_kernel_ms = Some(kernel_100ns.saturating_sub(last.2) as f64 / 10_000.0);
                    last.1 = user_100ns;
                    last.2 = kernel_100ns;
                }
            }
            if target_pid != GetCurrentProcessId() {
                CloseHandle(handle);
            }
        }
    };
    HostRates {
        cpu_pct: cpu,
        gpu_pct: gpu,
        ram_mb: ram,
        ram_avail_mb,
        commit_avail_mb,
        page_faults,
        proc_user_ms,
        proc_kernel_ms,
    }
}

/// CPU package temperature through Windows thermal zones, if the machine
/// exposes that counter. GPU temperature needs NVAPI, deliberately deferred.
pub fn temperatures() -> (Option<f64>, Option<String>, Option<f64>, Option<String>) {
    let counter =
        THERMAL.get_or_init(|| PdhCounter::new(r"\Thermal Zone Information(*)\Temperature"));
    let cpu = counter
        .as_ref()
        .and_then(PdhCounter::values)
        .and_then(|values| {
            values
                .into_iter()
                .filter(|v| v.is_finite() && *v > 0.0)
                .max_by(f64::total_cmp)
                .map(|raw| {
                    // The counter is usually Kelvin; some drivers report tenths.
                    if raw > 500.0 {
                        raw / 10.0 - 273.15
                    } else {
                        raw - 273.15
                    }
                })
        });
    let cpu = match cpu {
        Some(value) if value > -100.0 && value < 150.0 => (Some(value), None),
        _ => (None, Some("thermal zone sensor unavailable".to_owned())),
    };
    // GPU temp reason is recorded once on the run manifest; omit the
    // constant string from every sample row.
    (cpu.0, cpu.1, None, None)
}

/// Primary monitor size in pixels.
pub fn display() -> (u32, u32) {
    // SAFETY: these indices only query metrics, no arguments are passed.
    unsafe {
        (
            GetSystemMetrics(SM_CXSCREEN) as u32,
            GetSystemMetrics(SM_CYSCREEN) as u32,
        )
    }
}

/// HTTPS POST through WinHTTP with the system trust store. Returns the status
/// code and body; network failures are returned as text, never panics.
pub fn post(path: &str, body: &str, token: Option<&str>) -> Result<(u16, String), String> {
    request(
        "POST",
        path,
        body.as_bytes(),
        "application/json",
        token,
        Timeouts::default(),
    )
}

/// Larger JSON posts (mid-run / final ingest) need a longer send window so a
/// sample-heavy body is not aborted mid-upload when the title is still alive.
pub fn post_ingest(path: &str, body: &str, token: Option<&str>) -> Result<(u16, String), String> {
    request(
        "POST",
        path,
        body.as_bytes(),
        "application/json",
        token,
        Timeouts {
            resolve: 5_000,
            connect: 5_000,
            send: 60_000,
            receive: 60_000,
        },
    )
}

/// JPEG screenshot upload with short timeouts so a stalled collector cannot
/// block the recorder for a full sample period.
pub fn post_jpeg(path: &str, body: &[u8], token: &str) -> Result<(u16, String), String> {
    request(
        "POST",
        path,
        body,
        "image/jpeg",
        Some(token),
        Timeouts {
            resolve: 2000,
            connect: 2000,
            send: 4000,
            receive: 4000,
        },
    )
}

/// Binary/text upload for run logs (larger bodies, longer send window).
pub fn post_bytes(
    path: &str,
    body: &[u8],
    content_type: &str,
    token: &str,
) -> Result<(u16, String), String> {
    request(
        "POST",
        path,
        body,
        content_type,
        Some(token),
        Timeouts {
            resolve: 3000,
            connect: 3000,
            send: 30_000,
            receive: 30_000,
        },
    )
}

pub fn token_machine_id(token: &str) -> Option<String> {
    let (status, body) = request(
        "GET",
        "/vi/v1/stats",
        b"",
        "application/json",
        Some(token),
        Timeouts::default(),
    )
    .ok()?;
    if status != 200 {
        return None;
    }
    find_string(&body, "machine_id")
}

struct Timeouts {
    resolve: i32,
    connect: i32,
    send: i32,
    receive: i32,
}

impl Default for Timeouts {
    fn default() -> Self {
        Self {
            resolve: 3000,
            connect: 3000,
            send: 5000,
            receive: 15_000,
        }
    }
}

struct SharedHttp {
    session: *mut c_void,
    connect: *mut c_void,
}

// SAFETY: handles are only used while the Mutex guard is held.
unsafe impl Send for SharedHttp {}

fn shared_http() -> Result<std::sync::MutexGuard<'static, Option<SharedHttp>>, String> {
    static HTTP: OnceLock<Mutex<Option<SharedHttp>>> = OnceLock::new();
    let mut slot = HTTP
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if slot.is_none() {
        let agent = wide("adel-VIctory-telemetry/1.0");
        let host = wide("rxnd.de");
        // SAFETY: WinHTTP session/connect opened once and kept for reuse.
        unsafe {
            let session = WinHttpOpen(
                agent.as_ptr(),
                WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
                std::ptr::null(),
                std::ptr::null(),
                0,
            );
            if session.is_null() {
                return Err("WinHttpOpen failed".into());
            }
            let connect = WinHttpConnect(session, host.as_ptr(), 443, 0);
            if connect.is_null() {
                WinHttpCloseHandle(session);
                return Err("WinHttpConnect failed".into());
            }
            *slot = Some(SharedHttp { session, connect });
        }
    }
    Ok(slot)
}

fn request(
    method: &str,
    path: &str,
    body: &[u8],
    content_type: &str,
    token: Option<&str>,
    timeouts: Timeouts,
) -> Result<(u16, String), String> {
    let len = u32::try_from(body.len()).map_err(|_| "body too large for WinHTTP".to_owned())?;
    let verb = wide(method);
    let resource = wide(path);
    let mut headers = format!("Content-Type: {content_type}\r\n");
    if let Some(token) = token {
        headers.push_str("Authorization: Bearer ");
        headers.push_str(token);
        headers.push_str("\r\n");
    }
    let headers = wide(&headers);

    let held = shared_http()?;
    let Some(http) = held.as_ref() else {
        return Err("WinHTTP session missing".into());
    };

    // SAFETY: request handle is checked and closed before return; session/connect stay open.
    unsafe {
        let request = WinHttpOpenRequest(
            http.connect,
            verb.as_ptr(),
            resource.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            WINHTTP_FLAG_SECURE,
        );
        if request.is_null() {
            return Err("WinHttpOpenRequest failed".into());
        }
        WinHttpSetTimeouts(
            request,
            timeouts.resolve,
            timeouts.connect,
            timeouts.send,
            timeouts.receive,
        );
        let ok = WinHttpSendRequest(
            request,
            headers.as_ptr(),
            u32::MAX,
            body.as_ptr().cast::<c_void>(),
            len,
            len,
            0,
        );
        if ok == 0 {
            WinHttpCloseHandle(request);
            return Err("WinHttpSendRequest failed".into());
        }
        let ok = WinHttpReceiveResponse(request, std::ptr::null_mut());
        if ok == 0 {
            WinHttpCloseHandle(request);
            return Err("WinHttpReceiveResponse failed".into());
        }
        let mut status = 0u32;
        let mut size = std::mem::size_of::<u32>() as u32;
        WinHttpQueryHeaders(
            request,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            std::ptr::null(),
            std::ptr::from_mut(&mut status).cast(),
            &mut size,
            std::ptr::null_mut(),
        );
        let mut response = Vec::new();
        loop {
            let mut chunk = [0u8; 4096];
            let mut read = 0u32;
            if WinHttpReadData(
                request,
                chunk.as_mut_ptr().cast(),
                chunk.len() as u32,
                &mut read,
            ) == 0
                || read == 0
            {
                break;
            }
            response.extend_from_slice(&chunk[..read as usize]);
            if response.len() > 64 * 1024 {
                break;
            }
        }
        WinHttpCloseHandle(request);
        Ok((
            status as u16,
            String::from_utf8_lossy(&response).into_owned(),
        ))
    }
}

/// Named auto-reset-style stop signal shared by the game and its telemetry agent.
pub fn stop_event_name(run_id: &str) -> String {
    format!("Local\\VictoryTelemStop-{run_id}")
}

/// Shared-memory name for the FPS ring mirrored to the sidecar agent.
pub fn metrics_mapping_name(game_pid: u32) -> String {
    format!("Local\\VictoryTelemMetrics-{game_pid}")
}

/// Owns a Windows event handle used to ask the sidecar to finish cleanly.
pub struct StopEvent {
    handle: HANDLE,
}

impl StopEvent {
    /// Create the named event the sidecar waits on (game side).
    pub fn create(name: &str) -> Result<Self, String> {
        let name = wide(name);
        // SAFETY: NUL-terminated name; manual-reset so a late waiter still sees it.
        let handle = unsafe { CreateEventW(std::ptr::null(), 1, 0, name.as_ptr()) };
        if handle.is_null() {
            return Err("CreateEventW failed".into());
        }
        Ok(Self { handle })
    }

    /// Open an existing stop event (agent side).
    pub fn open(name: &str) -> Result<Self, String> {
        let name = wide(name);
        // SYNCHRONIZE is enough to wait; 0x00100000 is the Win32 value.
        const SYNCHRONIZE: u32 = 0x0010_0000;
        // SAFETY: NUL-terminated name for an event created by the game.
        let handle = unsafe { OpenEventW(SYNCHRONIZE, 0, name.as_ptr()) };
        if handle.is_null() {
            return Err("OpenEventW failed".into());
        }
        Ok(Self { handle })
    }

    pub fn signal(&self) {
        // SAFETY: handle was created/opened by this type.
        unsafe {
            let _ = SetEvent(self.handle);
        }
    }

    /// Returns true when the event is signaled.
    pub fn is_signaled(&self) -> bool {
        // SAFETY: handle was created/opened by this type.
        unsafe { WaitForSingleObject(self.handle, 0) == WAIT_OBJECT_0 }
    }
}

impl Drop for StopEvent {
    fn drop(&mut self) {
        // SAFETY: handle is exclusively owned.
        unsafe {
            CloseHandle(self.handle);
        }
    }
}

// SAFETY: HANDLE is process-scoped and only used from the owning thread/agent.
unsafe impl Send for StopEvent {}

/// Watch a process for exit (agent waits until the game dies).
pub struct ProcessWatch {
    handle: HANDLE,
}

impl ProcessWatch {
    pub fn open(pid: u32) -> Result<Self, String> {
        // SAFETY: PROCESS_SYNCHRONIZE is enough for WaitForSingleObject.
        let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
        if handle.is_null() {
            return Err(format!("OpenProcess({pid}) failed"));
        }
        Ok(Self { handle })
    }

    /// True when the process has exited (or the handle is already signaled).
    pub fn is_exited(&self) -> bool {
        // SAFETY: handle opened above.
        unsafe { WaitForSingleObject(self.handle, 0) == WAIT_OBJECT_0 }
    }

    /// Wait up to `timeout_ms` for the process to exit. Returns true if exited.
    pub fn wait_ms(&self, timeout_ms: u32) -> bool {
        // SAFETY: handle opened above.
        match unsafe { WaitForSingleObject(self.handle, timeout_ms) } {
            WAIT_OBJECT_0 => true,
            WAIT_TIMEOUT => false,
            _ => true,
        }
    }
}

impl Drop for ProcessWatch {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.handle);
        }
    }
}

unsafe impl Send for ProcessWatch {}

/// Captures the foreground game window as a heavily compressed JPEG.
/// Returns `None` when the window is not the target PID's, hidden, empty, or
/// capture fails. Pass the game PID so a sibling agent can capture it.
pub fn capture_game_jpeg(target_pid: u32) -> Option<Vec<u8>> {
    // SAFETY: window handles are queried and validated before use; GDI objects
    // created below are released on every exit path.
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_null() || IsWindowVisible(hwnd) == 0 {
            return None;
        }
        let mut owner = 0u32;
        GetWindowThreadProcessId(hwnd, &mut owner);
        if owner != target_pid {
            return None;
        }
        let mut rect = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        if GetClientRect(hwnd, &mut rect) == 0 {
            return None;
        }
        let width = rect.right.saturating_sub(rect.left);
        let height = rect.bottom.saturating_sub(rect.top);
        if width < 32 || height < 32 {
            return None;
        }
        let window_dc = GetDC(hwnd);
        if window_dc.is_null() {
            return None;
        }
        let memory_dc = CreateCompatibleDC(window_dc);
        if memory_dc.is_null() {
            ReleaseDC(hwnd, window_dc);
            return None;
        }
        let bitmap = CreateCompatibleBitmap(window_dc, width, height);
        if bitmap.is_null() {
            DeleteDC(memory_dc);
            ReleaseDC(hwnd, window_dc);
            return None;
        }
        let previous = SelectObject(memory_dc, bitmap);
        let printed = PrintWindow(hwnd, memory_dc, PW_RENDERFULLCONTENT);
        if printed == 0 {
            BitBlt(memory_dc, 0, 0, width, height, window_dc, 0, 0, SRCCOPY);
        }
        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                biSizeImage: 0,
                biXPelsPerMeter: 0,
                biYPelsPerMeter: 0,
                biClrUsed: 0,
                biClrImportant: 0,
            },
            ..Default::default()
        };
        let stride = (width as usize) * 4;
        let mut bgra = vec![0u8; stride * height as usize];
        let copied = GetDIBits(
            memory_dc,
            bitmap,
            0,
            height as u32,
            bgra.as_mut_ptr().cast(),
            &mut info,
            DIB_RGB_COLORS,
        );
        SelectObject(memory_dc, previous);
        DeleteObject(bitmap);
        DeleteDC(memory_dc);
        ReleaseDC(hwnd, window_dc);
        if copied == 0 {
            return None;
        }
        encode_downscaled_jpeg(&bgra, width as u32, height as u32)
    }
}

fn encode_downscaled_jpeg(bgra: &[u8], width: u32, height: u32) -> Option<Vec<u8>> {
    const MAX_EDGE: u32 = 480;
    let scale = (width.max(height) as f32 / MAX_EDGE as f32).max(1.0);
    let out_w = ((width as f32 / scale).round() as u32).max(1);
    let out_h = ((height as f32 / scale).round() as u32).max(1);
    let out_w_u16 = u16::try_from(out_w).ok()?;
    let out_h_u16 = u16::try_from(out_h).ok()?;
    let mut scaled = vec![0u8; (out_w * out_h * 4) as usize];
    let mut visible = 0u32;
    for y in 0..out_h {
        let src_y = ((y as f32 + 0.5) * scale) as u32;
        for x in 0..out_w {
            let src_x = ((x as f32 + 0.5) * scale) as u32;
            let i = ((src_y * width + src_x) * 4) as usize;
            let b = *bgra.get(i)?;
            let g = *bgra.get(i + 1)?;
            let r = *bgra.get(i + 2)?;
            let a = *bgra.get(i + 3).unwrap_or(&255);
            if r | g | b != 0 {
                visible += 1;
            }
            let o = ((y * out_w + x) * 4) as usize;
            scaled[o] = b;
            scaled[o + 1] = g;
            scaled[o + 2] = r;
            scaled[o + 3] = a;
        }
    }
    // A fully black capture usually means the swapchain was not readable.
    if visible * 100 < out_w * out_h {
        return None;
    }
    let mut jpeg = Vec::new();
    let encoder = jpeg_encoder::Encoder::new(&mut jpeg, 28);
    encoder
        .encode(
            &scaled,
            out_w_u16,
            out_h_u16,
            jpeg_encoder::ColorType::Bgra,
        )
        .ok()?;
    if jpeg.len() < 64 || jpeg.len() > 1024 * 1024 {
        return None;
    }
    if !(jpeg.starts_with(&[0xff, 0xd8, 0xff]) && jpeg.ends_with(&[0xff, 0xd9])) {
        return None;
    }
    Some(jpeg)
}

/// Registers this installation once and returns its per-machine token.
pub fn register(machine_id: &str, alias: &str) -> Result<String, String> {
    let body = Json::Obj(vec![
        ("schema", Json::Int(1)),
        ("machine_id", Json::string(machine_id)),
        ("alias", Json::string(alias)),
    ])
    .to_string();
    let (status, response) = post("/vi/v1/register", &body, None)?;
    if status != 200 && status != 201 {
        return Err(format!("register returned HTTP {status}: {response}"));
    }
    find_string(&response, "token").ok_or_else(|| "register response had no token".to_owned())
}
