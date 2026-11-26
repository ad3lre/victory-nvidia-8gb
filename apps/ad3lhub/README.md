# Ad3lHub

Leo-styled desktop launcher for VIctory-based projects. **ad3lre/victory-nvidia-8gb** is featured; other PortWatch projects are secondary.

No game files are included or downloaded. You need your own copy of the game.

## Features

- Single home view (no tabs): featured hero, game/tools strip, community list; settings, activity log, and add-repo as modals
- Featured install/play for `ad3lre/victory-nvidia-8gb`
- Live PortWatch project list, custom GitHub links
- Install / update / uninstall (repos, build targets, shader cache; saves kept)
- Game-folder check, drive search, install-folder move
- Launch presets (Ad3l balanced, pareto, Series S low-memory, NVIDIA 8 GB) plus curated `VICTORY_*` flags for display, memory, performance, lighting, and diagnostics — more than Mandyhub’s few toggles
- Freeform env overrides and an effective-environment preview for power users
- **Shader warm** (AnyXboxX-style): `victory shaders` on game `.rpf` archives before Play, with a fingerprint marker so later launches skip when the build/flags/archives are unchanged; optional host pipeline prebuild
- Detect Git / Rust / MSVC; install missing tools with winget
- Update checks on start and every 15 minutes
- Resilient boot (featured shows before PortWatch), throttled job log streaming, shallow-fetch deepen + cargo `--locked` retry on update/install

## Requirements

- Windows 10 or 11, 64-bit
- WebView2 (bundled bootstrapper on first run)
- To build projects: Git, Rust, and C++ Build Tools (Ad3lHub can install them via winget)

## Develop

```powershell
cd apps\ad3lhub
npm install
npm run tauri:dev
```

## Release build

```powershell
cd apps\ad3lhub
npm install
npm run tauri:build
```

Installer output: `src-tauri\target\release\bundle\nsis\`.

## Credits

UI and product direction: Ad3lHub. Launcher behavior adapted from [Mandyhub](https://github.com/Z-rox/mandyhub) by Zettrox. Project list from [PortWatch](https://portwatch-vi.xyz). Projects are their authors’ work on VIctory by Marvisak and contributors.

License: GPL-3.0-or-later (same as this repository).
