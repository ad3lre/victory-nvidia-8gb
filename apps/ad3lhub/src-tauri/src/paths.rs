use parking_lot::Mutex;
use std::path::{Path, PathBuf};

pub const APP_DIR: &str = "Ad3lHub";
pub const PROJECT_PARTS: &[&str] = &["repos", "target", "cache", "data"];
pub const EXE_NAME: &str = "game_scarlett_master.exe";
pub const REQUIRED: &[&str] = &[
    EXE_NAME,
    "MicrosoftGame.Config",
    "common.rpf",
    "data.rpf",
    "levels.rpf",
    "script.rpf",
    "shaders.rpf",
    "textures.rpf",
];
pub const FEATURED_REPO: &str = "ad3lre/victory-nvidia-8gb";
pub const FEATURED_NAME: &str = "Victory NVIDIA 8 GB";
pub const PORTWATCH: &str = "https://portwatch-vi.xyz/data/tracker.json";
pub const AUTO_CHECK_SECS: u64 = 15 * 60;

static INSTALL_DIR: Mutex<Option<PathBuf>> = Mutex::new(None);

pub fn default_workspace() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join(APP_DIR)
}

pub fn workspace() -> PathBuf {
    INSTALL_DIR
        .lock()
        .clone()
        .unwrap_or_else(default_workspace)
}

pub fn set_install_dir(dir: &str) {
    let d = dir.trim();
    *INSTALL_DIR.lock() = (!d.is_empty()).then(|| PathBuf::from(d));
}

pub fn slug(repo: &str) -> String {
    repo.replace('/', "_")
}

pub fn repo_dir(repo: &str) -> PathBuf {
    workspace().join("repos").join(slug(repo))
}

pub fn target_dir(repo: &str) -> PathBuf {
    workspace().join("target").join(slug(repo))
}

pub fn built_file(repo: &str) -> PathBuf {
    target_dir(repo).join("built-from.txt")
}

pub fn detect_build_profile(repo_dir: &Path) -> &'static str {
    let cargo_toml = repo_dir.join("Cargo.toml");
    if let Ok(text) = std::fs::read_to_string(&cargo_toml) {
        if text.contains("[profile.diag]") {
            return "diag";
        }
    }
    "release"
}

pub fn built_exe(repo: &str) -> PathBuf {
    let dir = repo_dir(repo);
    let profile = detect_build_profile(&dir);
    target_dir(repo).join(profile).join("victory.exe")
}

pub fn has_projects(dir: &Path) -> bool {
    std::fs::read_dir(dir.join("repos"))
        .map(|mut d| d.next().is_some())
        .unwrap_or(false)
}

pub fn normalize_dir(s: &str) -> PathBuf {
    let t = s.trim().trim_matches('"').trim();
    let p = PathBuf::from(t);
    if p.is_file() {
        p.parent().map(Path::to_path_buf).unwrap_or(p)
    } else {
        p
    }
}

pub fn find_game_in(dir: &Path, depth: u32) -> Option<PathBuf> {
    if dir.join(EXE_NAME).is_file() {
        return Some(dir.to_path_buf());
    }
    if depth == 0 {
        return None;
    }
    let entries = std::fs::read_dir(dir).ok()?;
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().to_lowercase();
        let skip = [
            "windows",
            "program files",
            "program files (x86)",
            "programdata",
            "appdata",
        ];
        if name.starts_with('$') || name.starts_with('.') || skip.contains(&name.as_str()) {
            continue;
        }
        if e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            if let Some(found) = find_game_in(&e.path(), depth - 1) {
                return Some(found);
            }
        }
    }
    None
}

pub fn search_drives() -> Result<String, String> {
    for letter in b'C'..=b'Z' {
        let root = PathBuf::from(format!("{}:\\", letter as char));
        if !root.is_dir() {
            continue;
        }
        if let Some(found) = find_game_in(&root, 3) {
            return Ok(found.display().to_string());
        }
    }
    Err("The game was not found on any drive. Choose its folder with Browse.".into())
}

pub fn move_dir(from: &Path, to: &Path, log: &dyn Fn(String)) -> std::io::Result<()> {
    if !from.exists() {
        return Ok(());
    }
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if !to.exists() && std::fs::rename(from, to).is_ok() {
        return Ok(());
    }
    copy_dir(from, to, log)?;
    std::fs::remove_dir_all(from)
}

fn copy_dir(from: &Path, to: &Path, log: &dyn Fn(String)) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for e in std::fs::read_dir(from)? {
        let e = e?;
        let target = to.join(e.file_name());
        if e.file_type()?.is_dir() {
            copy_dir(&e.path(), &target, log)?;
        } else {
            std::fs::copy(e.path(), &target)?;
        }
    }
    log(format!("copied {}", from.display()));
    Ok(())
}

pub fn migrate_old_folders() {
    for var in ["LOCALAPPDATA", "APPDATA"] {
        let Some(base) = std::env::var_os(var).map(PathBuf::from) else {
            continue;
        };
        for old_name in ["VI Launcher", "Mandyhub"] {
            let (old, new) = (base.join(old_name), base.join(APP_DIR));
            if old.is_dir() && !new.exists() {
                let _ = std::fs::rename(&old, &new);
            }
        }
    }
}
