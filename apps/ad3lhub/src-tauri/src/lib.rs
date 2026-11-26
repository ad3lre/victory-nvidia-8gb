mod catalog;
mod fullscreen;
mod jobs;
mod paths;
mod process_util;
mod tools;

use catalog::{featured_project, fetch_portwatch, parse_repo, repo_state, Project, RepoState};
use jobs::{install, install_tools, play, uninstall, JobSnapshot, SharedJob};
use parking_lot::Mutex;
use paths::{
    find_game_in, has_projects, migrate_old_folders, move_dir, normalize_dir, search_drives,
    set_install_dir, workspace, PROJECT_PARTS, REQUIRED,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub game_dir: String,
    pub fix_8gb: bool,
    pub bright_shadows: bool,
    pub defaults_set: bool,
    pub fullscreen: bool,
    pub install_dir: String,
    pub later: Vec<(String, String)>,
    pub selected: Option<String>,
    pub custom: Vec<Project>,
    pub series_s: bool,
    pub render_scale: f32,
    pub extra_env: String,
    pub gpu_filter: usize,
}

impl Settings {
    fn ensure_defaults(mut self) -> Self {
        if self.render_scale <= 0.0 {
            self.render_scale = 0.5;
        }
        if !self.defaults_set {
            self.defaults_set = true;
            self.fix_8gb = true;
            self.bright_shadows = true;
            self.series_s = true;
            self.render_scale = 0.5;
        }
        self
    }
}

struct AppState {
    settings: Mutex<Settings>,
    settings_path: PathBuf,
    job: SharedJob,
    repos: Arc<Mutex<HashMap<String, RepoState>>>,
}

fn settings_path() -> PathBuf {
    paths::default_workspace().join("settings.json")
}

fn load_settings() -> Settings {
    let path = settings_path();
    let s: Settings = std::fs::read_to_string(&path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();
    s.ensure_defaults()
}

fn save_settings(path: &PathBuf, s: &Settings) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let text = serde_json::to_string_pretty(s).map_err(|e| e.to_string())?;
    std::fs::write(path, text).map_err(|e| e.to_string())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CatalogOut {
    featured: Project,
    community: Vec<Project>,
    error: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FilesCheck {
    game_dir: String,
    empty: bool,
    files: Vec<(String, bool)>,
    ok: bool,
}

#[tauri::command]
fn get_settings(state: State<'_, AppState>) -> Settings {
    state.settings.lock().clone()
}

#[tauri::command]
fn save_settings_cmd(state: State<'_, AppState>, settings: Settings) -> Result<Settings, String> {
    let s = settings.ensure_defaults();
    set_install_dir(&s.install_dir);
    save_settings(&state.settings_path, &s)?;
    *state.settings.lock() = s.clone();
    Ok(s)
}

#[tauri::command]
fn fetch_catalog(state: State<'_, AppState>) -> CatalogOut {
    let custom = state.settings.lock().custom.clone();
    match fetch_portwatch() {
        Ok(mut list) => {
            list.retain(|p| !p.repo.eq_ignore_ascii_case(paths::FEATURED_REPO));
            let mut community = custom;
            community.extend(list);
            CatalogOut {
                featured: featured_project(),
                community,
                error: None,
            }
        }
        Err(e) => CatalogOut {
            featured: featured_project(),
            community: custom,
            error: Some(e),
        },
    }
}

#[tauri::command]
fn detect_tools() -> tools::Tools {
    tools::detect()
}

#[tauri::command]
fn check_game_files(state: State<'_, AppState>) -> FilesCheck {
    let game_dir = state.settings.lock().game_dir.clone();
    let mut dir = normalize_dir(&game_dir);
    if !game_dir.trim().is_empty() && !dir.join(paths::EXE_NAME).is_file() {
        if let Some(found) = find_game_in(&dir, 2) {
            dir = found;
        }
    }
    let text = dir.display().to_string();
    if !game_dir.trim().is_empty() && text != game_dir {
        state.settings.lock().game_dir = text.clone();
        let _ = save_settings(&state.settings_path, &state.settings.lock());
    }
    let empty = !game_dir.is_empty()
        && std::fs::read_dir(&dir)
            .map(|mut d| d.next().is_none())
            .unwrap_or(true);
    let files: Vec<(String, bool)> = REQUIRED
        .iter()
        .map(|f| {
            (
                (*f).to_string(),
                !game_dir.is_empty() && dir.join(f).is_file(),
            )
        })
        .collect();
    let ok = !files.is_empty() && files.iter().all(|f| f.1);
    FilesCheck {
        game_dir: state.settings.lock().game_dir.clone(),
        empty,
        files,
        ok,
    }
}

#[tauri::command]
fn search_game_folder() -> Result<String, String> {
    search_drives()
}

#[tauri::command]
fn get_job(state: State<'_, AppState>) -> JobSnapshot {
    state.job.lock().snapshot()
}

#[tauri::command]
fn check_repo(state: State<'_, AppState>, project: Project) -> RepoState {
    {
        let mut m = state.repos.lock();
        let e = m.entry(project.repo.clone()).or_default();
        if e.checking {
            return e.clone();
        }
        e.checking = true;
    }
    let st = repo_state(&project);
    state.repos.lock().insert(project.repo.clone(), st.clone());
    st
}

#[tauri::command]
fn repo_statuses(state: State<'_, AppState>) -> HashMap<String, RepoState> {
    state.repos.lock().clone()
}

#[tauri::command]
fn update_available(state: State<'_, AppState>, project: Project) -> Option<String> {
    if project.local_exe.is_some() {
        return None;
    }
    {
        let j = state.job.lock();
        if j.running && j.repo.as_deref() == Some(project.repo.as_str()) {
            return None;
        }
    }
    let remote = state.repos.lock().get(&project.repo)?.remote.clone()?;
    if !paths::built_exe(&project.repo).is_file() {
        return None;
    }
    let built = std::fs::read_to_string(paths::built_file(&project.repo))
        .ok()
        .map(|s| s.trim().to_string());
    let newer = built.as_deref() != Some(remote.as_str());
    let later = state
        .settings
        .lock()
        .later
        .iter()
        .any(|(r, c)| r == &project.repo && c == &remote);
    (newer && !later).then_some(remote)
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
enum JobKind {
    Install,
    Play,
    Uninstall,
    InstallTools,
}

#[tauri::command]
fn start_job(
    app: AppHandle,
    state: State<'_, AppState>,
    kind: JobKind,
    project: Option<Project>,
) -> Result<(), String> {
    {
        let mut j = state.job.lock();
        if j.running {
            return Err("a job is already running".into());
        }
        *j = jobs::Job {
            running: true,
            stage: "Starting".into(),
            repo: project.as_ref().map(|p| p.repo.clone()),
            ..Default::default()
        };
    }
    let _ = app.emit("job", &state.job.lock().snapshot());

    let job = state.job.clone();
    let settings = state.settings.lock().clone();
    let repos = state.repos.clone();
    let settings_path = state.settings_path.clone();

    std::thread::spawn(move || {
        let result = match kind {
            JobKind::InstallTools => install_tools(&app, &job),
            JobKind::Install => {
                let p = project.as_ref().expect("project");
                let updating = paths::built_exe(&p.repo).is_file();
                let r = install(&app, &job, p);
                if r.is_ok() {
                    let mut j = job.lock();
                    j.stage = if updating {
                        "Updated: ready to play".into()
                    } else {
                        "Installed: ready to play".into()
                    };
                }
                r
            }
            JobKind::Uninstall => uninstall(&app, &job, project.as_ref().expect("project")),
            JobKind::Play => {
                let p = project.as_ref().expect("project");
                let mut extra = settings.extra_env.clone();
                if settings.fix_8gb {
                    extra.push_str("\nVICTORY_TEXTURE_DROP_MIPS=2\nVICTORY_NULL_COLOR=grey");
                }
                if settings.bright_shadows {
                    extra.push_str("\nVICTORY_AMBIENT=0.6");
                }
                play(
                    &app,
                    &job,
                    p,
                    &PathBuf::from(&settings.game_dir),
                    settings.series_s,
                    settings.render_scale,
                    &extra,
                    settings.fullscreen,
                )
            }
        };

        {
            let mut j = job.lock();
            j.running = false;
            match &result {
                Ok(()) => {
                    if j.stage == "Starting" || j.stage.is_empty() {
                        j.stage = match kind {
                            JobKind::Uninstall => "Uninstalled".into(),
                            JobKind::Play => "Game closed".into(),
                            JobKind::InstallTools => "Tools installed".into(),
                            JobKind::Install => j.stage.clone(),
                        };
                    }
                }
                Err(e) => {
                    j.failed = true;
                    j.stage = match kind {
                        JobKind::Install => "Install failed (see the log)".into(),
                        _ => "Failed".into(),
                    };
                    j.log.push(format!("ERROR: {e}"));
                }
            }
        }
        let _ = app.emit("job", &job.lock().snapshot());

        if let Some(p) = project {
            if !matches!(kind, JobKind::Play) {
                let st = repo_state(&p);
                repos.lock().insert(p.repo.clone(), st);
            }
        }
        let _ = settings_path;
    });

    Ok(())
}

#[tauri::command]
fn add_custom_repo(state: State<'_, AppState>, link: String) -> Result<Project, String> {
    let repo = parse_repo(&link).ok_or_else(|| "not a GitHub owner/name link".to_string())?;
    let p = Project {
        name: repo.clone(),
        repo: repo.clone(),
        branch: None,
        gpu: Vec::new(),
        fps: None,
        render: None,
        verified: false,
        summary: "Custom repository".into(),
        custom: true,
        local_exe: None,
        featured: false,
    };
    {
        let mut s = state.settings.lock();
        if s.custom.iter().any(|c| c.repo == repo)
            || repo.eq_ignore_ascii_case(paths::FEATURED_REPO)
        {
            return Err("already in the list".into());
        }
        s.custom.push(p.clone());
        save_settings(&state.settings_path, &s)?;
    }
    Ok(p)
}

#[tauri::command]
fn defer_update(state: State<'_, AppState>, repo: String, commit: String) -> Result<(), String> {
    let mut s = state.settings.lock();
    s.later.retain(|(r, _)| r != &repo);
    s.later.push((repo, commit));
    save_settings(&state.settings_path, &s)
}

#[tauri::command]
fn set_install_folder(
    state: State<'_, AppState>,
    dir: String,
    move_projects: bool,
) -> Result<Settings, String> {
    let from = workspace();
    let to = PathBuf::from(dir.trim());
    if move_projects && has_projects(&from) && from != to {
        let log = |m: String| eprintln!("{m}");
        for part in PROJECT_PARTS {
            move_dir(&from.join(part), &to.join(part), &log)
                .map_err(|e| format!("move {part}: {e}"))?;
        }
    }
    let mut s = state.settings.lock().clone();
    s.install_dir = to.display().to_string();
    set_install_dir(&s.install_dir);
    save_settings(&state.settings_path, &s)?;
    *state.settings.lock() = s.clone();
    Ok(s)
}

#[tauri::command]
fn workspace_path() -> String {
    workspace().display().to_string()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    migrate_old_folders();
    let settings = load_settings();
    set_install_dir(&settings.install_dir);
    let settings_path = settings_path();
    let _ = std::fs::create_dir_all(paths::default_workspace());

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_process::init())
        .manage(AppState {
            settings: Mutex::new(settings),
            settings_path,
            job: Arc::new(Mutex::new(jobs::Job::default())),
            repos: Arc::new(Mutex::new(HashMap::new())),
        })
        .invoke_handler(tauri::generate_handler![
            get_settings,
            save_settings_cmd,
            fetch_catalog,
            detect_tools,
            check_game_files,
            search_game_folder,
            get_job,
            check_repo,
            repo_statuses,
            update_available,
            start_job,
            add_custom_repo,
            defer_update,
            set_install_folder,
            workspace_path,
        ])
        .setup(|app| {
            let _ = tools::detect();
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                loop {
                    std::thread::sleep(std::time::Duration::from_secs(paths::AUTO_CHECK_SECS));
                    let _ = handle.emit("auto-check", ());
                }
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Ad3lHub");
}
