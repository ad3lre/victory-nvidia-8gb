use crate::process_util::hide_window;
use parking_lot::Mutex;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Clone, Default, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Tools {
    pub git: Option<String>,
    pub cargo: Option<String>,
    pub msvc: bool,
    pub winget: bool,
}

impl Tools {
    pub fn ready(&self) -> bool {
        self.git.is_some() && self.cargo.is_some() && self.msvc
    }

    #[allow(dead_code)]
    pub fn missing(&self) -> Vec<&'static str> {
        let mut m = Vec::new();
        if self.git.is_none() {
            m.push("Git");
        }
        if self.cargo.is_none() {
            m.push("Rust");
        }
        if !self.msvc {
            m.push("C++ build tools");
        }
        m
    }
}

static FOUND: Mutex<Option<Tools>> = Mutex::new(None);

pub fn current() -> Option<Tools> {
    FOUND.lock().clone()
}

fn env_path(var: &str) -> Option<PathBuf> {
    std::env::var_os(var).map(PathBuf::from)
}

fn answers(program: &Path) -> bool {
    let mut c = Command::new(program);
    c.arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    hide_window(&mut c);
    c.status().map(|s| s.success()).unwrap_or(false)
}

fn find(name: &str, places: &[Option<PathBuf>]) -> Option<PathBuf> {
    if answers(Path::new(name)) {
        return Some(PathBuf::from(name));
    }
    places
        .iter()
        .flatten()
        .find(|p| p.is_file() && answers(p))
        .cloned()
}

fn msvc_installed() -> bool {
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Some(pf86) = env_path("ProgramFiles(x86)") {
        let vswhere = pf86
            .join("Microsoft Visual Studio")
            .join("Installer")
            .join("vswhere.exe");
        if vswhere.is_file() {
            let mut c = Command::new(&vswhere);
            c.args(["-all", "-products", "*", "-property", "installationPath"]);
            hide_window(&mut c);
            if let Ok(o) = c.output() {
                roots.extend(
                    String::from_utf8_lossy(&o.stdout)
                        .lines()
                        .map(str::trim)
                        .filter(|l| !l.is_empty())
                        .map(PathBuf::from),
                );
            }
        }
    }
    for base in [env_path("ProgramFiles"), env_path("ProgramFiles(x86)")]
        .into_iter()
        .flatten()
    {
        let vs = base.join("Microsoft Visual Studio");
        for year in std::fs::read_dir(&vs).into_iter().flatten().flatten() {
            for edition in std::fs::read_dir(year.path()).into_iter().flatten().flatten() {
                roots.push(edition.path());
            }
        }
    }
    roots.iter().any(|root| {
        std::fs::read_dir(root.join("VC").join("Tools").join("MSVC"))
            .into_iter()
            .flatten()
            .flatten()
            .any(|v| {
                v.path()
                    .join("bin")
                    .join("Hostx64")
                    .join("x64")
                    .join("link.exe")
                    .is_file()
            })
    })
}

/// Drop cached detection so the next `detect()` re-probes PATH (e.g. after winget).
pub fn invalidate() {
    *FOUND.lock() = None;
}

pub fn detect() -> Tools {
    let pf = env_path("ProgramFiles");
    let pf86 = env_path("ProgramFiles(x86)");
    let local = env_path("LOCALAPPDATA");
    let home = env_path("USERPROFILE");
    let git = find(
        "git",
        &[
            pf.as_ref().map(|p| p.join("Git").join("cmd").join("git.exe")),
            pf86.as_ref()
                .map(|p| p.join("Git").join("cmd").join("git.exe")),
            local
                .as_ref()
                .map(|p| p.join("Programs").join("Git").join("cmd").join("git.exe")),
        ],
    );
    let cargo = find(
        "cargo",
        &[
            env_path("CARGO_HOME").map(|p| p.join("bin").join("cargo.exe")),
            home.as_ref()
                .map(|p| p.join(".cargo").join("bin").join("cargo.exe")),
        ],
    );
    let t = Tools {
        git: git.map(|p| p.display().to_string()),
        cargo: cargo.map(|p| p.display().to_string()),
        msvc: msvc_installed(),
        winget: answers(Path::new("winget")),
    };
    *FOUND.lock() = Some(t.clone());
    t
}

fn command(path: Option<PathBuf>, name: &str) -> Command {
    let program = path.unwrap_or_else(|| PathBuf::from(name));
    let mut c = Command::new(&program);
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(t) = current() {
        for p in [t.git, t.cargo].into_iter().flatten() {
            let pb = PathBuf::from(p);
            if let Some(d) = pb.parent().filter(|d| !d.as_os_str().is_empty()) {
                dirs.push(d.to_path_buf());
            }
        }
    }
    if !dirs.is_empty() {
        if let Some(path) = std::env::var_os("PATH") {
            dirs.extend(std::env::split_paths(&path));
        }
        if let Ok(joined) = std::env::join_paths(dirs) {
            c.env("PATH", joined);
        }
    }
    c
}

pub fn git() -> Command {
    command(current().and_then(|t| t.git.map(PathBuf::from)), "git")
}

pub fn cargo() -> Command {
    command(current().and_then(|t| t.cargo.map(PathBuf::from)), "cargo")
}

pub fn install_steps(t: &Tools) -> Vec<(&'static str, Vec<String>)> {
    let common = [
        "--exact",
        "--source",
        "winget",
        "--accept-package-agreements",
        "--accept-source-agreements",
    ];
    let mut steps = Vec::new();
    let mut step = |what: &'static str, id: &str, extra: &[&str]| {
        let mut a: Vec<String> = vec!["install".into(), "--id".into(), id.into()];
        a.extend(common.iter().map(|s| s.to_string()));
        a.extend(extra.iter().map(|s| s.to_string()));
        steps.push((what, a));
    };
    if t.git.is_none() {
        step("Git", "Git.Git", &["--silent"]);
    }
    if !t.msvc {
        step(
            "C++ build tools",
            "Microsoft.VisualStudio.2022.BuildTools",
            &[
                "--override",
                "--wait --passive --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended",
            ],
        );
    }
    if t.cargo.is_none() {
        step("Rust", "Rustlang.Rustup", &["--silent"]);
    }
    steps
}
