mod catalog;
mod flags;
mod fullscreen;
mod jobs;
mod paths;
mod process_util;
mod tools;
mod warm_state;

use catalog::{featured_project, fetch_portwatch, parse_repo, repo_state, Project, RepoState};
use flags::{preset_flags, resolve_env, FlagCatalog, LaunchPlan};
use jobs::{install, install_tools, play, uninstall, warm_shaders, JobSnapshot, SharedJob};
use parking_lot::Mutex;
use paths::{
    find_game_in, has_projects, migrate_old_folders, move_dir, normalize_dir, search_drives,
    set_install_dir, workspace, PROJECT_PARTS, REQUIRED,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub game_dir: String,
    #[serde(default)]
    pub fix_8gb: bool,
    #[serde(default)]
    pub bright_shadows: bool,
    pub defaults_set: bool,
    pub fullscreen: bool,
    pub install_dir: String,
    pub later: Vec<(String, String)>,
    pub selected: Option<String>,
    pub custom: Vec<Project>,
    #[serde(default)]
    pub series_s: bool,
    #[serde(default = "default_scale")]
    pub render_scale: f32,
    pub extra_env: String,
    pub gpu_filter: usize,
    #[serde(default = "default_preset")]
    pub preset: String,
    #[serde(default)]
    pub flags: BTreeMap<String, String>,
    #[serde(default)]
    pub show_expert: bool,
    /// Run `victory shaders` before Play when the warm marker is stale (AnyXboxX trick).
    #[serde(default = "default_true")]
    pub warm_before_play: bool,
    /// Always re-warm even when the marker matches.
    #[serde(default)]
    pub always_rewarm: bool,
    /// Also run `victory translate --pipelines` after shaders (much slower; GPU-specific).
    /// On by default so first Play builds `host-pipelines.bin` and avoids the cold-boot hang.
    #[serde(default = "default_true")]
    pub warm_pipelines: bool,
}

fn default_true() -> bool {
    true
}

fn default_preset() -> String {
    "ad3l-balanced".into()
}

fn default_scale() -> f32 {
    0.5
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            game_dir: String::new(),
            fix_8gb: true,
            bright_shadows: true,
            defaults_set: false,
            fullscreen: false,
            install_dir: String::new(),
            later: Vec::new(),
            selected: None,
            custom: Vec::new(),
            series_s: true,
            render_scale: 0.5,
            extra_env: String::new(),
            gpu_filter: 0,
            preset: default_preset(),
            flags: BTreeMap::new(),
            show_expert: false,
            warm_before_play: true,
            always_rewarm: false,
            warm_pipelines: true,
        }
    }
}

impl Settings {
    fn ensure_defaults(mut self) -> Self {
        if self.render_scale <= 0.0 {
            self.render_scale = 0.5;
        }
        if self.preset.is_empty() {
            self.preset = default_preset();
        }
        if !self.defaults_set {
            self.defaults_set = true;
            self.fix_8gb = true;
            self.bright_shadows = true;
            self.series_s = true;
            self.render_scale = 0.5;
            self.preset = default_preset();
            self.flags = preset_flags("ad3l-balanced");
            // Mirror legacy Mandyhub toggles into the flag map once.
            if self.fix_8gb {
                self.flags
                    .insert("TEXTURE_DROP_MIPS".into(), "2".into());
                self.flags.insert("NULL_COLOR".into(), "grey".into());
            }
            if self.bright_shadows {
                self.flags.insert("AMBIENT".into(), "0.6".into());
            }
            if self.series_s {
                self.flags.insert("CONSOLE".into(), "series-s".into());
                self.flags.insert("OWN_UPSCALER".into(), "1".into());
                self.flags
                    .insert("RENDER_SCALE".into(), format!("{:.2}", self.render_scale));
            }
        } else if self.flags.is_empty() && self.preset != "blank" && self.preset != "custom" {
            self.flags = preset_flags(&self.preset);
        }
        self
    }

    fn launch_plan(&self) -> LaunchPlan {
        LaunchPlan {
            preset: self.preset.clone(),
            flags: self.flags.clone(),
            extra_env: self.extra_env.clone(),
            fullscreen_hook: self.fullscreen,
        }
    }
}

struct AppState {
    settings: Arc<Mutex<Settings>>,
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
    // Always recompute — a previous "checking" early-return could stick forever
    // if two invokes raced. Mark checking only as a UI hint while we work.
    {
        let mut m = state.repos.lock();
        let e = m.entry(project.repo.clone()).or_default();
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

fn same_commit(a: &str, b: &str) -> bool {
    let a = a.trim().to_ascii_lowercase();
    let b = b.trim().to_ascii_lowercase();
    if a.is_empty() || b.is_empty() {
        return false;
    }
    a == b || a.starts_with(&b) || b.starts_with(&a)
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
    if !paths::built_exe(&project.repo).is_file() {
        return None;
    }
    // Prefer freshly resolved remote tip over a stale cache entry.
    let remote = catalog::remote_head(&project).or_else(|| {
        state
            .repos
            .lock()
            .get(&project.repo)
            .and_then(|r| r.remote.clone())
    })?;
    let built = std::fs::read_to_string(paths::built_file(&project.repo))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())?;
    if same_commit(&built, &remote) {
        return None;
    }
    let later = state
        .settings
        .lock()
        .later
        .iter()
        .any(|(r, c)| r == &project.repo && same_commit(c, &remote));
    (!later).then_some(remote)
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
enum JobKind {
    Install,
    Play,
    Uninstall,
    InstallTools,
    WarmShaders,
}

#[tauri::command]
fn start_job(
    app: AppHandle,
    state: State<'_, AppState>,
    kind: JobKind,
    project: Option<Project>,
) -> Result<(), String> {
    let needs_project = !matches!(kind, JobKind::InstallTools);
    if needs_project && project.is_none() {
        return Err("no project selected for this job".into());
    }
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
    let settings_live = state.settings.clone();
    let settings_snap = state.settings.lock().clone();
    let repos = state.repos.clone();
    let settings_path = state.settings_path.clone();

    std::thread::spawn(move || {
        let result = match kind {
            JobKind::InstallTools => install_tools(&app, &job),
            JobKind::Install => match project.as_ref() {
                None => Err("no project selected".into()),
                Some(p) => {
                    let updating = paths::built_exe(&p.repo).is_file();
                    let r = install(&app, &job, p);
                    if r.is_ok() {
                        // Drop deferred "Later" for this repo — tip is installed now.
                        {
                            let mut s = settings_live.lock();
                            s.later.retain(|(repo, _)| repo != &p.repo);
                            let _ = save_settings(&settings_path, &s);
                        }
                        let mut j = job.lock();
                        j.stage = if updating {
                            "Updated: ready to play".into()
                        } else {
                            "Installed: ready to play".into()
                        };
                    }
                    r
                }
            },
            JobKind::Uninstall => match project.as_ref() {
                Some(p) => uninstall(&app, &job, p),
                None => Err("no project selected".into()),
            },
            JobKind::WarmShaders => match project.as_ref() {
                None => Err("no project selected".into()),
                Some(p) => {
                    let plan = settings_snap.launch_plan();
                    let env = resolve_env(&plan);
                    warm_shaders(
                        &app,
                        &job,
                        p,
                        &PathBuf::from(&settings_snap.game_dir),
                        &env,
                        settings_snap.always_rewarm,
                        settings_snap.warm_pipelines,
                    )
                }
            },
            JobKind::Play => match project.as_ref() {
                None => Err("no project selected".into()),
                Some(p) => {
                    let plan = settings_snap.launch_plan();
                    let env = resolve_env(&plan);
                    play(
                        &app,
                        &job,
                        p,
                        &PathBuf::from(&settings_snap.game_dir),
                        &env,
                        plan.fullscreen_hook,
                        settings_snap.warm_before_play,
                        settings_snap.always_rewarm,
                        settings_snap.warm_pipelines,
                    )
                }
            },
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
                            JobKind::WarmShaders => "Shaders warmed".into(),
                            JobKind::Install => j.stage.clone(),
                        };
                    }
                }
                Err(e) => {
                    j.failed = true;
                    j.stage = match kind {
                        JobKind::Install => "Install failed (see the log)".into(),
                        JobKind::WarmShaders => "Warm failed (see the log)".into(),
                        _ => "Failed".into(),
                    };
                    j.log.push(format!("ERROR: {e}"));
                }
            }
        }
        let snap = job.lock().snapshot();
        let _ = app.emit("job", &snap);

        if let Some(p) = &project {
            if !matches!(kind, JobKind::Play | JobKind::WarmShaders) {
                let st = repo_state(p);
                repos.lock().insert(p.repo.clone(), st.clone());
                let _ = app.emit("repo", &(p.repo.clone(), st));
            }
        }
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

#[tauri::command]
fn flag_catalog() -> FlagCatalog {
    FlagCatalog {
        presets: flags::presets(),
        flags: flags::catalog(),
    }
}

#[tauri::command]
fn apply_preset(state: State<'_, AppState>, preset: String) -> Result<Settings, String> {
    let mut s = state.settings.lock().clone();
    s.preset = preset.clone();
    s.flags = if preset == "custom" {
        s.flags
    } else {
        preset_flags(&preset)
    };
    // Keep legacy mirrors roughly in sync for older UI bits.
    if let Some(console) = s.flags.get("CONSOLE") {
        s.series_s = console == "series-s";
    }
    if let Some(scale) = s.flags.get("RENDER_SCALE") {
        if let Ok(v) = scale.parse::<f32>() {
            s.render_scale = v;
        }
    }
    s.fix_8gb = s.flags.get("TEXTURE_DROP_MIPS").map(|v| v != "0").unwrap_or(false)
        && s.flags.get("NULL_COLOR").map(|v| !v.is_empty()).unwrap_or(false);
    s.bright_shadows = s
        .flags
        .get("AMBIENT")
        .map(|v| !v.is_empty())
        .unwrap_or(false);
    save_settings(&state.settings_path, &s)?;
    *state.settings.lock() = s.clone();
    Ok(s)
}

#[tauri::command]
fn preview_env(state: State<'_, AppState>) -> BTreeMap<String, String> {
    resolve_env(&state.settings.lock().launch_plan())
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
            settings: Arc::new(Mutex::new(settings)),
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
            flag_catalog,
            apply_preset,
            preview_env,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launch_presets_leave_relay_copy_off() {
        for id in ["ad3l-balanced", "pareto", "series-s-low", "nvidia-8gb", "blank"] {
            let flags = preset_flags(id);
            assert!(
                !flags.contains_key("RELAY_COPY") || flags.get("RELAY_COPY").is_some_and(|v| v == "0"),
                "{id} must not enable RELAY_COPY by default"
            );
            let env = resolve_env(&LaunchPlan {
                preset: id.to_string(),
                ..Default::default()
            });
            assert_ne!(env.get("VICTORY_RELAY_COPY").map(String::as_str), Some("1"), "{id}");
        }
    }

    #[test]
    fn launch_presets_default_to_play_fps_diagnostics() {
        for id in ["ad3l-balanced", "pareto"] {
            let flags = preset_flags(id);
            assert_eq!(flags.get("D3D12_DRED").map(String::as_str), Some("faults"), "{id}");
            assert_eq!(flags.get("D3D12_DEBUG").map(String::as_str), Some("0"), "{id}");
            assert_eq!(flags.get("VIEW_BYTES").map(String::as_str), Some("0"), "{id}");
            assert_eq!(flags.get("WIDE_SPIN").map(String::as_str), Some("50"), "{id}");
            assert_eq!(flags.get("BVH_NO_HIT").map(String::as_str), Some("1"), "{id}");
            assert_eq!(flags.get("BVH_SKIP_COPY").map(String::as_str), Some("1"), "{id}");
            assert_eq!(flags.get("EARLY_DEPTH").map(String::as_str), Some("1"), "{id}");
        }
    }

    #[test]
    fn warm_pipelines_defaults_on_for_new_and_legacy_settings() {
        assert!(Settings::default().warm_pipelines);
        // Minimal saved settings (camelCase): omit warmPipelines → default true.
        let legacy: Settings = serde_json::from_str(
            r#"{
              "gameDir":"",
              "defaultsSet":false,
              "fullscreen":false,
              "installDir":"",
              "later":[],
              "selected":null,
              "custom":[],
              "extraEnv":"",
              "gpuFilter":0
            }"#,
        )
        .unwrap();
        assert!(legacy.warm_pipelines);
        let off: Settings = serde_json::from_str(
            r#"{
              "gameDir":"",
              "defaultsSet":false,
              "fullscreen":false,
              "installDir":"",
              "later":[],
              "selected":null,
              "custom":[],
              "extraEnv":"",
              "gpuFilter":0,
              "warmPipelines":false
            }"#,
        )
        .unwrap();
        assert!(!off.warm_pipelines);
    }
}
