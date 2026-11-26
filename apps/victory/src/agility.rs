//! The D3D12 Agility SDK: which D3D12 runtime the host uses.
//!
//! Windows 10's in-box runtime stops at shader model 6.5 and refuses root
//! signatures that index the descriptor heaps directly (flags 0x400, 0x800),
//! which every translated pipeline uses. D3D12 looks for these two exports on
//! the process's executable; when they name a runtime newer than the
//! system's, it loads `D3D12Core.dll` from `D3D12SDKPath` (relative to the
//! executable) instead. `build.rs` fetches the matching 1.620.2
//! `D3D12Core.dll` into that folder on MSVC builds. If the system runtime is
//! older than 620 and the app-local DLL is absent or has a different SDK
//! version, device creation fails with `D3D12_ERROR_INVALID_REDIST`
//! (`0x887e0003`).
//!
//! `build.rs` exports both symbols and installs the redistributable.
#![allow(non_upper_case_globals)]

/// The runtime's SDK version: the minor number of the
/// `Microsoft.Direct3D.D3D12` package used by this fork
/// (1.620.2). Keep the two in step.
#[unsafe(no_mangle)]
pub static D3D12SDKVersion: u32 = 620;

/// A C string pointer D3D12 reads once at device creation.
#[repr(transparent)]
pub struct SdkPath(*const std::ffi::c_char);

// SAFETY: points at a static, immutable C string.
unsafe impl Sync for SdkPath {}

/// Where `D3D12Core.dll` is, relative to the executable.
#[unsafe(no_mangle)]
pub static D3D12SDKPath: SdkPath = SdkPath(c".\\D3D12\\".as_ptr());

/// The folder `D3D12SDKPath` names, beside the running executable.
pub fn runtime_dir() -> Option<std::path::PathBuf> {
    Some(std::env::current_exe().ok()?.parent()?.join("D3D12"))
}

/// Floor size for a real Agility `D3D12Core.dll` (1.620.2 is several MB). A
/// truncated placeholder next to the exe still yields `0x887E0003`.
const MIN_CORE_BYTES: u64 = 100_000;

/// Before `--execute`: refuse to start if `D3D12\D3D12Core.dll` is missing or
/// truncated. Device creation otherwise fails later with the opaque
/// `0x887e0003` / `D3D12_ERROR_INVALID_REDIST` the title throws as a C++
/// exception — with nothing saying the redistributable was the cause.
pub fn require_for_execute() -> anyhow::Result<()> {
    let dir = runtime_dir().ok_or_else(|| {
        anyhow::anyhow!("cannot locate the executable directory to find .\\D3D12\\D3D12Core.dll")
    })?;
    let dll = dir.join("D3D12Core.dll");
    let meta = match std::fs::metadata(&dll) {
        Ok(meta) if meta.is_file() => meta,
        _ => anyhow::bail!(
            "Agility SDK runtime missing: {} not found. Rebuild so apps/victory/build.rs \
             installs Microsoft.Direct3D.D3D12 1.620.2 under .\\D3D12\\ next to victory.exe \
             (without this, device creation fails with 0x887E0003)",
            dll.display()
        ),
    };
    if meta.len() < MIN_CORE_BYTES {
        anyhow::bail!(
            "Agility SDK runtime looks truncated at {} ({} bytes). Delete it and rebuild so \
             build.rs reinstalls Microsoft.Direct3D.D3D12 1.620.2; a bad DLL fails device \
             creation with 0x887E0003",
            dll.display(),
            meta.len()
        );
    }
    Ok(())
}
