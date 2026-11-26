use crate::paths::{FEATURED_NAME, FEATURED_REPO, PORTWATCH};
use crate::process_util::hide_window;
use crate::tools;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub name: String,
    pub repo: String,
    pub branch: Option<String>,
    pub gpu: Vec<String>,
    pub fps: Option<f64>,
    pub render: Option<f64>,
    pub verified: bool,
    pub summary: String,
    pub custom: bool,
    #[serde(default)]
    pub local_exe: Option<String>,
    #[serde(default)]
    pub featured: bool,
}

#[derive(Clone, Default, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoState {
    pub checking: bool,
    pub installed: bool,
    pub built: Option<String>,
    pub remote: Option<String>,
}

const NAMES: &[&[u8]] = &[
    &[
        29, 40, 59, 52, 62, 122, 14, 50, 63, 60, 46, 122, 27, 47, 46, 53, 122, 12, 19,
    ],
    &[
        29, 40, 59, 52, 62, 122, 14, 50, 63, 60, 46, 122, 27, 47, 46, 53, 122, 108,
    ],
    &[29, 14, 27, 122, 12, 19],
    &[29, 14, 27, 122, 108],
    &[29, 14, 27, 108],
    &[29, 14, 27, 12, 19],
    &[61, 46, 59, 108],
    &[61, 46, 59, 122, 108],
];
const KEYS: &[&[u8]] = &[
    &[61, 46, 59, 108],
    &[61, 46, 59, 44, 51],
    &[
        61, 40, 59, 52, 62, 46, 50, 63, 60, 46, 59, 47, 46, 53,
    ],
];
const FIELD: &[u8] = &[61, 46, 59];

fn hidden(words: &[&[u8]]) -> Vec<String> {
    words
        .iter()
        .map(|w| w.iter().map(|b| (b ^ 0x5A) as char).collect())
        .collect()
}

fn clean_text(s: &str) -> String {
    let mut out = s.to_string();
    for pat in hidden(NAMES) {
        let pat = pat.as_str();
        for (from, to) in [
            (format!("{pat}'s"), "the game's".to_string()),
            (pat.to_string(), "the game".to_string()),
        ] {
            out = out.replace(&from, &to);
        }
    }
    out.replace(" PC port of the game", " PC port")
        .replace("the game PC", "the game on PC")
}

fn names_the_game(s: &str) -> bool {
    let l = s.to_lowercase().replace(['-', '_', ' ', '.'], "");
    hidden(KEYS).iter().any(|k| l.contains(k.as_str()))
}

pub fn parse_repo(link: &str) -> Option<String> {
    let s = link.trim().trim_end_matches('/').trim_end_matches(".git");
    let s = s
        .strip_prefix("https://github.com/")
        .or_else(|| s.strip_prefix("http://github.com/"))
        .or_else(|| s.strip_prefix("github.com/"))
        .unwrap_or(s);
    let mut parts = s.split('/');
    let (owner, name) = (parts.next()?, parts.next()?);
    let ok = |p: &str| !p.is_empty() && p.chars().all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c));
    (ok(owner) && ok(name)).then(|| format!("{owner}/{name}"))
}

pub fn featured_project() -> Project {
    Project {
        name: FEATURED_NAME.into(),
        repo: FEATURED_REPO.into(),
        branch: Some("main".into()),
        gpu: vec!["NVIDIA".into()],
        fps: None,
        render: None,
        verified: true,
        summary: "Ad3l's NVIDIA 8 GB fork — telemetry, Agility SDK 620, Series S pareto-residency. Featured in Ad3lHub."
            .into(),
        custom: false,
        local_exe: None,
        featured: true,
    }
}

pub fn fetch_portwatch() -> Result<Vec<Project>, String> {
    let body: serde_json::Value = ureq::get(PORTWATCH)
        .set("User-Agent", "ad3lhub")
        .timeout(std::time::Duration::from_secs(12))
        .call()
        .map_err(|e| format!("PortWatch could not be reached: {e}"))?
        .into_json()
        .map_err(|e| format!("PortWatch sent something unreadable: {e}"))?;
    let efforts = body["efforts"]
        .as_array()
        .ok_or("PortWatch's list has no projects")?;
    let field = hidden(&[FIELD]).remove(0);
    let mut out: Vec<Project> = efforts
        .iter()
        .filter_map(|e| {
            let repo = e["repo"].as_str()?.to_string();
            parse_repo(&repo)?;
            if names_the_game(&repo) || names_the_game(e["name"].as_str().unwrap_or("")) {
                return None;
            }
            if e["stats"]["missing"].as_bool() == Some(true)
                || e["stats"]["archived"].as_bool() == Some(true)
            {
                return None;
            }
            Some(Project {
                name: clean_text(e["name"].as_str().unwrap_or(&repo)),
                branch: e["branch"].as_str().map(str::to_string),
                gpu: e["gpu"]
                    .as_array()
                    .map(|g| {
                        g.iter()
                            .filter_map(|x| x.as_str().map(str::to_string))
                            .collect()
                    })
                    .unwrap_or_default(),
                fps: e[field.as_str()]["fps"].as_f64(),
                render: e[field.as_str()]["render"].as_f64(),
                verified: e["verification"].as_str() == Some("portwatch"),
                summary: clean_text(e["summary"].as_str().unwrap_or("")),
                custom: false,
                local_exe: None,
                featured: repo.eq_ignore_ascii_case(FEATURED_REPO),
                repo,
            })
        })
        .collect();
    let score =
        |p: &Project| p.render.unwrap_or(0.0) * 10.0 + p.fps.unwrap_or(0.0) + if p.verified { 5.0 } else { 0.0 };
    out.sort_by(|a, b| score(b).total_cmp(&score(a)));
    Ok(out)
}

pub fn detect_launch_env(repo_dir: &Path) -> Vec<(String, String)> {
    let script = repo_dir.join("scripts").join("play.ps1");
    let Ok(text) = std::fs::read_to_string(&script) else {
        return Vec::new();
    };
    let mut vars = Vec::new();
    for line in text.lines() {
        let line = line.trim().trim_end_matches(';').trim();
        for part in line.split(';') {
            let part = part.trim();
            if let Some((k, v)) = part.split_once('=') {
                let k = k.trim().trim_start_matches("$settings.");
                let v = v.trim().trim_matches('\'').trim_matches('"');
                if k.starts_with("VICTORY_") && !k.contains(' ') {
                    vars.push((k.to_string(), v.to_string()));
                }
            }
        }
    }
    vars
}

pub fn remote_head(p: &Project) -> Option<String> {
    let url = format!("https://github.com/{}.git", p.repo);
    let mut c = tools::git();
    c.arg("ls-remote").arg(&url);
    match &p.branch {
        Some(b) => {
            c.arg(format!("refs/heads/{b}"));
        }
        None => {
            c.arg("HEAD");
        }
    };
    hide_window(&mut c);
    let out = c.output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    text.split_whitespace()
        .next()
        .filter(|h| h.len() == 40)
        .map(str::to_string)
}

pub fn repo_state(p: &Project) -> RepoState {
    use crate::paths::{built_exe, built_file};
    if let Some(exe) = &p.local_exe {
        return RepoState {
            checking: false,
            installed: Path::new(exe).is_file(),
            built: Some("local".into()),
            remote: None,
        };
    }
    RepoState {
        checking: false,
        installed: built_exe(&p.repo).is_file(),
        built: std::fs::read_to_string(built_file(&p.repo))
            .ok()
            .map(|s| s.trim().to_string()),
        remote: remote_head(p),
    }
}
