import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { useCallback, useEffect, useMemo, useState } from "react";
import logo from "./assets/logo.png";
import { AdvancedSettings } from "./components/AdvancedSettings";
import {
  IconAlert,
  IconCheck,
  IconDownload,
  IconFlame,
  IconFolder,
  IconGear,
  IconLibrary,
  IconPlay,
  IconRefresh,
  IconTerminal,
  IconUsers,
} from "./components/Icons";
import { Titlebar } from "./components/Titlebar";
import { UpdateNotice } from "./components/UpdateNotice";
import type {
  Catalog,
  FlagCatalog,
  FilesCheck,
  JobSnapshot,
  Project,
  RepoState,
  Settings,
  Tools,
} from "./types";

type Page = "library" | "community" | "settings" | "log";

const emptyJob: JobSnapshot = {
  running: false,
  repo: null,
  stage: "",
  log: [],
  failed: false,
};

export default function App() {
  const [page, setPage] = useState<Page>("library");
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
  const [flagCatalog, setFlagCatalog] = useState<FlagCatalog | null>(null);
  const [preview, setPreview] = useState<Record<string, string>>({});
  const [noticeRepo, setNoticeRepo] = useState<string | null>(null);
  const [ready, setReady] = useState(false);

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

  const refreshPreview = useCallback(async () => {
    setPreview(await invoke<Record<string, string>>("preview_env"));
  }, []);

  const persist = useCallback(
    async (next: Settings) => {
      const saved = await invoke<Settings>("save_settings_cmd", { settings: next });
      setSettings(saved);
      await refreshPreview();
      return saved;
    },
    [refreshPreview],
  );

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
        setFlagCatalog(await invoke<FlagCatalog>("flag_catalog"));
        await Promise.all([refreshCatalog(), refreshFiles(), refreshTools(), refreshPreview()]);
        setJob(await invoke<JobSnapshot>("get_job"));
        const u1 = await listen<JobSnapshot>("job", (e) => {
          const next = e.payload;
          setJob((prev) => {
            // After install/update finishes, re-probe so the toast clears.
            if (prev.running && !next.running && !next.failed && next.repo) {
              const repo = next.repo;
              void refreshCatalog().then((c) => {
                const all = [c.featured, ...c.community];
                const hit = all.find((p) => p.repo === repo);
                if (hit) void checkProject(hit);
              });
            }
            return next;
          });
        });
        const u2 = await listen("auto-check", () => {
          void refreshCatalog().then((c) => {
            const all = [c.featured, ...c.community];
            for (const p of all) void checkProject(p);
          });
        });
        const u3 = await listen<[string, RepoState]>("repo", (e) => {
          const [repo, st] = e.payload;
          setStatuses((prev) => ({ ...prev, [repo]: st }));
          void refreshCatalog().then((c) => {
            const all = [c.featured, ...c.community];
            const hit = all.find((p) => p.repo === repo);
            if (hit) void checkProject(hit);
          });
        });
        unsubs = [u1, u2, u3];
        setReady(true);
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e));
        setReady(true);
      }
    })();
    return () => {
      for (const u of unsubs) u();
    };
  }, [checkProject, refreshCatalog, refreshFiles, refreshPreview, refreshTools]);

  useEffect(() => {
    if (!catalog) return;
    const all = [catalog.featured, ...catalog.community];
    for (const p of all) void checkProject(p);
  }, [catalog, checkProject]);

  useEffect(() => {
    if (!catalog || job.running) return;
    const all = [catalog.featured, ...catalog.community];
    const hit = all.find((p) => updates[p.repo] && statuses[p.repo]?.installed);
    setNoticeRepo((prev) => {
      if (prev && updates[prev] && statuses[prev]?.installed) return prev;
      return hit?.repo ?? null;
    });
  }, [catalog, updates, statuses, job.running]);

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

  const installedCount = useMemo(
    () => Object.values(statuses).filter((s) => s.installed).length,
    [statuses],
  );
  const updateCount = Object.keys(updates).length;

  async function start(
    kind: "install" | "play" | "uninstall" | "installTools" | "warmShaders",
    project?: Project,
  ) {
    setError(null);
    try {
      await invoke("start_job", { kind, project: project ?? null });
      if (kind === "install" || kind === "play" || kind === "warmShaders") setPage("log");
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
    const s = await invoke<Settings>("set_install_folder", { dir, moveProjects: move });
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

  if (!ready || !settings || !catalog) {
    return (
      <div className="app">
        <Titlebar />
        <div className="boot">
          <img src={logo} alt="" />
          <div className="boot-copy">
            <strong>Ad3lHub</strong>
            <p>Starting…</p>
          </div>
          {error ? <p className="error">{error}</p> : null}
        </div>
      </div>
    );
  }

  const featured = catalog.featured;
  const featuredState = statuses[featured.repo];
  const featuredUpdate = updates[featured.repo];
  const toolsReady = !!(tools?.git && tools.cargo && tools.msvc);
  const canPlay = !!files?.ok && !!featuredState?.installed && !job.running;
  const noticeProject =
    noticeRepo == null
      ? null
      : [catalog.featured, ...catalog.community].find((p) => p.repo === noticeRepo) ?? null;

  return (
    <div className="app">
      <Titlebar />
      {noticeProject && !job.running ? (
        <UpdateNotice
          name={noticeProject.name}
          repo={noticeProject.repo}
          onLater={() => {
            const commit = updates[noticeProject.repo];
            if (commit) {
              void invoke("defer_update", { repo: noticeProject.repo, commit }).then(() =>
                checkProject(noticeProject),
              );
            }
            setNoticeRepo(null);
          }}
          onUpdate={() => {
            setNoticeRepo(null);
            void start("install", noticeProject);
          }}
        />
      ) : null}

      <div className="workspace">
        <aside className="rail">
          <div className="rail-brand">
            <img src={logo} alt="" className="rail-logo" />
            <div>
              <strong>Ad3lHub</strong>
              <span>Victory launcher</span>
            </div>
          </div>
          <nav className="rail-nav">
            <button
              type="button"
              className={`rail-link ${page === "library" ? "active" : ""}`}
              onClick={() => setPage("library")}
            >
              <IconLibrary />
              <span>Library</span>
            </button>
            <button
              type="button"
              className={`rail-link ${page === "community" ? "active" : ""}`}
              onClick={() => setPage("community")}
            >
              <IconUsers />
              <span>Community</span>
              {gpuFiltered.length > 0 ? (
                <em className="rail-count">{gpuFiltered.length}</em>
              ) : null}
            </button>
            <button
              type="button"
              className={`rail-link ${page === "settings" ? "active" : ""}`}
              onClick={() => setPage("settings")}
            >
              <IconGear />
              <span>Settings</span>
            </button>
            <button
              type="button"
              className={`rail-link ${page === "log" ? "active" : ""}`}
              onClick={() => setPage("log")}
            >
              <IconTerminal />
              <span>Activity</span>
              {job.running ? <em className="rail-dot" /> : null}
            </button>
          </nav>
          <div className="rail-foot">
            <div className="rail-meta">
              <span className={`pill ${toolsReady ? "good" : "warn"}`}>
                {toolsReady ? <IconCheck size={12} /> : <IconAlert size={12} />}
                Tools
              </span>
              {updateCount > 0 ? (
                <span className="pill warn">
                  <IconRefresh size={12} />
                  {updateCount}
                </span>
              ) : null}
            </div>
            <p className="hint stage">{job.stage || "Idle"}</p>
          </div>
        </aside>

        <main className="main">
          {page === "library" ? (
            <section className="page" key="library">
              <div className="page-head reveal" style={{ ["--d" as string]: "0ms" }}>
                <div>
                  <p className="eyebrow">Library</p>
                  <h1>Play the featured fork</h1>
                  <p>Install once, warm shaders, then launch with your game folder.</p>
                </div>
                <div className="actions">
                  <button type="button" className="btn ghost" onClick={() => void refreshCatalog()}>
                    <IconRefresh size={14} /> Refresh
                  </button>
                </div>
              </div>

              <div className="library-grid">
                <div className="library-main">
                  <article className="hero reveal" style={{ ["--d" as string]: "60ms" }}>
                    <div className="hero-visual" aria-hidden>
                      <img src={logo} alt="" />
                      <div className="hero-ring" />
                    </div>
                    <div className="hero-body">
                      <p className="kicker">Featured · NVIDIA 8GB</p>
                      <h2>{featured.name}</h2>
                      <p className="summary">{featured.summary}</p>
                      <div className="meta">
                        <span className="pill mono">{featured.repo}</span>
                        {featuredState?.installed ? (
                          <span className="pill good">
                            <IconCheck size={12} /> Installed
                          </span>
                        ) : (
                          <span className="pill">Not installed</span>
                        )}
                        {featuredUpdate ? (
                          <span className="pill warn">
                            <IconRefresh size={12} /> Update
                          </span>
                        ) : null}
                      </div>
                      <div className="actions hero-actions">
                        {featuredUpdate ? (
                          <>
                            <button
                              type="button"
                              className="btn"
                              disabled={job.running || !toolsReady}
                              onClick={() => void start("install", featured)}
                            >
                              <IconRefresh size={14} /> Update now
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
                            <IconDownload size={14} />
                            {featuredState?.installed ? "Reinstall" : "Install"}
                          </button>
                        )}
                        <button
                          type="button"
                          className="btn secondary"
                          disabled={!canPlay}
                          onClick={() => void start("play", featured)}
                        >
                          <IconPlay size={14} /> Play
                        </button>
                        <button
                          type="button"
                          className="btn secondary"
                          disabled={job.running || !featuredState?.installed || !files?.ok}
                          onClick={() => void start("warmShaders", featured)}
                        >
                          <IconFlame size={14} /> Warm
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
                  </article>

                  <div className="stat-strip reveal" style={{ ["--d" as string]: "120ms" }}>
                    <div className="stat">
                      <IconLibrary size={18} />
                      <div>
                        <b>{installedCount}</b>
                        <span>Installed</span>
                      </div>
                    </div>
                    <div className="stat">
                      <IconFolder size={18} />
                      <div>
                        <b>{files?.ok ? "Ready" : "Set folder"}</b>
                        <span>Game files</span>
                      </div>
                    </div>
                    <div className="stat">
                      <IconGear size={18} />
                      <div>
                        <b>{settings.preset}</b>
                        <span>Launch preset</span>
                      </div>
                    </div>
                    <div className="stat">
                      <IconFlame size={18} />
                      <div>
                        <b>{settings.warmBeforePlay ? "On" : "Off"}</b>
                        <span>Warm before play</span>
                      </div>
                    </div>
                  </div>

                  <div className="panel soft reveal" style={{ ["--d" as string]: "180ms" }}>
                    <div className="section-head">
                      <div>
                        <h3>
                          <IconUsers size={14} /> Community shortcuts
                        </h3>
                        <p className="hint">PortWatch builds live in Community — open that rail.</p>
                      </div>
                      <button type="button" className="btn ghost" onClick={() => setPage("community")}>
                        Browse →
                      </button>
                    </div>
                  </div>
                </div>

                <aside className="library-side">
                  <div className="panel soft reveal" style={{ ["--d" as string]: "90ms" }}>
                    <h3>
                      <IconFolder size={14} /> Game folder
                    </h3>
                    <p className="hint">Your title files — Ad3lHub never downloads the game.</p>
                    <label className="field">
                      Path
                      <input
                        type="text"
                        value={settings.gameDir}
                        onChange={(e) => setSettings({ ...settings, gameDir: e.target.value })}
                        onBlur={() => void persist(settings).then(() => refreshFiles())}
                        placeholder="Folder with game_scarlett_master.exe"
                      />
                    </label>
                    <div className="actions">
                      <button type="button" className="btn secondary" onClick={() => void browseGame()}>
                        Browse
                      </button>
                      <button type="button" className="btn secondary" onClick={() => void findGame()}>
                        Find on drives
                      </button>
                    </div>
                    {files ? (
                      <ul className="files">
                        {files.empty ? <li className="bad">Folder is empty</li> : null}
                        {files.files.map(([name, ok]) => (
                          <li key={name} className={ok ? "ok" : "bad"}>
                            {ok ? "✓" : "✗"} {name}
                          </li>
                        ))}
                      </ul>
                    ) : null}
                  </div>

                  <div className="panel soft reveal" style={{ ["--d" as string]: "150ms" }}>
                    <h3>Toolchain</h3>
                    <div className="tool-grid">
                      <span className={`tool ${tools?.git ? "on" : ""}`}>
                        {tools?.git ? <IconCheck size={12} /> : <IconAlert size={12} />}
                        Git
                      </span>
                      <span className={`tool ${tools?.cargo ? "on" : ""}`}>
                        {tools?.cargo ? <IconCheck size={12} /> : <IconAlert size={12} />}
                        Rust
                      </span>
                      <span className={`tool ${tools?.msvc ? "on" : ""}`}>
                        {tools?.msvc ? <IconCheck size={12} /> : <IconAlert size={12} />}
                        MSVC
                      </span>
                    </div>
                    {!toolsReady ? (
                      <button
                        type="button"
                        className="btn"
                        disabled={job.running || !tools?.winget}
                        onClick={() => void start("installTools")}
                      >
                        Install missing tools
                      </button>
                    ) : (
                      <p className="hint">Ready to clone and build.</p>
                    )}
                    <button
                      type="button"
                      className="btn ghost"
                      onClick={() => setPage("settings")}
                    >
                      <IconGear size={14} /> Launch flags & presets
                    </button>
                  </div>
                </aside>
              </div>
            </section>
          ) : null}

          {page === "community" ? (
            <section className="page" key="community">
              <div className="page-head reveal" style={{ ["--d" as string]: "0ms" }}>
                <div>
                  <p className="eyebrow">Community</p>
                  <h1>PortWatch projects</h1>
                  <p>Everything after the featured NVIDIA fork.</p>
                </div>
                <label className="field filter">
                  GPU
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
              <div className="project-grid">
                {gpuFiltered.map((p, i) => {
                  const st = statuses[p.repo];
                  const upd = updates[p.repo];
                  const active = selected === p.repo;
                  return (
                    <div
                      key={p.repo}
                      className={`project-card reveal ${active ? "active" : ""}`}
                      style={{ ["--d" as string]: `${40 + i * 35}ms` }}
                      onClick={() => {
                        setSelected(p.repo);
                        void persist({ ...settings, selected: p.repo });
                      }}
                    >
                      <div className="project-mark">
                        <IconUsers size={18} />
                      </div>
                      <div className="project-body">
                        <h4>{p.name}</h4>
                        <p className="mono">{p.repo}</p>
                        <div className="meta">
                          {p.verified ? <span className="pill good">Verified</span> : null}
                          {p.fps != null ? <span className="pill">{p.fps.toFixed(1)} fps</span> : null}
                          {st?.installed ? (
                            <span className="pill good">
                              <IconCheck size={12} /> Installed
                            </span>
                          ) : null}
                          {upd ? (
                            <span className="pill warn">
                              <IconRefresh size={12} /> Update
                            </span>
                          ) : null}
                        </div>
                      </div>
                      <div className="actions" onClick={(e) => e.stopPropagation()}>
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
                          <IconPlay size={14} /> Play
                        </button>
                      </div>
                    </div>
                  );
                })}
              </div>
              <div className="panel soft reveal" style={{ ["--d" as string]: "200ms" }}>
                <h3>Add GitHub repo</h3>
                <div className="add-row">
                  <label className="field grow">
                    Link
                    <input
                      type="text"
                      value={customLink}
                      onChange={(e) => setCustomLink(e.target.value)}
                      placeholder="owner/name"
                    />
                  </label>
                  <button type="button" className="btn secondary" onClick={() => void addCustom()}>
                    Add
                  </button>
                </div>
              </div>
            </section>
          ) : null}

          {page === "settings" ? (
            <section className="page" key="settings">
              <div className="page-head reveal" style={{ ["--d" as string]: "0ms" }}>
                <div>
                  <p className="eyebrow">Settings</p>
                  <h1>Launch profile</h1>
                  <p>Presets and curated VICTORY_* flags — more than Mandyhub.</p>
                </div>
              </div>
              <div className="settings-grid">
                <div className="settings-main reveal" style={{ ["--d" as string]: "60ms" }}>
                  {flagCatalog ? (
                    <AdvancedSettings
                      settings={settings}
                      catalog={flagCatalog}
                      preview={preview}
                      onChange={setSettings}
                      onPersist={(next) => void persist(next)}
                      onPreset={(id) => {
                        void invoke<Settings>("apply_preset", { preset: id }).then(async (s) => {
                          setSettings(s);
                          await refreshPreview();
                        });
                      }}
                    />
                  ) : null}
                </div>
                <aside className="settings-side reveal" style={{ ["--d" as string]: "120ms" }}>
                  <div className="panel soft">
                    <h3>Install folder</h3>
                    <p className="hint mono">
                      {settings.installDir || "%LOCALAPPDATA%\\Ad3lHub"}
                    </p>
                    <button type="button" className="btn secondary" onClick={() => void browseInstall()}>
                      Choose folder…
                    </button>
                  </div>
                </aside>
              </div>
            </section>
          ) : null}

          {page === "log" ? (
            <section className="page" key="log">
              <div className="page-head reveal" style={{ ["--d" as string]: "0ms" }}>
                <div>
                  <p className="eyebrow">Activity</p>
                  <h1>{job.stage || "No job running"}</h1>
                  <p>Install, warm, and play output lands here.</p>
                </div>
              </div>
              {error ? <p className="error">{error}</p> : null}
              <pre className="log reveal" style={{ ["--d" as string]: "80ms" }}>
                {job.log.length
                  ? job.log.slice(-300).join("\n")
                  : "Install, warm, or play to see output here."}
              </pre>
            </section>
          ) : null}
        </main>
      </div>
    </div>
  );
}
