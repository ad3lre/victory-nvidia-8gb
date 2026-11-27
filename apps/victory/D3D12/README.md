# D3D12 Agility SDK 1.620.2 (vendored)

App-local Microsoft Direct3D Agility runtime used by `apps/victory`
(`D3D12SDKVersion` 620 / `D3D12SDKPath` `.\D3D12\`).

| File | Role |
|------|------|
| `D3D12Core.dll` | Agility core (required to start) |
| `d3d12SDKLayers.dll` | Debug layer companion (`VICTORY_D3D12_DEBUG`) |
| `LICENSE.txt` / `LICENSE-CODE.txt` | Microsoft package licences |

Source package (same bytes `build.rs` used to download):

https://api.nuget.org/v3-flatcontainer/microsoft.direct3d.d3d12/1.620.2/microsoft.direct3d.d3d12.1.620.2.nupkg

These are Microsoft redistributables, not title/game assets. `build.rs`
copies them next to `victory.exe` on MSVC builds; NuGet fetch remains a
fallback if a file is missing from this folder.
