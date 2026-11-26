import type { FlagCatalog, FlagDef, Settings } from "../types";

type Props = {
  settings: Settings;
  catalog: FlagCatalog;
  preview: Record<string, string>;
  onChange: (next: Settings) => void;
  onPreset: (id: string) => void;
  onPersist: (next: Settings) => void;
};

function flagValue(settings: Settings, def: FlagDef): string {
  if (Object.prototype.hasOwnProperty.call(settings.flags, def.key)) {
    return settings.flags[def.key] ?? "";
  }
  return def.default;
}

export function AdvancedSettings({
  settings,
  catalog,
  preview,
  onChange,
  onPreset,
  onPersist,
}: Props) {
  const categories = Array.from(new Set(catalog.flags.map((f) => f.category)));

  function setFlag(key: string, value: string) {
    const flags = { ...settings.flags, [key]: value };
    if (value === "") delete flags[key];
    const next: Settings = {
      ...settings,
      preset: "custom",
      flags,
    };
    if (key === "CONSOLE") next.seriesS = value === "series-s";
    if (key === "RENDER_SCALE") {
      const n = Number(value);
      if (Number.isFinite(n) && n > 0) next.renderScale = n;
    }
    onChange(next);
  }

  function commit(next: Settings = settings) {
    onPersist(next);
  }

  return (
    <div className="panel advanced">
      <div className="section-head">
        <div>
          <h3>Launch profile</h3>
          <p className="hint">
            Presets from this NVIDIA fork’s scripts — more knobs than Mandyhub for people who know
            the flags.
          </p>
        </div>
        <label className="check">
          <input
            type="checkbox"
            checked={settings.showExpert}
            onChange={(e) => {
              const next = { ...settings, showExpert: e.target.checked };
              onChange(next);
              commit(next);
            }}
          />
          Expert flags
        </label>
      </div>

      <div className="preset-grid">
        {catalog.presets.map((p) => (
          <button
            key={p.id}
            type="button"
            className={`preset ${settings.preset === p.id ? "active" : ""}`}
            title={p.help}
            onClick={() => onPreset(p.id)}
          >
            <strong>{p.label}</strong>
            <span>{p.help}</span>
          </button>
        ))}
      </div>

      {categories.map((cat) => {
        const flags = catalog.flags.filter(
          (f) => f.category === cat && (settings.showExpert || !f.expert),
        );
        if (flags.length === 0) return null;
        return (
          <div key={cat} className="flag-group">
            <h4>{cat}</h4>
            <div className="flag-list">
              {flags.map((f) => (
                <FlagControl
                  key={f.key}
                  def={f}
                  value={flagValue(settings, f)}
                  onChange={(v) => setFlag(f.key, v)}
                  onBlur={() => commit()}
                />
              ))}
            </div>
          </div>
        );
      })}

      <div className="flag-group">
        <h4>Shader warm (faster later launches)</h4>
        <p className="hint">
          Same trick as AnyXboxX: run <code>victory shaders</code> on the game’s .rpf archives before
          Play, then skip when the warm marker still matches this build and flags.
        </p>
        <label className="check">
          <input
            type="checkbox"
            checked={settings.warmBeforePlay}
            onChange={(e) => {
              const next = { ...settings, warmBeforePlay: e.target.checked };
              onChange(next);
              commit(next);
            }}
          />
          Warm shaders before Play when stale
        </label>
        <label className="check">
          <input
            type="checkbox"
            checked={settings.alwaysRewarm}
            onChange={(e) => {
              const next = { ...settings, alwaysRewarm: e.target.checked };
              onChange(next);
              commit(next);
            }}
          />
          Always re-warm (ignore marker)
        </label>
        <label className="check">
          <input
            type="checkbox"
            checked={settings.warmPipelines}
            onChange={(e) => {
              const next = { ...settings, warmPipelines: e.target.checked };
              onChange(next);
              commit(next);
            }}
          />
          Also prebuild host pipelines (slow, GPU/driver specific)
        </label>
      </div>

      <label className="check">
        <input
          type="checkbox"
          checked={settings.fullscreen}
          onChange={(e) => {
            const next = { ...settings, fullscreen: e.target.checked };
            onChange(next);
            commit(next);
          }}
        />
        Borderless fullscreen hook (force after launch)
      </label>

      <label className="field">
        Freeform VICTORY_* lines (override everything above)
        <textarea
          value={settings.extraEnv}
          onChange={(e) => onChange({ ...settings, extraEnv: e.target.value })}
          onBlur={() => commit()}
          placeholder={"VICTORY_WAVE_WIDE=0\nVICTORY_MAX_FPS=30"}
        />
      </label>

      <details className="preview">
        <summary>Effective environment ({Object.keys(preview).length} vars)</summary>
        <pre className="log">
          {Object.entries(preview)
            .map(([k, v]) => `${k}=${v}`)
            .join("\n") || "(none — blank preset and no freeform lines)"}
        </pre>
      </details>
    </div>
  );
}

function FlagControl({
  def,
  value,
  onChange,
  onBlur,
}: {
  def: FlagDef;
  value: string;
  onChange: (v: string) => void;
  onBlur: () => void;
}) {
  if (def.kind === "toggle") {
    const on = value === "1" || value.toLowerCase() === "true";
    return (
      <label className="flag-row" title={def.help}>
        <span>
          <strong>{def.label}</strong>
          <em>{def.help}</em>
        </span>
        <input
          type="checkbox"
          checked={on}
          onChange={(e) => {
            onChange(e.target.checked ? "1" : "0");
            onBlur();
          }}
        />
      </label>
    );
  }

  if (def.kind === "choice") {
    return (
      <label className="flag-row field" title={def.help}>
        <span>
          <strong>{def.label}</strong>
          <em>{def.help}</em>
        </span>
        <select
          value={value}
          onChange={(e) => {
            onChange(e.target.value);
            onBlur();
          }}
        >
          {def.choices.map((c) => (
            <option key={c || "(off)"} value={c}>
              {c === "" ? "(off)" : c}
            </option>
          ))}
        </select>
      </label>
    );
  }

  return (
    <label className="flag-row field" title={def.help}>
      <span>
        <strong>{def.label}</strong>
        <em>{def.help}</em>
      </span>
      <input
        type={def.kind === "number" ? "number" : "text"}
        value={value}
        placeholder={def.default || "unset"}
        onChange={(e) => onChange(e.target.value)}
        onBlur={onBlur}
      />
    </label>
  );
}
