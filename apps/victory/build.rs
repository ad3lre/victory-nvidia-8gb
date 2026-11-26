use std::fs;
use std::io::{self, Cursor, Read};
use std::path::{Path, PathBuf};

const AGILITY_VERSION: &str = "1.620.2";
const AGILITY_URL: &str = "https://api.nuget.org/v3-flatcontainer/microsoft.direct3d.d3d12/1.620.2/microsoft.direct3d.d3d12.1.620.2.nupkg";
const AGILITY_MEMBER: &str = "build/native/bin/x64/D3D12Core.dll";
const AGILITY_SHA256: &str = "70a6d47d335b4a15180544c01a35b84474522a369dc582d85fe057313a3aeaa0";

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=VICTORY_SKIP_AGILITY_FETCH");

    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        println!("cargo:rustc-link-arg-bins=/EXPORT:D3D12SDKVersion,DATA");
        println!("cargo:rustc-link-arg-bins=/EXPORT:D3D12SDKPath,DATA");
        match ensure_agility_core() {
            Ok(path) => println!(
                "cargo:warning=Agility SDK {AGILITY_VERSION} ready at {}",
                path.display()
            ),
            Err(error) => {
                println!(
                    "cargo:warning=Agility SDK {AGILITY_VERSION} D3D12Core.dll not installed: {error}"
                );
                println!(
                    "cargo:warning=Device creation may fail with 0x887E0003 until D3D12\\D3D12Core.dll sits next to victory.exe"
                );
            }
        }
    }
}

fn ensure_agility_core() -> io::Result<PathBuf> {
    let dest_dir = exe_dir()?.join("D3D12");
    let dest = dest_dir.join("D3D12Core.dll");
    if dll_ok(&dest)? {
        return Ok(dest);
    }
    if std::env::var_os("VICTORY_SKIP_AGILITY_FETCH").is_some() {
        return Err(io::Error::other(
            "VICTORY_SKIP_AGILITY_FETCH is set and D3D12Core.dll is missing",
        ));
    }
    fs::create_dir_all(&dest_dir)?;
    let bytes = download_agility_dll()?;
    verify_sha256(&bytes, AGILITY_SHA256)?;
    let tmp = dest.with_extension("dll.part");
    fs::write(&tmp, &bytes)?;
    fs::rename(&tmp, &dest)?;
    Ok(dest)
}

fn exe_dir() -> io::Result<PathBuf> {
    let out = PathBuf::from(std::env::var_os("OUT_DIR").ok_or_else(|| {
        io::Error::new(io::ErrorKind::NotFound, "OUT_DIR unset")
    })?);
    out.ancestors()
        .nth(3)
        .map(Path::to_path_buf)
        .ok_or_else(|| io::Error::other("cannot locate target profile directory from OUT_DIR"))
}

fn dll_ok(path: &Path) -> io::Result<bool> {
    if !path.is_file() {
        return Ok(false);
    }
    let bytes = fs::read(path)?;
    Ok(verify_sha256(&bytes, AGILITY_SHA256).is_ok())
}

fn download_agility_dll() -> io::Result<Vec<u8>> {
    const ATTEMPTS: u32 = 3;
    let mut last = None;
    for attempt in 1..=ATTEMPTS {
        match download_agility_dll_once() {
            Ok(bytes) => return Ok(bytes),
            Err(error) => {
                last = Some(error);
                if attempt < ATTEMPTS {
                    std::thread::sleep(std::time::Duration::from_millis(400 * u64::from(attempt)));
                }
            }
        }
    }
    Err(last.unwrap_or_else(|| io::Error::other("Agility download failed")))
}

fn download_agility_dll_once() -> io::Result<Vec<u8>> {
    let response = ureq::get(AGILITY_URL)
        .timeout(std::time::Duration::from_secs(30))
        .call()
        .map_err(|error| io::Error::other(format!("download {AGILITY_URL}: {error}")))?;
    let mut nupkg = Vec::new();
    response
        .into_reader()
        .take(64 * 1024 * 1024)
        .read_to_end(&mut nupkg)?;
    let mut archive = zip::ZipArchive::new(Cursor::new(nupkg))
        .map_err(|error| io::Error::other(format!("open nupkg: {error}")))?;
    let mut entry = archive
        .by_name(AGILITY_MEMBER)
        .map_err(|error| io::Error::other(format!("nupkg missing {AGILITY_MEMBER}: {error}")))?;
    let mut dll = Vec::with_capacity(entry.size() as usize);
    entry.read_to_end(&mut dll)?;
    Ok(dll)
}

fn verify_sha256(bytes: &[u8], expect_hex: &str) -> io::Result<()> {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(bytes);
    let actual = hex_lower(&digest);
    if actual != expect_hex {
        return Err(io::Error::other(format!(
            "D3D12Core.dll sha256 mismatch: got {actual}, expected {expect_hex}"
        )));
    }
    Ok(())
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0xf) as usize] as char);
    }
    out
}
