use crate::catalog::{detect_launch_env, Project};
use crate::fullscreen;
use crate::paths::{
    built_exe, built_file, detect_build_profile, repo_dir, slug, target_dir, workspace, EXE_NAME,
};
use crate::process_util::hide_window;
use crate::tools;
use crate::warm_state;
use parking_lot::Mutex;
use serde::Serialize;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobSnapshot {
    pub running: bool,
    pub repo: Option<String>,
    pub stage: String,
    pub log: Vec<String>,
    pub failed: bool,
}

#[derive(Default)]
pub struct Job {
    pub running: bool,
    pub repo: Option<String>,
    pub stage: String,
    pub log: Vec<String>,
    pub failed: bool,
}

impl Job {
    pub fn snapshot(&self) -> JobSnapshot {
        JobSnapshot {
            running: self.running,
            repo: self.repo.clone(),
            stage: self.stage.clone(),
            log: self.log.clone(),
            failed: self.failed,
        }
    }
}

pub type SharedJob = Arc<Mutex<Job>>;

fn emit_job(app: &AppHandle, job: &SharedJob) {
    let snap = job.lock().snapshot();
    let _ = app.emit("job", &snap);
}

fn stage(app: &AppHandle, job: &SharedJob, s: &str) {
    {
        let mut j = job.lock();
        j.stage = s.to_string();
        j.log.push(format!("== {s}"));
    }
    emit_job(app, job);
}

fn push_log(app: &AppHandle, job: &SharedJob, line: String) {
    job.lock().log.push(line);
    emit_job(app, job);
}

pub fn run_cmd(app: &AppHandle, job: &SharedJob, c: &mut Command) -> Result<(), String> {
    run_with(app, job, c, |_| {})
}

fn run_with(
    app: &AppHandle,
    job: &SharedJob,
    c: &mut Command,
    spawned: impl FnOnce(u32),
) -> Result<(), String> {
    c.stdout(Stdio::piped()).stderr(Stdio::piped());
    hide_window(c);
    let mut child = c
        .spawn()
        .map_err(|e| format!("could not start {:?}: {e}", c.get_program()))?;
    spawned(child.id());
    let (out, err) = (child.stdout.take(), child.stderr.take());
    let job_o = job.clone();
    let app_o = app.clone();
    let t_out = std::thread::spawn(move || {
        if let Some(out) = out {
            for line in BufReader::new(out).lines().flatten() {
                push_log(&app_o, &job_o, line);
            }
        }
    });
    let job_e = job.clone();
    let app_e = app.clone();
    let t_err = std::thread::spawn(move || {
        if let Some(err) = err {
            for line in BufReader::new(err).lines().flatten() {
                push_log(&app_e, &job_e, line);
            }
        }
    });
    let status = child
        .wait()
        .map_err(|e| format!("process wait failed: {e}"))?;
    let _ = t_out.join();
    let _ = t_err.join();
    if status.success() {
        Ok(())
    } else {
        Err(format!("exit {}", status.code().unwrap_or(-1)))
    }
}

fn git_head(dir: &Path) -> Result<String, String> {
    let mut c = tools::git();
    c.args(["rev-parse", "HEAD"]).current_dir(dir);
    hide_window(&mut c);
    let out = c
        .output()
        .map_err(|e| format!("git rev-parse HEAD failed: {e}"))?;
    if !out.status.success() {
        return Err("git rev-parse HEAD failed".into());
    }
    let hash = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if hash.len() < 7 {
        return Err(format!("unexpected HEAD hash: {hash}"));
    }
    Ok(hash)
}

pub fn install(app: &AppHandle, job: &SharedJob, p: &Project) -> Result<(), String> {
    let ws = workspace();
    std::fs::create_dir_all(ws.join("repos"))
        .map_err(|e| format!("cannot make {}: {e}", ws.display()))?;
    let dir = repo_dir(&p.repo);
    let url = format!("https://github.com/{}.git", p.repo);
    if dir.join(".git").is_dir() {
        stage(app, job, "Downloading the new version");
        match &p.branch {
            Some(b) => {
                let dst = format!("refs/remotes/origin/{b}");
                run_cmd(
                    app,
                    job,
                    tools::git()
                        .args([
                            "fetch",
                            "--depth",
                            "1",
                            "origin",
                            &format!("+refs/heads/{b}:{dst}"),
                        ])
                        .current_dir(&dir),
                )?;
                run_cmd(
                    app,
                    job,
                    tools::git()
                        .args(["checkout", "-B", b, &format!("origin/{b}")])
                        .current_dir(&dir),
                )?;
            }
            None => {
                // Pin to the same OID `ls-remote HEAD` reports so update checks stay in sync.
                let tip = crate::catalog::remote_head(p)
                    .ok_or_else(|| "could not resolve remote HEAD".to_string())?;
                run_cmd(
                    app,
                    job,
                    tools::git()
                        .args(["fetch", "--depth", "1", "origin", &tip])
                        .current_dir(&dir),
                )?;
                run_cmd(
                    app,
                    job,
                    tools::git()
                        .args(["reset", "--hard", "FETCH_HEAD"])
                        .current_dir(&dir),
                )?;
            }
        }
    } else {
        stage(app, job, "Downloading the project");
        let mut c = tools::git();
        c.args(["clone", "--depth", "1"]);
        if let Some(b) = &p.branch {
            c.args(["--branch", b]);
        }
        c.arg(&url).arg(&dir);
        run_cmd(app, job, &mut c)?;
    }
    stage(app, job, "Building (the first time takes several minutes)");
    let target = target_dir(&p.repo);
    let profile = detect_build_profile(&dir);
    let has_lockfile = dir.join("Cargo.lock").is_file();
    let mut cargo = tools::cargo();
    // Full diagnostics is the default victory feature (debug layer / DRED /
    // crash naming). Pass it explicitly so older checkouts without
    // `default = ["diagnostics"]` still get a diagnostics binary.
    cargo.args([
        "build",
        "--profile",
        profile,
        "-p",
        "victory",
        "--features",
        "diagnostics",
        "--target-dir",
    ]);
    cargo.arg(&target);
    if has_lockfile {
        cargo.arg("--locked");
    }
    cargo.current_dir(&dir);
    run_cmd(app, job, &mut cargo)?;
    if !built_exe(&p.repo).is_file() {
        return Err(format!(
            "the build made no {}",
            built_exe(&p.repo).display()
        ));
    }
    // Record the commit we actually built — must succeed or update checks stay wrong forever.
    let recorded = git_head(&dir)?;
    std::fs::create_dir_all(target_dir(&p.repo))
        .map_err(|e| format!("cannot write build marker dir: {e}"))?;
    std::fs::write(built_file(&p.repo), &recorded)
        .map_err(|e| format!("could not write built-from.txt: {e}"))?;
    push_log(app, job, format!("built from {recorded}"));
    Ok(())
}

pub fn uninstall(app: &AppHandle, job: &SharedJob, p: &Project) -> Result<(), String> {
    if p.local_exe.is_some() {
        return Err("this build was not installed by Ad3lHub".into());
    }
    stage(app, job, "Uninstalling");
    let ws = workspace();
    for dir in [
        repo_dir(&p.repo),
        target_dir(&p.repo),
        ws.join("cache").join(slug(&p.repo)),
    ] {
        if dir.exists() {
            push_log(app, job, format!("removing {}", dir.display()));
            std::fs::remove_dir_all(&dir)
                .map_err(|e| format!("could not remove {}: {e}", dir.display()))?;
        }
    }
    Ok(())
}

/// Prewarm the shader cache (`victory shaders …`), optionally host pipelines.
/// Skips when the AnyXboxX-style warm marker already matches (unless `always`).
pub fn warm_shaders(
    app: &AppHandle,
    job: &SharedJob,
    p: &Project,
    game_dir: &Path,
    env: &std::collections::BTreeMap<String, String>,
    always: bool,
    pipelines: bool,
) -> Result<(), String> {
    let exe = p
        .local_exe
        .as_ref()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| built_exe(&p.repo));
    if !exe.is_file() {
        return Err("this project is not installed yet: press Install".into());
    }
    let ws = workspace();
    let cache = ws.join("cache").join(slug(&p.repo));
    let _ = std::fs::create_dir_all(&cache);
    let archives = warm_state::warm_archives(game_dir)?;
    let mut warm_env = env.clone();
    warm_env.insert("VICTORY_CACHE_DIR".into(), cache.display().to_string());
    warm_env.insert(
        "VICTORY_DATA_DIR".into(),
        ws.join("data").join(slug(&p.repo)).display().to_string(),
    );

    let build = warm_state::build_id(&exe);
    let ids = warm_state::archive_identities(&archives);
    let fp = warm_state::fingerprint(&build, &warm_env, &ids);
    let marker = warm_state::marker_path(&cache);
    let current = warm_state::read_marker(&marker);
    let shaders_stale = warm_state::needs_warm(always, current.as_deref(), &fp);
    // Sibling of the shader cache folder (`…/shaders` → `…/pipelines`).
    let library = warm_state::pipeline_library_path(&cache);
    let need_pipelines = warm_state::need_pipeline_warm(pipelines, always, &library);
    if !shaders_stale && !need_pipelines {
        stage(app, job, "Shader cache already warm — skipping");
        push_log(app, job, format!("warm marker ok ({fp})"));
        return Ok(());
    }

    let dir = repo_dir(&p.repo);
    if shaders_stale {
        stage(
            app,
            job,
            "Warming shaders (first time takes several minutes — later launches skip this)",
        );
        let mut c = Command::new(&exe);
        c.arg("shaders");
        for a in &archives {
            c.arg(a);
        }
        for (k, v) in detect_launch_env(&dir) {
            c.env(&k, &v);
        }
        for (k, v) in &warm_env {
            c.env(k, v);
        }
        push_log(
            app,
            job,
            format!(
                "victory shaders ({} archives) → {}",
                archives.len(),
                cache.display()
            ),
        );
        run_cmd(app, job, &mut c)?;
    } else {
        push_log(
            app,
            job,
            format!("warm marker ok ({fp}); shaders skipped, host pipelines still needed"),
        );
    }

    if need_pipelines {
        stage(app, job, "Prebuilding host pipelines (GPU-specific, slow)");
        let shaders = game_dir.join("shaders.rpf");
        let dlls = [
            game_dir.join("XboxBvhBuilder.dll"),
            game_dir.join("amd_fidelityfx_upscaler_xbox.dll"),
        ];
        if shaders.is_file() {
            let mut t = Command::new(&exe);
            t.arg("translate").arg("--pipelines").arg(&shaders);
            let title = game_dir.join(EXE_NAME);
            if title.is_file() {
                t.arg(&title);
            }
            for d in &dlls {
                if d.is_file() {
                    t.arg(d);
                }
            }
            for (k, v) in detect_launch_env(&dir) {
                t.env(&k, &v);
            }
            for (k, v) in &warm_env {
                t.env(k, v);
            }
            run_cmd(app, job, &mut t)?;
        } else {
            push_log(
                app,
                job,
                format!(
                    "no shaders.rpf in {} — pipeline prebuild skipped",
                    game_dir.display()
                ),
            );
        }
    }

    if shaders_stale {
        warm_state::write_marker(&marker, &fp)
            .map_err(|e| format!("could not write warm marker: {e}"))?;
        push_log(app, job, format!("warm marker written ({fp})"));
    }
    stage(app, job, "Shaders warmed — ready to play");
    Ok(())
}

pub fn play(
    app: &AppHandle,
    job: &SharedJob,
    p: &Project,
    game_dir: &Path,
    env: &std::collections::BTreeMap<String, String>,
    fullscreen: bool,
    warm_before: bool,
    always_rewarm: bool,
    warm_pipelines: bool,
) -> Result<(), String> {
    let exe = p
        .local_exe
        .as_ref()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| built_exe(&p.repo));
    if !exe.is_file() {
        return Err("this project is not installed yet: press Install".into());
    }
    if warm_before {
        warm_shaders(app, job, p, game_dir, env, always_rewarm, warm_pipelines)?;
    }
    stage(app, job, "Playing");
    let ws = workspace();
    let mut c = Command::new(&exe);
    c.arg("run").arg(game_dir.join(EXE_NAME)).arg("--execute");
    let dir = repo_dir(&p.repo);
    for (k, v) in detect_launch_env(&dir) {
        c.env(&k, &v);
    }
    let console = env
        .get("VICTORY_CONSOLE")
        .map(String::as_str)
        .unwrap_or("series-s");
    if console.eq_ignore_ascii_case("series-s") || console.eq_ignore_ascii_case("series-x") {
        c.args(["--console", console]);
    }
    push_log(app, job, format!("== Ad3lHub flags ({})", env.len()));
    for (k, v) in env {
        c.env(k, v);
        push_log(app, job, format!("{k}={v}"));
    }
    let data = ws.join("data").join(slug(&p.repo));
    let _ = std::fs::create_dir_all(&data);
    c.env("VICTORY_DATA_DIR", &data);
    c.env("VICTORY_CACHE_DIR", ws.join("cache").join(slug(&p.repo)));
    run_with(app, job, &mut c, |pid| {
        if fullscreen {
            fullscreen::keep_borderless(pid);
        }
    })
}

pub fn install_tools(app: &AppHandle, job: &SharedJob) -> Result<(), String> {
    let t = tools::detect();
    if t.ready() {
        return Ok(());
    }
    if !t.winget {
        return Err("winget is not available; install Git, Rust, and C++ Build Tools manually".into());
    }
    let steps = tools::install_steps(&t);
    for (what, args) in steps {
        stage(app, job, &format!("Installing {what}"));
        let mut c = Command::new("winget");
        c.args(&args);
        run_cmd(app, job, &mut c)?;
    }
    tools::detect();
    Ok(())
}
