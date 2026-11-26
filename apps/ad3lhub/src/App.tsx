import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { useCallback, useEffect, useMemo, useState } from "react";
import { Titlebar } from "./components/Titlebar";
import type {
  Catalog,
  FilesCheck,
  JobSnapshot,
  Project,
  RepoState,
  Settings,
  Tools,
} from "./types";

const emptyJob: JobSnapshot = {
  running: false,
  repo: null,
  stage: "",
  log: [],
  failed: false,
};

export default function App() {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [catalog, setCatalog] = useState<Catalog | null>(null);
  const [tools, setTools] = useState<Tools | null>(null);
  const [files, setFiles] = useState<FilesCheck | null>(null);
  const [job, setJob] = useState<JobSnapshot>(emptyJob);
  const [statuses, setStatuses] = useState<Record<string, RepoState>>({});
  const [updates, setUpdates] = useState<Record<string, string>>({});
  const [customLink, setCustomLink] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState<string | null>(null);

  const refreshCatalog = useCallback(async () => {
    const c = await invoke<Catalog>("fetch_catalog");
    setCatalog(c);
    return c;
  }, []);

  const refreshFiles = useCallback(async () => {
    const f = await invoke<FilesCheck>("check_game_files");
    setFiles(f);
    return f;
  }, []);

  const refreshTools = useCallback(async () => {
    const t = await invoke<Tools>("detect_tools");
    setTools(t);
    return t;
  }, []);

  const persist = useCallback(async (next: Settings) => {
    const saved = await invoke<Settings>("save_settings_cmd", { settings: next });
    setSettings(saved);
    return saved;
  }, []);

  const checkProject = useCallback(async (project: Project) => {
    const st = await invoke<RepoState>("check_repo", { project });
    setStatuses((prev) => ({ ...prev, [project.repo]: st }));
    const upd = await invoke<string | null>("update_available", { project });
    setUpdates((prev) => {
      const copy = { ...prev };
      if (upd) copy[project.repo] = upd;
      else delete copy[project.repo];
      return copy;
    });
  }, []);

  useEffect(() => {
    let unsubs: Array<() => void> = [];
    (async () => {
      try {
        const s = await invoke<Settings>("get_settings");
        setSettings(s);
        setSelected(s.selected);
        await Promise.all([refreshCatalog(), refreshFiles(), refreshTools()]);
        setJob(await invoke<JobSnapshot>("get_job"));
        const u1 = await listen<JobSnapshot>("job", (e) => setJob(e.payload));
        const u2 = await listen("auto-check", () => {
          void refreshCatalog().then((c) => {
            const all = [c.featured, ...c.community];
            for (const p of all) void checkProject(p);
          });
        });
        unsubs = [u1, u2];
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e));
      }
    })();
    return () => {
      for (const u of unsubs) u();
    };
  }, [checkProject, refreshCatalog, refreshFiles, refreshTools]);

  useEffect(() => {
    if (!catalog) return;
    const all = [catalog.featured, ...catalog.community];
    for (const p of all) void checkProject(p);
  }, [catalog, checkProject]);

  const gpuFiltered = useMemo(() => {
    if (!catalog || !settings) return [];
    const filter = settings.gpuFilter;
    if (filter === 0) return catalog.community;
    const labels = ["", "NVIDIA", "AMD", "Intel"];
    const want = labels[filter];
    return catalog.community.filter(
      (p) => p.gpu.length === 0 || p.gpu.some((g) => g.toUpperCase().includes(want)),
    );
  }, [catalog, settings]);

  async function start(kind: "install" | "play" | "uninstall" | "installTools", project?: Project) {
    setError(null);
    try {
      await invoke("start_job", { kind, project: project ?? null });
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }

  async function browseGame() {
    const dir = await open({ directory: true, multiple: false, title: "Game folder" });
    if (!dir || !settings || Array.isArray(dir)) return;
    await persist({ ...settings, gameDir: dir });
    await refreshFiles();
  }

  async function browseInstall() {
    const dir = await open({ directory: true, multiple: false, title: "Install folder" });
    if (!dir || Array.isArray(dir)) return;
    const move = window.confirm("Move existing installed projects to this folder?");
    const s = await invoke<Settings>("set_install_folder", {
      dir,
      moveProjects: move,
    });
    setSettings(s);
  }

  async function findGame() {
    try {
      const found = await invoke<string>("search_game_folder");
      if (!settings) return;
      await persist({ ...settings, gameDir: found });
      await refreshFiles();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }

  async function addCustom() {
    try {
      await invoke<Project>("add_custom_repo", { link: customLink });
      setCustomLink("");
      await refreshCatalog();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }

  if (!settings || !catalog) {
    return (
      <div className="app">
        <Titlebar />
        <main className="shell">
          <p className="stage">Loading Ad3lHub…</p>
          {error ? <p className="error">{error}</p> : null}
        </main>
      </div>
    );
  }

  const featured = catalog.featured;
  const featuredState = statuses[featured.repo];
  const featuredUpdate = updates[featured.repo];
  const toolsReady = !!(tools?.git && tools.cargo && tools.msvc);
  const canPlay = !!files?.ok && !!featuredState?.installed && !job.running;

  return (
    <div className="app">
      <Titlebar />
      <main className="shell">
        <section className="hero">
          <h1 className="brand">Ad3lHub</h1>
          <p className="lede">
            Launch VIctory builds on your PC. Our NVIDIA 8 GB fork is first; community projects
            stay one click away.
          </p>
        </section>

        <section className="featured">
          <div className="featured-top">
            <div>
              <p className="kicker">Featured</p>
              <h2>{featured.name}</h2>
              <p className="summary">{featured.summary}</p>
              <div className="meta">
                <span className="pill">{featured.repo}</span>
                {featuredState?.installed ? (
                  <span className="pill good">Installed</span>
                ) : (
                  <span className="pill">Not installed</span>
                )}
                {featuredUpdate ? <span className="pill warn">Update available</span> : null}
                {featured.gpu.map((g) => (
                  <span className="pill" key={g}>
                    {g}
                  </span>
                ))}
              </div>
            </div>
            <div className="actions">
              {featuredUpdate ? (
                <>
                  <button
                    type="button"
                    className="btn"
                    disabled={job.running || !toolsReady}
                    onClick={() => void start("install", featured)}
                  >
                    Update now
                  </button>
                  <button
                    type="button"
                    className="btn secondary"
                    onClick={() =>
                      void invoke("defer_update", {
                        repo: featured.repo,
                        commit: featuredUpdate,
                      }).then(() => checkProject(featured))
                    }
                  >
                    Later
                  </button>
                </>
              ) : (
                <button
                  type="button"
                  className="btn"
                  disabled={job.running || !toolsReady}
                  onClick={() => void start("install", featured)}
                >
                  {featuredState?.installed ? "Reinstall" : "Install"}
                </button>
              )}
              <button
                type="button"
                className="btn secondary"
                disabled={!canPlay}
                onClick={() => void start("play", featured)}
              >
                Play
              </button>
              {featuredState?.installed ? (
                <button
                  type="button"
                  className="btn danger"
                  disabled={job.running}
                  onClick={() => {
                    if (window.confirm(`Uninstall ${featured.name}?`)) {
                      void start("uninstall", featured);
                    }
                  }}
                >
                  Uninstall
                </button>
              ) : null}
            </div>
          </div>

          <div className="panel">
            <div className="grid-2">
              <label className="field">
                Game folder
                <input
                  type="text"
                  value={settings.gameDir}
                  onChange={(e) => setSettings({ ...settings, gameDir: e.target.value })}
                  onBlur={() => void persist(settings).then(() => refreshFiles())}
                  placeholder="Folder with your own game files"
                />
              </label>
              <div className="actions" style={{ alignSelf: "end" }}>
                <button type="button" className="btn secondary" onClick={() => void browseGame()}>
                  Browse
                </button>
                <button type="button" className="btn secondary" onClick={() => void findGame()}>
                  Find on drives
                </button>
              </div>
            </div>
            {files ? (
              <ul className="files">
                {files.empty ? <li className="bad">Folder is empty</li> : null}
                {files.files.map(([name, ok]) => (
                  <li key={name} className={ok ? "ok" : "bad"}>
                    {ok ? "✓" : "✗"} {name}
                  </li>
                ))}
                {!files.ok ? (
                  <li className="bad">Play stays off until every required file is present.</li>
                ) : null}
              </ul>
            ) : null}
          </div>
        </section>

        <section className="section">
          <div className="section-head">
            <div>
              <h3>Community projects</h3>
              <p className="hint">From PortWatch, after our featured fork.</p>
            </div>
            <label className="field" style={{ width: 160 }}>
              GPU filter
              <select
                value={settings.gpuFilter}
                onChange={(e) => {
                  const next = { ...settings, gpuFilter: Number(e.target.value) };
                  setSettings(next);
                  void persist(next);
                }}
              >
                <option value={0}>All</option>
                <option value={1}>NVIDIA</option>
                <option value={2}>AMD</option>
                <option value={3}>Intel</option>
              </select>
            </label>
          </div>
          {catalog.error ? <p className="error">{catalog.error}</p> : null}
          <div className="list">
            {gpuFiltered.map((p) => {
              const st = statuses[p.repo];
              const upd = updates[p.repo];
              const active = selected === p.repo;
              return (
                <div
                  key={p.repo}
                  className="row"
                  style={active ? { borderColor: "rgb(255 90 31 / 0.45)" } : undefined}
                >
                  <div className="row-main">
                    <h4>{p.name}</h4>
                    <p>
                      {p.repo}
                      {p.verified ? " · verified" : ""}
                      {p.fps != null ? ` · ${p.fps.toFixed(1)} fps` : ""}
                      {st?.installed ? " · installed" : ""}
                      {upd ? " · update" : ""}
                    </p>
                  </div>
                  <div className="actions">
                    <button
                      type="button"
                      className="btn secondary"
                      onClick={() => {
                        setSelected(p.repo);
                        void persist({ ...settings, selected: p.repo });
                      }}
                    >
                      Select
                    </button>
                    <button
                      type="button"
                      className="btn secondary"
                      disabled={job.running || !toolsReady}
                      onClick={() => void start("install", p)}
                    >
                      {upd ? "Update" : st?.installed ? "Reinstall" : "Install"}
                    </button>
                    <button
                      type="button"
                      className="btn"
                      disabled={job.running || !files?.ok || !st?.installed}
                      onClick={() => void start("play", p)}
                    >
                      Play
                    </button>
                  </div>
                </div>
              );
            })}
          </div>
          <div className="panel">
            <div className="grid-2">
              <label className="field">
                Add GitHub repo
                <input
                  type="text"
                  value={customLink}
                  onChange={(e) => setCustomLink(e.target.value)}
                  placeholder="owner/name or https://github.com/owner/name"
                />
              </label>
              <div className="actions" style={{ alignSelf: "end" }}>
                <button type="button" className="btn secondary" onClick={() => void addCustom()}>
                  Add
                </button>
                <button
                  type="button"
                  className="btn secondary"
                  onClick={() => void refreshCatalog()}
                >
                  Refresh list
                </button>
              </div>
            </div>
          </div>
        </section>

        <section className="section">
          <div className="section-head">
            <h3>Settings & tools</h3>
            <p className="hint stage">{job.stage || "Idle"}</p>
          </div>
          <div className="panel">
            <div className="grid-2">
              <label className="check">
                <input
                  type="checkbox"
                  checked={settings.seriesS}
                  onChange={(e) => {
                    const next = { ...settings, seriesS: e.target.checked };
                    setSettings(next);
                    void persist(next);
                  }}
                />
                Xbox Series S profile
              </label>
              <label className="field">
                Render scale
                <input
                  type="number"
                  min={0.25}
                  max={1}
                  step={0.05}
                  value={settings.renderScale}
                  onChange={(e) =>
                    setSettings({ ...settings, renderScale: Number(e.target.value) || 0.5 })
                  }
                  onBlur={() => void persist(settings)}
                />
              </label>
              <label className="check">
                <input
                  type="checkbox"
                  checked={settings.fix8gb}
                  onChange={(e) => {
                    const next = { ...settings, fix8gb: e.target.checked };
                    setSettings(next);
                    void persist(next);
                  }}
                />
                8 GB graphics card fix
              </label>
              <label className="check">
                <input
                  type="checkbox"
                  checked={settings.brightShadows}
                  onChange={(e) => {
                    const next = { ...settings, brightShadows: e.target.checked };
                    setSettings(next);
                    void persist(next);
                  }}
                />
                Brighter shadows
              </label>
              <label className="check">
                <input
                  type="checkbox"
                  checked={settings.fullscreen}
                  onChange={(e) => {
                    const next = { ...settings, fullscreen: e.target.checked };
                    setSettings(next);
                    void persist(next);
                  }}
                />
                Borderless fullscreen
              </label>
            </div>
            <label className="field">
              Extra VICTORY_* environment lines
              <textarea
                value={settings.extraEnv}
                onChange={(e) => setSettings({ ...settings, extraEnv: e.target.value })}
                onBlur={() => void persist(settings)}
                placeholder={"VICTORY_FOO=1\nVICTORY_BAR=2"}
              />
            </label>
            <div className="actions">
              <button type="button" className="btn secondary" onClick={() => void browseInstall()}>
                Install folder…
              </button>
              <span className="hint">
                {settings.installDir || "Default %LOCALAPPDATA%\\Ad3lHub"}
              </span>
            </div>
            <div className="meta">
              <span className={`pill ${tools?.git ? "good" : "warn"}`}>
                Git {tools?.git ? "ready" : "missing"}
              </span>
              <span className={`pill ${tools?.cargo ? "good" : "warn"}`}>
                Rust {tools?.cargo ? "ready" : "missing"}
              </span>
              <span className={`pill ${tools?.msvc ? "good" : "warn"}`}>
                C++ tools {tools?.msvc ? "ready" : "missing"}
              </span>
            </div>
            {!toolsReady ? (
              <button
                type="button"
                className="btn"
                disabled={job.running || !tools?.winget}
                onClick={() => void start("installTools")}
              >
                Install missing tools for me
              </button>
            ) : null}
          </div>
          {error ? <p className="error">{error}</p> : null}
          {job.log.length > 0 ? <pre className="log">{job.log.slice(-200).join("\n")}</pre> : null}
        </section>
      </main>
    </div>
  );
}
