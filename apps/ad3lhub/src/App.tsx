import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import logo from "./assets/logo.png";
import { AdvancedSettings } from "./components/AdvancedSettings";
import { ConfirmModal } from "./components/ConfirmModal";
import {
  IconAlert,
  IconCheck,
  IconDownload,
  IconFlame,
  IconFolder,
  IconGear,
  IconPlay,
  IconRefresh,
  IconTerminal,
  IconUsers,
} from "./components/Icons";
import { Modal } from "./components/Modal";
import { Titlebar } from "./components/Titlebar";
import { UpdateNotice } from "./components/UpdateNotice";
import { formatError } from "./lib/errors";
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

const emptyJob: JobSnapshot = {
  running: false,
  repo: null,
  stage: "",
  log: [],
  failed: false,
};

const featuredFallback: Project = {
  name: "Victory NVIDIA 8 GB",
  repo: "ad3lre/victory-nvidia-8gb",
  branch: "main",
  gpu: ["NVIDIA"],
  fps: null,
  render: null,
  verified: true,
  summary:
    "Ad3l's NVIDIA 8 GB fork — telemetry, Agility SDK 620, Series S pareto-residency. Featured in Ad3lHub.",
  custom: false,
  localExe: null,
  featured: true,
};

type ModalKind = "settings" | "activity" | "addRepo" | null;

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
  const [flagCatalog, setFlagCatalog] = useState<FlagCatalog | null>(null);
  const [preview, setPreview] = useState<Record<string, string>>({});
  const [noticeRepo, setNoticeRepo] = useState<string | null>(null);
  const [ready, setReady] = useState(false);
  const [modal, setModal] = useState<ModalKind>(null);
  const [confirmUninstall, setConfirmUninstall] = useState<Project | null>(null);
  const [searchingGame, setSearchingGame] = useState(false);
  const autoOpenedJob = useRef(false);
  const checking = useRef(new Set<string>());

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
    if (checking.current.has(project.repo)) return;
    checking.current.add(project.repo);
    try {
      const st = await invoke<RepoState>("check_repo", { project });
      setStatuses((prev) => ({ ...prev, [project.repo]: st }));
      const upd = await invoke<string | null>("update_available", { project });
      setUpdates((prev) => {
        const copy = { ...prev };
        if (upd) copy[project.repo] = upd;
        else delete copy[project.repo];
        return copy;
      });
    } catch (e) {
      setError(formatError(e));
    } finally {
      checking.current.delete(project.repo);
    }
  }, []);

  useEffect(() => {
    let unsubs: Array<() => void> = [];
    let cancelled = false;
    (async () => {
      try {
        const s = await invoke<Settings>("get_settings");
        if (cancelled) return;
        setSettings(s);
        setSelected(s.selected);
        // Show home immediately with featured fallback — PortWatch / tools load in parallel.
        setCatalog({
          featured: featuredFallback,
          community: s.custom ?? [],
          error: null,
        });
        setReady(true);

        const [flags] = await Promise.all([
          invoke<FlagCatalog>("flag_catalog"),
          refreshFiles().catch((e) => {
            setError(formatError(e));
            return null;
          }),
          refreshTools().catch((e) => {
            setError(formatError(e));
            return null;
          }),
          refreshPreview().catch(() => undefined),
          refreshCatalog().catch((e) => {
            setError(formatError(e));
            return null;
          }),
        ]);
        if (cancelled) return;
        setFlagCatalog(flags);
        setJob(await invoke<JobSnapshot>("get_job"));

        const u1 = await listen<JobSnapshot>("job", (e) => {
          const next = e.payload;
          setJob((prev) => {
            if (prev.running && !next.running && !next.failed && next.repo) {
              const repo = next.repo;
              void refreshCatalog().then((c) => {
                const all = [c.featured, ...c.community];
                const hit = all.find((p) => p.repo === repo);
                if (hit) void checkProject(hit);
              });
              void refreshTools();
            }
            return next;
          });
          if (next.running && !autoOpenedJob.current) {
            autoOpenedJob.current = true;
            setModal("activity");
          }
          if (!next.running) autoOpenedJob.current = false;
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
      } catch (e) {
        setError(formatError(e));
        setReady(true);
      }
    })();
    return () => {
      cancelled = true;
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

  async function start(
    kind: "install" | "play" | "uninstall" | "installTools" | "warmShaders",
    project?: Project,
  ) {
    setError(null);
    try {
      await invoke("start_job", { kind, project: project ?? null });
      setModal("activity");
      autoOpenedJob.current = true;
    } catch (e) {
      setError(formatError(e));
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
    try {
      const move = window.confirm("Move existing installed projects to this folder?");
      const s = await invoke<Settings>("set_install_folder", { dir, moveProjects: move });
      setSettings(s);
    } catch (e) {
      setError(formatError(e));
    }
  }

  async function findGame() {
    setSearchingGame(true);
    setError(null);
    try {
      const found = await invoke<string>("search_game_folder");
      if (!settings) return;
      await persist({ ...settings, gameDir: found });
      await refreshFiles();
    } catch (e) {
      setError(formatError(e));
    } finally {
      setSearchingGame(false);
    }
  }

  async function addCustom() {
    try {
      await invoke<Project>("add_custom_repo", { link: customLink });
      setCustomLink("");
      setModal(null);
      await refreshCatalog();
    } catch (e) {
      setError(formatError(e));
    }
  }

  if (!ready || !settings) {
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

  const featured = catalog?.featured ?? featuredFallback;
  const featuredState = statuses[featured.repo];
  const featuredUpdate = updates[featured.repo];
  const toolsReady = !!(tools?.git && tools.cargo && tools.msvc);
  const canPlay = !!files?.ok && !!featuredState?.installed && !job.running;
  const noticeProject =
    noticeRepo == null || !catalog
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

      <main className="home">
        <header className="home-top reveal" style={{ ["--d" as string]: "0ms" }}>
          <div className="home-brand">
            <img src={logo} alt="" className="home-logo" />
            <div>
              <h1 className="wordmark">Ad3lHub</h1>
              <p className="tagline">Install, update, and play Victory forks.</p>
            </div>
          </div>
          <div className="home-actions">
            <button
              type="button"
              className="btn ghost icon-btn"
              title="Activity"
              onClick={() => setModal("activity")}
            >
              <IconTerminal size={16} />
              {job.running ? <em className="job-dot" /> : null}
            </button>
            <button
              type="button"
              className="btn ghost icon-btn"
              title="Settings"
              onClick={() => setModal("settings")}
            >
              <IconGear size={16} />
            </button>
            <button
              type="button"
              className="btn ghost"
              onClick={() => void refreshCatalog().catch((e) => setError(formatError(e)))}
            >
              <IconRefresh size={14} /> Refresh
            </button>
          </div>
        </header>

        {error ? (
          <p className="error banner-error" role="alert">
            {error}
            <button type="button" className="btn ghost" onClick={() => setError(null)}>
              Dismiss
            </button>
          </p>
        ) : null}

        <article className="hero reveal" style={{ ["--d" as string]: "40ms" }}>
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
              {job.running && job.repo === featured.repo ? (
                <span className="pill warn">{job.stage || "Working…"}</span>
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
                  onClick={() => setConfirmUninstall(featured)}
                >
                  Uninstall
                </button>
              ) : null}
            </div>
            {!toolsReady ? (
              <p className="hint">
                Install Git, Rust, and MSVC before building — use the tools strip below.
              </p>
            ) : null}
          </div>
        </article>

        <section className="strip reveal" style={{ ["--d" as string]: "90ms" }}>
          <div className="strip-block">
            <h3>
              <IconFolder size={14} /> Game folder
            </h3>
            <p className="hint">Ad3lHub never downloads the game — point at your title files.</p>
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
              <button
                type="button"
                className="btn secondary"
                disabled={searchingGame}
                onClick={() => void findGame()}
              >
                {searchingGame ? "Searching…" : "Find on drives"}
              </button>
            </div>
            {files ? (
              <ul className="files compact">
                {files.empty ? <li className="bad">Folder is empty</li> : null}
                {files.files.map(([name, ok]) => (
                  <li key={name} className={ok ? "ok" : "bad"}>
                    {ok ? "✓" : "✗"} {name}
                  </li>
                ))}
              </ul>
            ) : null}
          </div>

          <div className="strip-block">
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
            <p className="hint stage">{job.stage || "Idle"}</p>
          </div>
        </section>

        <section className="community reveal" style={{ ["--d" as string]: "130ms" }}>
          <div className="section-head">
            <div>
              <h3>
                <IconUsers size={14} /> Community projects
              </h3>
              <p className="hint">PortWatch builds after the featured NVIDIA fork.</p>
            </div>
            <div className="actions">
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
              <button type="button" className="btn secondary" onClick={() => setModal("addRepo")}>
                Add repo
              </button>
            </div>
          </div>
          {catalog?.error ? <p className="error">{catalog.error}</p> : null}
          <div className="project-list">
            {gpuFiltered.length === 0 ? (
              <p className="hint">No community projects yet — refresh or add a GitHub repo.</p>
            ) : (
              gpuFiltered.map((p) => {
                const st = statuses[p.repo];
                const upd = updates[p.repo];
                const active = selected === p.repo;
                return (
                  <div
                    key={p.repo}
                    className={`project-row ${active ? "active" : ""}`}
                    onClick={() => {
                      setSelected(p.repo);
                      void persist({ ...settings, selected: p.repo });
                    }}
                  >
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
                      {st?.installed ? (
                        <button
                          type="button"
                          className="btn danger"
                          disabled={job.running}
                          onClick={() => setConfirmUninstall(p)}
                        >
                          Uninstall
                        </button>
                      ) : null}
                    </div>
                  </div>
                );
              })
            )}
          </div>
        </section>
      </main>

      <Modal open={modal === "settings"} title="Launch profile" onClose={() => setModal(null)} wide>
        <div className="settings-modal">
          {flagCatalog ? (
            <AdvancedSettings
              settings={settings}
              catalog={flagCatalog}
              preview={preview}
              onChange={setSettings}
              onPersist={(next) => void persist(next)}
              onPreset={(id) => {
                void invoke<Settings>("apply_preset", { preset: id })
                  .then(async (s) => {
                    setSettings(s);
                    await refreshPreview();
                  })
                  .catch((e) => setError(formatError(e)));
              }}
            />
          ) : (
            <p className="hint">Loading flags…</p>
          )}
          <div className="panel soft">
            <h3>Install folder</h3>
            <p className="hint mono">{settings.installDir || "%LOCALAPPDATA%\\Ad3lHub"}</p>
            <button type="button" className="btn secondary" onClick={() => void browseInstall()}>
              Choose folder…
            </button>
          </div>
        </div>
      </Modal>

      <Modal
        open={modal === "activity"}
        title={job.stage || "Activity"}
        onClose={() => setModal(null)}
        wide
      >
        {job.failed ? <p className="error">Last job failed — see the log.</p> : null}
        <pre className="log">
          {job.log.length
            ? job.log.slice(-400).join("\n")
            : "Install, warm, or play to see output here."}
        </pre>
      </Modal>

      <Modal
        open={modal === "addRepo"}
        title="Add GitHub repo"
        onClose={() => setModal(null)}
        footer={
          <div className="actions">
            <button type="button" className="btn secondary" onClick={() => setModal(null)}>
              Cancel
            </button>
            <button type="button" className="btn" onClick={() => void addCustom()}>
              Add
            </button>
          </div>
        }
      >
        <label className="field">
          Link
          <input
            type="text"
            value={customLink}
            onChange={(e) => setCustomLink(e.target.value)}
            placeholder="owner/name or https://github.com/owner/name"
            onKeyDown={(e) => {
              if (e.key === "Enter") void addCustom();
            }}
          />
        </label>
      </Modal>

      <ConfirmModal
        open={!!confirmUninstall}
        title="Uninstall project"
        body={
          confirmUninstall
            ? `Remove ${confirmUninstall.name} (${confirmUninstall.repo})? Saves in the data folder are kept.`
            : ""
        }
        confirmLabel="Uninstall"
        danger
        onCancel={() => setConfirmUninstall(null)}
        onConfirm={() => {
          const p = confirmUninstall;
          setConfirmUninstall(null);
          if (p) void start("uninstall", p);
        }}
      />
    </div>
  );
}
