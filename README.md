# Adel's VIctory NVIDIA 8 GB

## Ad3lHub

Leo-styled Windows launcher for this fork and other VIctory projects: see [`apps/ad3lhub`](apps/ad3lhub). Tag `ad3lhub-v*` to publish a GitHub Release build.

## Older Windows D3D12 runtimes

The integration retains the NVIDIA fork's D3D12 Agility SDK 620 exports.
`apps/victory/build.rs` downloads Microsoft.Direct3D.D3D12 **1.620.2** from
NuGet during `cargo build` (MSVC targets) and installs
`target\<profile>\D3D12\D3D12Core.dll` next to `victory.exe`, verified by
SHA-256. Keep that `D3D12` folder beside any copied executable.

Skip the fetch with `VICTORY_SKIP_AGILITY_FETCH=1` only when the DLL is
already present. Manual package URL (same bytes build.rs uses):

<https://api.nuget.org/v3-flatcontainer/microsoft.direct3d.d3d12/1.620.2/microsoft.direct3d.d3d12.1.620.2.nupkg>

If the log reports `0x887E0003` (`D3D12_ERROR_INVALID_REDIST`), the runtime is
missing, outside the configured `D3D12` subfolder, or mismatched with SDK 620.
Delete any stale app-local DLL and rebuild so `build.rs` can reinstall 1.620.2.

See the [GTX 1660 test notes](docs/HARDWARE-GTX1660.md) for previous results.

## Run the pareto-residency profile

Build from this checkout with the pinned Rust toolchain, MSVC C++ Build
Tools, and the Windows SDK. The launcher selects Series S and the residency
settings from the RTX build. Use your own title files.

```powershell
cargo build --release --locked -p victory
$env:VICTORY_GAME_DIR = 'D:\path\to\your\title'
$env:VICTORY_RUNNER = (Resolve-Path .\target\release\victory.exe).Path
.\Play-VIctory.cmd pareto-residency
```

The fork's existing `scripts\play-nvidia-8gb.ps1 -Exe <path>` command launches
the new renderer with Series X at 1080p by default. The `-DropMips` argument
is accepted for older scripts and emits a warning when given a nonzero value.
Press F10 to toggle the performance overlay. It starts hidden in pareto
launches unless `VICTORY_OVERLAY=1` is set. Use `-NoOverlay` or
`VICTORY_OVERLAY=0` for benchmarks without it. The diagnostics build's
separate panel is still available on F3.

Set `VICTORY_CONSOLE=series-x` to use Series X with the same launcher.
`VICTORY_RESOLUTION=1920x1080` caps the recognized resolution ladder.
For an independent-flip black window, set `VICTORY_WINDOW_PERCENT=88`
with an explicit `VICTORY_RESOLUTION` or `VICTORY_WINDOW_SIZE`.
Set `VICTORY_CACHE_DIR` to choose a cache outside the checkout. The
launcher defaults to `%LOCALAPPDATA%\VIctory\cache-pareto`.

See the [performance comparison and test plan](docs/NVIDIA-INTEGRATION.md).

Every run sends an anonymous performance report (PC specs, driver versions,
fps timeline) to `https://rxnd.de/vi/v1` so regressions can be compared between
builds; local copies stay in `%LOCALAPPDATA%\victory\data\telemetry`. On
Windows a sibling recorder process keeps uploading if the game crashes. It is
on by default and can be disabled with `VICTORY_TELEMETRY=0`, `[telemetry]
enabled = false` in `victory.toml`, or `victory run --no-telemetry`. See
[docs/telemetry.md](docs/telemetry.md) for exactly what is collected.

## Upstream RX 7900 XTX notes

A fork of [VIctory](https://github.com/Marvisak/VIctory), a compatibility layer
that runs an Xbox Series title natively on Windows from files the user supplies.
This one is about **speed on AMD RDNA 3**: the world at 30 frames a second on
one PC, without giving up the picture. It is not there yet.

**This repository contains no game files, code or assets.** You need your own
copy of the game. It is not affiliated with or endorsed by the game's developer
or publisher.

## Where it stands (2026-11-25)

Tested on one machine only: Windows 11, Intel i9-12900K, 64 GB RAM, Radeon
RX 7900 XTX, 1440p, the Series X profile.

| | |
|---|---|
| Menus, videos, loading screens | 30 fps |
| Prologue, indoors | 50-75 ms a frame (13-20 fps) |
| First mission with the boat | 46-53 ms (19-22 fps) |
| Street chase with many cars and people | 100-165 ms (6-10 fps), single frames far longer |
| Outdoors on foot, the camp | 60-90 ms |
| Flying | 42-57 ms |
| Longest session so far | 41 minutes, no crash |
| Memory | 40-46 GB of private memory: **64 GB of RAM recommended** |

It started at 280 ms a frame in the same scenes two days earlier.

What is known to be wrong:

- Trees, plants and far buildings flicker outdoors in some places.
- Rooms with little direct light are too dark without `-RayTracing`.
- Textures of what just came into view appear late while the title streams.
- The first run with a new driver or a new build stutters while the driver makes
  pipelines (`victory warm` makes them ahead).
- Recording a frame in the world makes the title's own watchdog end it.

## How to run it

You need Windows 10/11 x64, a CPU with AVX2, a Direct3D 12 GPU with shader
model 6.6 and mesh shaders, Rust (the toolchain is pinned), the MSVC build
tools, and your own dump of the title.

```powershell
git clone https://github.com/avatarmanz/VIctory-7900XTX
cd VIctory-7900XTX
copy victory.example.toml victory.toml      # then set your paths, outside this folder
cargo build --locked --profile diag -p victory --features diagnostics
scripts\play.ps1 -Exe <path of your copy's executable>
```

`scripts\play.ps1 -Safe` leaves out the newest, faster ways; try it first when
something is wrong. `-RayTracing` turns the title's ray-traced indirect light on.

Before the first run, ahead of time (both can be stopped and go on later):

```powershell
target\diag\victory.exe shaders <the shader archives>   # every shader translated, minutes
target\diag\victory.exe warm --used                     # pipelines earlier runs drew with, for a new driver or build
```

Commands, debugging switches and upstream's own description:
[docs/README-fxceless.md](docs/README-fxceless.md). Every `VICTORY_...` switch is
described in a comment where it is read.

## What is different here

In short; the reasons and the measurements are in [docs/findings.md](docs/findings.md).

- **List calls are written down, not carried out, on the title's threads**
  (`VICTORY_THIN`): eight threads of ours translate them.
- **Submissions are prepared by a thread of ours and joined**
  (`VICTORY_ASYNC_SUBMIT`, `VICTORY_JOIN_PARTS`), with what can be spread over
  threads spread: refreshes, compares (AVX2), descriptor records, landings.
- **What shaders wrote comes back beside the write guards**, by the dwords that
  changed, on a thread of its own (`VICTORY_RELAY`, `VICTORY_LAND`).
- **Buffer fills that need bytes still on the card are done on the card**
  (`VICTORY_CARD_STEPS`) instead of waiting for it.
- **The title's writes are caught more cheaply**: one view opened at a fault,
  blocks that rest, places in the title's code that look before they write.
- **Textures go up on a copy queue of their own** and a slot shows one only after
  its upload ran, which also ended a device loss that came every 15 minutes or so.
- **Boats float**: textures that shaders write and the title's CPU reads go back
  to title memory (from fxceless and Geckomacabre, on by default here).
- **Frame record and replay** with per-pass times and shader variants compared
  turn by turn ([docs/replay.md](docs/replay.md)).

## Credits

This is a tree of other people's work with more on top. The history is kept, and
commits taken from elsewhere name the commit and its author.

- **[VIctory](https://github.com/Marvisak/VIctory)** by Marvisak and its
  contributors: the loader, the runtime services, the Direct3D 12 back end and
  the shader recompiler.
- **[fxceless/victory-fork](https://github.com/fxceless/victory-fork)**: the fork
  this one started from, and the people whose pull requests it carries.
- **[DexNom/VIctory](https://github.com/DexNom/VIctory)**: ray tracing fixes,
  cube gradients, sparse textures.
- **[Belobug/VIctory-Belobug-AMD](https://github.com/Belobug/VIctory-Belobug-AMD)**:
  the ambient light idea, pipeline workers, and work by romainhedouin.
- **[FQY55kai/ZJOU-VICTORY](https://github.com/FQY55kai/ZJOU-VICTORY)**.
- **[myokex/victory-amd](https://github.com/myokex/victory-amd)**: the guard over
  every view of a page (with Spexivus, joroyt and tightfall), and Joshua Odom's
  reads into title memory.
- **Geckomacabre** and **cxy0072019-sketch**: the texture read-back.
- **[XCL](https://github.com/ivall/xcl)** (GPL-2.0-only) was read for how the same
  title behaves under another layer; no code of it is here.
- This fork's changes were made with [Claude Code](https://claude.com/claude-code).

## License

GPL-3.0-or-later, as VIctory. See [LICENSE](LICENSE); the one file that is not
covered is named in [docs/asset-policy.md](docs/asset-policy.md).
