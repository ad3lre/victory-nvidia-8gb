//! Curated Victory launch flags and Ad3lHub presets — more control than Mandyhub.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FlagKind {
    Toggle,
    Choice,
    Number,
    Text,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FlagDef {
    pub key: &'static str,
    pub label: &'static str,
    pub help: &'static str,
    pub category: &'static str,
    pub kind: FlagKind,
    pub choices: &'static [&'static str],
    pub default: &'static str,
    pub expert: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PresetDef {
    pub id: &'static str,
    pub label: &'static str,
    pub help: &'static str,
}

pub fn catalog() -> Vec<FlagDef> {
    vec![
        FlagDef {
            key: "CONSOLE",
            label: "Console profile",
            help: "series-s is lighter (8 GB cards); series-x is the full Series X profile.",
            category: "Display",
            kind: FlagKind::Choice,
            choices: &["series-s", "series-x"],
            default: "series-s",
            expert: false,
        },
        FlagDef {
            key: "RESOLUTION",
            label: "Resolution cap",
            help: "Caps the recognized resolution ladder (e.g. 1920x1080).",
            category: "Display",
            kind: FlagKind::Text,
            choices: &[],
            default: "1920x1080",
            expert: false,
        },
        FlagDef {
            key: "RENDER_SCALE",
            label: "Render scale",
            help: "Internal scale with the Series S own-upscaler path (0.25–1.0).",
            category: "Display",
            kind: FlagKind::Number,
            choices: &[],
            default: "0.5",
            expert: false,
        },
        FlagDef {
            key: "OWN_UPSCALER",
            label: "Own upscaler",
            help: "Use Victory's upscaler with Series S / render scale.",
            category: "Display",
            kind: FlagKind::Toggle,
            choices: &[],
            default: "1",
            expert: false,
        },
        FlagDef {
            key: "OVERLAY",
            label: "Performance overlay",
            help: "F10 overlay. Off for clean benchmarks.",
            category: "Display",
            kind: FlagKind::Toggle,
            choices: &[],
            default: "1",
            expert: false,
        },
        FlagDef {
            key: "BORDERLESS",
            label: "Borderless window hint",
            help: "Ask the title for a borderless window when supported.",
            category: "Display",
            kind: FlagKind::Toggle,
            choices: &[],
            default: "0",
            expert: false,
        },
        FlagDef {
            key: "TEXTURE_DROP_MIPS",
            label: "Drop texture mips",
            help: "0–4. Higher saves VRAM on 8 GB cards (2 is the usual NVIDIA 8 GB fix).",
            category: "Memory",
            kind: FlagKind::Number,
            choices: &[],
            default: "2",
            expert: false,
        },
        FlagDef {
            key: "NULL_COLOR",
            label: "Missing texture color",
            help: "grey stand-in for missing textures; empty = off.",
            category: "Memory",
            kind: FlagKind::Choice,
            choices: &["", "grey"],
            default: "grey",
            expert: false,
        },
        FlagDef {
            key: "LEVELS_ON_DEMAND",
            label: "Levels on demand",
            help: "Sparse/tiled residency — less VRAM, more streaming work.",
            category: "Memory",
            kind: FlagKind::Toggle,
            choices: &[],
            default: "1",
            expert: false,
        },
        FlagDef {
            key: "HELD_LEVELS",
            label: "Held levels",
            help: "Keep recently used mip levels resident.",
            category: "Memory",
            kind: FlagKind::Toggle,
            choices: &[],
            default: "1",
            expert: true,
        },
        FlagDef {
            key: "TEXTURE_MAX_DIM",
            label: "Texture max dimension",
            help: "Cap upload size (e.g. 1024). 0 = off. Needs levels on demand.",
            category: "Memory",
            kind: FlagKind::Number,
            choices: &[],
            default: "0",
            expert: true,
        },
        FlagDef {
            key: "WINDOW_KB",
            label: "Window KB",
            help: "CPU/GPU window chunk size. 512 is the low-memory preset.",
            category: "Memory",
            kind: FlagKind::Number,
            choices: &[],
            default: "",
            expert: true,
        },
        FlagDef {
            key: "WINDOW_TAILS",
            label: "Window tails",
            help: "Trim window tails (low-memory).",
            category: "Memory",
            kind: FlagKind::Toggle,
            choices: &[],
            default: "0",
            expert: true,
        },
        FlagDef {
            key: "CARD_BUFFERS_MB",
            label: "Card buffers MB",
            help: "GPU buffer budget for card-side copies.",
            category: "Memory",
            kind: FlagKind::Number,
            choices: &[],
            default: "",
            expert: true,
        },
        FlagDef {
            key: "CARD_WINDOWS_MB",
            label: "Card windows MB",
            help: "GPU window budget (Series S low-memory uses 256).",
            category: "Memory",
            kind: FlagKind::Number,
            choices: &[],
            default: "",
            expert: true,
        },
        FlagDef {
            key: "CARD_BUFFERS_MIN_KB",
            label: "Card buffers min KB",
            help: "Minimum card buffer grain (pareto uses 64).",
            category: "Memory",
            kind: FlagKind::Number,
            choices: &[],
            default: "64",
            expert: true,
        },
        FlagDef {
            key: "STAGING_POOL_MB",
            label: "Staging pool MB",
            help: "Low-memory preset uses 64.",
            category: "Memory",
            kind: FlagKind::Number,
            choices: &[],
            default: "",
            expert: true,
        },
        FlagDef {
            key: "STAGING_CACHE_MB",
            label: "Staging cache MB",
            help: "0 disables the staging cache (Series S low-memory).",
            category: "Memory",
            kind: FlagKind::Number,
            choices: &[],
            default: "",
            expert: true,
        },
        FlagDef {
            key: "SCRATCH_SPARE_MB",
            label: "Scratch spare MB",
            help: "Spare scratch memory. Low-memory: 64.",
            category: "Memory",
            kind: FlagKind::Number,
            choices: &[],
            default: "",
            expert: true,
        },
        FlagDef {
            key: "SNAPSHOT_POOL_MB",
            label: "Snapshot pool MB",
            help: "Low-memory: 128.",
            category: "Memory",
            kind: FlagKind::Number,
            choices: &[],
            default: "",
            expert: true,
        },
        FlagDef {
            key: "TEXTURE_POOL_SPARE_MB",
            label: "Texture pool spare MB",
            help: "Low-memory: 256.",
            category: "Memory",
            kind: FlagKind::Number,
            choices: &[],
            default: "",
            expert: true,
        },
        FlagDef {
            key: "VIEW_COPIES_MB",
            label: "View copies MB",
            help: "Low-memory: 32.",
            category: "Memory",
            kind: FlagKind::Number,
            choices: &[],
            default: "",
            expert: true,
        },
        FlagDef {
            key: "RAM_TRIM_MB",
            label: "RAM trim threshold MB",
            help: "Trim idle pools when free physical RAM is under this. 0 = off.",
            category: "Memory",
            kind: FlagKind::Number,
            choices: &[],
            default: "0",
            expert: true,
        },
        FlagDef {
            key: "RELAY",
            label: "Relay stage",
            help: "0 off, 1 first stage, 2 second (pareto / NVIDIA default).",
            category: "Performance",
            kind: FlagKind::Choice,
            choices: &["0", "1", "2"],
            default: "2",
            expert: false,
        },
        FlagDef {
            key: "RELAY_COPY",
            label: "Relay on copy queue",
            help: "Off by default: fetching card buffers on the copy queue without a graphics-fence Wait removed NVIDIA devices (0x887A0001) and crashed AMD (amdxc64 null). Safe again only on builds that Wait before the copy.",
            category: "Performance",
            kind: FlagKind::Toggle,
            choices: &[],
            default: "0",
            expert: true,
        },
        FlagDef {
            key: "TEXTURE_NO_WAIT",
            label: "Texture no-wait",
            help: "Do not stall the CPU waiting for every texture upload.",
            category: "Performance",
            kind: FlagKind::Toggle,
            choices: &[],
            default: "1",
            expert: false,
        },
        FlagDef {
            key: "WIDE_BACK",
            label: "Wide back",
            help: "Parallel path for bringing shader writes back.",
            category: "Performance",
            kind: FlagKind::Toggle,
            choices: &[],
            default: "1",
            expert: false,
        },
        FlagDef {
            key: "WIDE_MERGE",
            label: "Wide merge",
            help: "Merge title memory writes on several threads.",
            category: "Performance",
            kind: FlagKind::Toggle,
            choices: &[],
            default: "1",
            expert: true,
        },
        FlagDef {
            key: "JOIN_PARTS",
            label: "Join parts",
            help: "Batch host copies (pareto uses 8).",
            category: "Performance",
            kind: FlagKind::Number,
            choices: &[],
            default: "8",
            expert: true,
        },
        FlagDef {
            key: "JOIN_COPIES",
            label: "Join copies",
            help: "Join related GPU copies.",
            category: "Performance",
            kind: FlagKind::Toggle,
            choices: &[],
            default: "1",
            expert: true,
        },
        FlagDef {
            key: "THIN",
            label: "Thin translation",
            help: "Spread shader translation across threads.",
            category: "Performance",
            kind: FlagKind::Toggle,
            choices: &[],
            default: "1",
            expert: true,
        },
        FlagDef {
            key: "UPLOAD_AHEAD",
            label: "Upload ahead",
            help: "Prefetch uploads for upcoming frames.",
            category: "Performance",
            kind: FlagKind::Toggle,
            choices: &[],
            default: "1",
            expert: true,
        },
        FlagDef {
            key: "CARD_STEPS",
            label: "Card steps",
            help: "Split card work into steps (pareto: 2).",
            category: "Performance",
            kind: FlagKind::Number,
            choices: &[],
            default: "2",
            expert: true,
        },
        FlagDef {
            key: "CARD_REHASH_RUNS",
            label: "Card rehash runs",
            help: "Rehash card buffers once per pass when set.",
            category: "Performance",
            kind: FlagKind::Toggle,
            choices: &[],
            default: "1",
            expert: true,
        },
        FlagDef {
            key: "INDIRECT_WINDOW",
            label: "Indirect window",
            help: "Indirect draw windowing path.",
            category: "Performance",
            kind: FlagKind::Toggle,
            choices: &[],
            default: "1",
            expert: true,
        },
        FlagDef {
            key: "INDIRECT_BATCH",
            label: "Indirect batch",
            help: "Batch clamps for indirect args (pareto: 2).",
            category: "Performance",
            kind: FlagKind::Number,
            choices: &[],
            default: "2",
            expert: true,
        },
        FlagDef {
            key: "MAP_THREADS",
            label: "Map threads",
            help: "Threads for mapped memory work.",
            category: "Performance",
            kind: FlagKind::Number,
            choices: &[],
            default: "1",
            expert: true,
        },
        FlagDef {
            key: "CARD_AWAKE",
            label: "Keep GPU clocks up",
            help: "Avoid deep GPU idle between frames.",
            category: "Performance",
            kind: FlagKind::Toggle,
            choices: &[],
            default: "1",
            expert: true,
        },
        FlagDef {
            key: "SHOW_AFTER",
            label: "Show after present path",
            help: "Present timing path used by pareto.",
            category: "Performance",
            kind: FlagKind::Toggle,
            choices: &[],
            default: "1",
            expert: true,
        },
        FlagDef {
            key: "LATE_WAIT",
            label: "Late wait stage",
            help: "0 off; 1–3 delay CPU waits for GPU work (expert).",
            category: "Performance",
            kind: FlagKind::Choice,
            choices: &["0", "1", "2", "3"],
            default: "0",
            expert: true,
        },
        FlagDef {
            key: "ASYNC_SUBMIT",
            label: "Async submit",
            help: "Submit without waiting each list (can be unstable on some NVIDIA drivers).",
            category: "Performance",
            kind: FlagKind::Toggle,
            choices: &[],
            default: "0",
            expert: true,
        },
        FlagDef {
            key: "AMBIENT",
            label: "Ambient light",
            help: "Stand-in for missing RT GI (e.g. 0.6 or 0.8). Empty = title only.",
            category: "Lighting",
            kind: FlagKind::Text,
            choices: &[],
            default: "0.6",
            expert: false,
        },
        FlagDef {
            key: "AMBIENT_AIR",
            label: "Ambient air scatter",
            help: "Bounds air scatter vs surrounding light (pareto: 1.5).",
            category: "Lighting",
            kind: FlagKind::Number,
            choices: &[],
            default: "1.5",
            expert: true,
        },
        FlagDef {
            key: "TEXTURE_BACK",
            label: "Texture back / water",
            help: "Refresh textures that the card wrote (helps water/reflections).",
            category: "Lighting",
            kind: FlagKind::Toggle,
            choices: &[],
            default: "0",
            expert: false,
        },
        FlagDef {
            key: "BVH_BUILDER",
            label: "BVH builder (RT)",
            help: "Experimental ray-tracing structure builder. Usually leave off.",
            category: "Lighting",
            kind: FlagKind::Toggle,
            choices: &[],
            default: "0",
            expert: true,
        },
        FlagDef {
            key: "TELEMETRY",
            label: "Fleet telemetry",
            help: "Send crash/perf telemetry for this NVIDIA fork. 0 disables.",
            category: "Diagnostics",
            kind: FlagKind::Toggle,
            choices: &[],
            default: "1",
            expert: false,
        },
        FlagDef {
            key: "LOG",
            label: "Log filter",
            help: "tracing filter, e.g. warn or victory_gpu=info,warn.",
            category: "Diagnostics",
            kind: FlagKind::Text,
            choices: &[],
            default: "warn",
            expert: false,
        },
        FlagDef {
            key: "CENSUS_FRAMES",
            label: "Memory census every N frames",
            help: "0 off. 120 logs a memory census line periodically.",
            category: "Diagnostics",
            kind: FlagKind::Number,
            choices: &[],
            default: "0",
            expert: true,
        },
        FlagDef {
            key: "PANEL",
            label: "Debug panel",
            help: "Extra on-screen panel (pair with overlay).",
            category: "Diagnostics",
            kind: FlagKind::Toggle,
            choices: &[],
            default: "0",
            expert: true,
        },
        FlagDef {
            key: "D3D12_DRED",
            label: "DRED breadcrumbs",
            help: "empty off; light = which list died; 1 = full (diagnostics builds).",
            category: "Diagnostics",
            kind: FlagKind::Choice,
            choices: &["", "light", "1", "faults"],
            default: "",
            expert: true,
        },
    ]
}

pub fn presets() -> Vec<PresetDef> {
    vec![
        PresetDef {
            id: "ad3l-balanced",
            label: "Ad3l balanced (8 GB)",
            help: "Our default: Series S, mip drop, ambient, overlay, core pareto relays.",
        },
        PresetDef {
            id: "pareto",
            label: "Pareto residency",
            help: "Matches Play-VIctory.cmd pareto-residency on this fork.",
        },
        PresetDef {
            id: "series-s-low",
            label: "Series S low memory",
            help: "scripts/play-series-s-low-memory.ps1 — tight VRAM windows.",
        },
        PresetDef {
            id: "nvidia-8gb",
            label: "NVIDIA 8 GB Series X",
            help: "scripts/play-nvidia-8gb.ps1 — Series X at 1080p with overlay.",
        },
        PresetDef {
            id: "blank",
            label: "Blank (repo play.ps1 only)",
            help: "No Ad3lHub flags — only the project's play.ps1 and your freeform lines.",
        },
        PresetDef {
            id: "custom",
            label: "Custom",
            help: "Your saved flag map; editing any control switches here.",
        },
    ]
}

fn set(map: &mut BTreeMap<String, String>, key: &str, value: &str) {
    if value.is_empty() {
        map.remove(key);
    } else {
        map.insert(key.to_string(), value.to_string());
    }
}

/// Base flag map for a named preset (keys without the VICTORY_ prefix).
pub fn preset_flags(id: &str) -> BTreeMap<String, String> {
    let mut m = BTreeMap::new();
    match id {
        "blank" => {}
        "nvidia-8gb" => {
            set(&mut m, "CONSOLE", "series-x");
            set(&mut m, "RESOLUTION", "1920x1080");
            set(&mut m, "TEXTURE_DROP_MIPS", "0");
            set(&mut m, "OVERLAY", "1");
            set(&mut m, "OWN_UPSCALER", "0");
            set(&mut m, "RENDER_SCALE", "");
        }
        "series-s-low" => {
            set(&mut m, "CONSOLE", "series-s");
            set(&mut m, "OWN_UPSCALER", "1");
            set(&mut m, "RENDER_SCALE", "0.5");
            set(&mut m, "CARD_WINDOWS_MB", "256");
            set(&mut m, "CARD_BUFFERS_MB", "768");
            set(&mut m, "WINDOW_KB", "512");
            set(&mut m, "WINDOW_TAILS", "1");
            set(&mut m, "CARD_BUFFERS_MIN_KB", "64");
            set(&mut m, "LEVELS_ON_DEMAND", "1");
            set(&mut m, "TEXTURE_DROP_MIPS", "1");
            set(&mut m, "HELD_LEVELS", "1");
            set(&mut m, "STAGING_CACHE_MB", "0");
            set(&mut m, "TEXTURE_NO_WAIT", "1");
            set(&mut m, "RELAY", "2");
            set(&mut m, "WIDE_BACK", "1");
            set(&mut m, "OVERLAY", "1");
            set(&mut m, "AMBIENT", "0.6");
            set(&mut m, "NULL_COLOR", "grey");
        }
        "pareto" => {
            set(&mut m, "CONSOLE", "series-s");
            set(&mut m, "OWN_UPSCALER", "1");
            set(&mut m, "RENDER_SCALE", "0.5");
            set(&mut m, "AMBIENT_AIR", "1.5");
            set(&mut m, "TEXTURE_NO_WAIT", "1");
            set(&mut m, "RELAY", "2");
            set(&mut m, "WIDE_BACK", "1");
            set(&mut m, "CARD_REHASH_RUNS", "1");
            set(&mut m, "WIDE_MERGE", "1");
            set(&mut m, "JOIN_COPIES", "1");
            set(&mut m, "INDIRECT_WINDOW", "1");
            // RELAY_COPY left off: unfixed builds device-remove / amdxc64-crash on it.
            set(&mut m, "CARD_STEPS", "2");
            set(&mut m, "INDIRECT_BATCH", "2");
            set(&mut m, "MAP_THREADS", "1");
            set(&mut m, "CARD_AWAKE", "1");
            set(&mut m, "SHOW_AFTER", "1");
            set(&mut m, "AMBIENT", "0.8");
            set(&mut m, "THIN", "1");
            set(&mut m, "JOIN_PARTS", "8");
            set(&mut m, "UPLOAD_AHEAD", "1");
            set(&mut m, "LEVELS_ON_DEMAND", "1");
            set(&mut m, "CARD_BUFFERS_MIN_KB", "64");
            set(&mut m, "LOG", "warn");
            set(&mut m, "TEXTURE_DROP_MIPS", "2");
            set(&mut m, "NULL_COLOR", "grey");
            set(&mut m, "OVERLAY", "1");
            set(&mut m, "TELEMETRY", "1");
        }
        // ad3l-balanced and custom fall through to balanced defaults when applying presets
        _ => {
            set(&mut m, "CONSOLE", "series-s");
            set(&mut m, "OWN_UPSCALER", "1");
            set(&mut m, "RENDER_SCALE", "0.5");
            set(&mut m, "RESOLUTION", "1920x1080");
            set(&mut m, "TEXTURE_DROP_MIPS", "2");
            set(&mut m, "NULL_COLOR", "grey");
            set(&mut m, "AMBIENT", "0.6");
            set(&mut m, "AMBIENT_AIR", "1.5");
            set(&mut m, "LEVELS_ON_DEMAND", "1");
            set(&mut m, "TEXTURE_NO_WAIT", "1");
            set(&mut m, "RELAY", "2");
            // RELAY_COPY left off (see FlagDef help / host.rs Wait fix).
            set(&mut m, "WIDE_BACK", "1");
            set(&mut m, "WIDE_MERGE", "1");
            set(&mut m, "JOIN_COPIES", "1");
            set(&mut m, "JOIN_PARTS", "8");
            set(&mut m, "THIN", "1");
            set(&mut m, "UPLOAD_AHEAD", "1");
            set(&mut m, "CARD_STEPS", "2");
            set(&mut m, "CARD_REHASH_RUNS", "1");
            set(&mut m, "INDIRECT_WINDOW", "1");
            set(&mut m, "INDIRECT_BATCH", "2");
            set(&mut m, "MAP_THREADS", "1");
            set(&mut m, "CARD_AWAKE", "1");
            set(&mut m, "SHOW_AFTER", "1");
            set(&mut m, "CARD_BUFFERS_MIN_KB", "64");
            set(&mut m, "OVERLAY", "1");
            set(&mut m, "TELEMETRY", "1");
            set(&mut m, "LOG", "warn");
        }
    }
    m
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchPlan {
    pub preset: String,
    pub flags: BTreeMap<String, String>,
    pub extra_env: String,
    pub fullscreen_hook: bool,
}

/// Merge preset → user flags → freeform extra lines into VICTORY_* env pairs.
pub fn resolve_env(plan: &LaunchPlan) -> BTreeMap<String, String> {
    let mut merged = if plan.preset == "custom" || plan.preset.is_empty() {
        // Custom: use the saved map as the base (already full).
        plan.flags.clone()
    } else if plan.preset == "blank" {
        BTreeMap::new()
    } else {
        let mut base = preset_flags(&plan.preset);
        for (k, v) in &plan.flags {
            if v.is_empty() {
                base.remove(k);
            } else {
                base.insert(k.clone(), v.clone());
            }
        }
        base
    };

    // Freeform lines always win.
    for line in plan.extra_env.lines() {
        if let Some((k, v)) = line.split_once('=') {
            let k = k.trim();
            let key = k.strip_prefix("VICTORY_").unwrap_or(k);
            if key.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
                && !key.is_empty()
            {
                let v = v.trim();
                if v.is_empty() {
                    merged.remove(key);
                } else {
                    merged.insert(key.to_string(), v.to_string());
                }
            }
        }
    }

    let mut out = BTreeMap::new();
    for (k, v) in merged {
        if v.is_empty() {
            continue;
        }
        out.insert(format!("VICTORY_{k}"), v);
    }
    out
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FlagCatalog {
    pub presets: Vec<PresetDef>,
    pub flags: Vec<FlagDef>,
}
