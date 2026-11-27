use std::fs;
use std::io::{self, Cursor, Read};
use std::path::{Path, PathBuf};

const AGILITY_VERSION: &str = "1.620.2";
const AGILITY_URL: &str =
    "https://api.nuget.org/v3-flatcontainer/microsoft.direct3d.d3d12/1.620.2/microsoft.direct3d.d3d12.1.620.2.nupkg";

struct AgilityDll {
    /// Path inside the nupkg.
    member: &'static str,
    /// File name under `.\D3D12\` next to the exe (and under the vendored folder).
    file: &'static str,
    sha256: &'static str,
}

const AGILITY_DLLS: &[AgilityDll] = &[
    AgilityDll {
        member: "build/native/bin/x64/D3D12Core.dll",
        file: "D3D12Core.dll",
        sha256: "70a6d47d335b4a15180544c01a35b84474522a369dc582d85fe057313a3aeaa0",
    },
    // Matching SDK layers: without this, VICTORY_D3D12_DEBUG fails with
    // 0x887A002D (SDK component missing) when Agility Core is loaded.
    AgilityDll {
        member: "build/native/bin/x64/d3d12SDKLayers.dll",
        file: "d3d12SDKLayers.dll",
        sha256: "3140822f550003d322b0a0681e0c40af7364c561296a1d7eeb96b3258c9cd32d",
    },
];

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=D3D12/D3D12Core.dll");
    println!("cargo:rerun-if-changed=D3D12/d3d12SDKLayers.dll");
    println!("cargo:rerun-if-env-changed=VICTORY_SKIP_AGILITY_FETCH");

    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        println!("cargo:rustc-link-arg-bins=/EXPORT:D3D12SDKVersion,DATA");
        println!("cargo:rustc-link-arg-bins=/EXPORT:D3D12SDKPath,DATA");
        match ensure_agility() {
            Ok(dir) => println!(
                "cargo:warning=Agility SDK {AGILITY_VERSION} ready at {}",
                dir.display()
            ),
            Err(error) => {
                println!(
                    "cargo:warning=Agility SDK {AGILITY_VERSION} not fully installed: {error}"
                );
                println!(
                    "cargo:warning=Device creation may fail with 0x887E0003 until D3D12\\D3D12Core.dll sits next to victory.exe; debug layer needs d3d12SDKLayers.dll too"
                );
            }
        }
    }
}

fn ensure_agility() -> io::Result<PathBuf> {
    let dest_dir = exe_dir()?.join("D3D12");
    let vendor_dir = vendor_dir()?;
    let missing: Vec<&AgilityDll> = AGILITY_DLLS
        .iter()
        .filter(|dll| !dll_ok(&dest_dir.join(dll.file), dll.sha256).unwrap_or(false))
        .collect();
    if missing.is_empty() {
        return Ok(dest_dir.join(AGILITY_DLLS[0].file));
    }

    fs::create_dir_all(&dest_dir)?;

    // Prefer the copies committed under apps/victory/D3D12/ (offline builds).
    let mut still_missing = Vec::new();
    for dll in missing {
        let vendor = vendor_dir.join(dll.file);
        if dll_ok(&vendor, dll.sha256).unwrap_or(false) {
            let dest = dest_dir.join(dll.file);
            let tmp = dest.with_extension("dll.part");
            fs::copy(&vendor, &tmp)?;
            fs::rename(&tmp, &dest)?;
            continue;
        }
        still_missing.push(dll);
    }
    if still_missing.is_empty() {
        return Ok(dest_dir.join(AGILITY_DLLS[0].file));
    }

    if std::env::var_os("VICTORY_SKIP_AGILITY_FETCH").is_some() {
        return Err(io::Error::other(
            "VICTORY_SKIP_AGILITY_FETCH is set and Agility DLLs are missing from target and apps/victory/D3D12",
        ));
    }

    let nupkg = download_agility_nupkg()?;
    let mut archive = zip::ZipArchive::new(Cursor::new(nupkg))
        .map_err(|error| io::Error::other(format!("open nupkg: {error}")))?;
    for dll in still_missing {
        let mut entry = archive.by_name(dll.member).map_err(|error| {
            io::Error::other(format!("nupkg missing {}: {error}", dll.member))
        })?;
        let mut bytes = Vec::with_capacity(entry.size() as usize);
        entry.read_to_end(&mut bytes)?;
        verify_sha256(&bytes, dll.sha256, dll.file)?;
        let dest = dest_dir.join(dll.file);
        let tmp = dest.with_extension("dll.part");
        fs::write(&tmp, &bytes)?;
        fs::rename(&tmp, &dest)?;
    }
    Ok(dest_dir.join(AGILITY_DLLS[0].file))
}

fn vendor_dir() -> io::Result<PathBuf> {
    let manifest = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").ok_or_else(|| {
        io::Error::new(io::ErrorKind::NotFound, "CARGO_MANIFEST_DIR unset")
    })?);
    Ok(manifest.join("D3D12"))
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

fn dll_ok(path: &Path, sha256: &str) -> io::Result<bool> {
    if !path.is_file() {
        return Ok(false);
    }
    let bytes = fs::read(path)?;
    Ok(verify_sha256(&bytes, sha256, path.display().to_string().as_str()).is_ok())
}

fn download_agility_nupkg() -> io::Result<Vec<u8>> {
    const ATTEMPTS: u32 = 3;
    let mut last = None;
    for attempt in 1..=ATTEMPTS {
        match download_agility_nupkg_once() {
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

fn download_agility_nupkg_once() -> io::Result<Vec<u8>> {
    let response = ureq::get(AGILITY_URL)
        .timeout(std::time::Duration::from_secs(60))
        .call()
        .map_err(|error| io::Error::other(format!("download {AGILITY_URL}: {error}")))?;
    let mut nupkg = Vec::new();
    response
        .into_reader()
        .take(64 * 1024 * 1024)
        .read_to_end(&mut nupkg)?;
    Ok(nupkg)
}

fn verify_sha256(bytes: &[u8], expect_hex: &str, label: &str) -> io::Result<()> {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(bytes);
    let actual = hex_lower(&digest);
    if actual != expect_hex {
        return Err(io::Error::other(format!(
            "{label} sha256 mismatch: got {actual}, expected {expect_hex}"
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
