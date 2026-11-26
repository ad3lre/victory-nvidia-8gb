@echo off
setlocal EnableExtensions DisableDelayedExpansion
if not "%~1"=="" if /i not "%~1"=="pareto-residency" (
  echo Usage: Play-VIctory.cmd pareto-residency 1>&2
  exit /b 2
)
if not defined VICTORY_GAME_DIR set "VICTORY_GAME_DIR=%~dp0..\GTA6.Xbox.v1.0.2.1"
if not defined VICTORY_TITLE_EXE set "VICTORY_TITLE_EXE=%VICTORY_GAME_DIR%\game_scarlett_master.exe"
if not exist "%VICTORY_TITLE_EXE%" (
  echo Set VICTORY_TITLE_EXE or VICTORY_GAME_DIR to point to your own title files. 1>&2
  exit /b 2
)
set "runner=%VICTORY_RUNNER%"
if not defined runner (
  set "runner=%~dp0victory-3d-rtx-pareto-residency.exe"
  if not exist "%~dp0victory-3d-rtx-pareto-residency.exe" set "runner=%~dp0target\release\victory.exe"
)
if not exist "%runner%" (
  echo Extract the Windows release or run cargo build --release --locked -p victory. 1>&2
  exit /b 2
)
for %%I in ("%runner%") do set "runner_dir=%%~dpI"
if not exist "%runner_dir%D3D12\D3D12Core.dll" (
  echo Agility SDK missing: %runner_dir%D3D12\D3D12Core.dll 1>&2
  echo Rebuild so apps\victory\build.rs installs D3D12Core.dll next to victory.exe ^(else 0x887E0003^). 1>&2
  exit /b 2
)
if not defined VICTORY_AMBIENT_AIR set "VICTORY_AMBIENT_AIR=1.5"
if not defined VICTORY_CACHE_DIR set "VICTORY_CACHE_DIR=%LOCALAPPDATA%\VIctory\cache-pareto"
if not defined VICTORY_TEXTURE_NO_WAIT set "VICTORY_TEXTURE_NO_WAIT=1"
if not defined VICTORY_RELAY set "VICTORY_RELAY=2"
if not defined VICTORY_WIDE_BACK set "VICTORY_WIDE_BACK=1"
if not defined VICTORY_CARD_REHASH_RUNS set "VICTORY_CARD_REHASH_RUNS=1"
if not defined VICTORY_WIDE_MERGE set "VICTORY_WIDE_MERGE=1"
if not defined VICTORY_JOIN_COPIES set "VICTORY_JOIN_COPIES=1"
if not defined VICTORY_INDIRECT_WINDOW set "VICTORY_INDIRECT_WINDOW=1"
rem RELAY_COPY off by default: copy-queue fetch without a graphics-fence Wait
rem device-removed NVIDIA (0x887A0001) and crashed AMD (amdxc64). Re-enable
rem only on builds that Wait in host.rs relay_card_back.
if not defined VICTORY_RELAY_COPY set "VICTORY_RELAY_COPY=0"
if not defined VICTORY_CARD_STEPS set "VICTORY_CARD_STEPS=2"
if not defined VICTORY_INDIRECT_BATCH set "VICTORY_INDIRECT_BATCH=2"
if not defined VICTORY_MAP_THREADS set "VICTORY_MAP_THREADS=1"
if not defined VICTORY_CARD_AWAKE set "VICTORY_CARD_AWAKE=1"
if not defined VICTORY_SHOW_AFTER set "VICTORY_SHOW_AFTER=1"
if not defined VICTORY_AMBIENT set "VICTORY_AMBIENT=0.8"
if not defined VICTORY_THIN set "VICTORY_THIN=1"
if not defined VICTORY_JOIN_PARTS set "VICTORY_JOIN_PARTS=8"
if not defined VICTORY_UPLOAD_AHEAD set "VICTORY_UPLOAD_AHEAD=1"
if not defined VICTORY_LEVELS_ON_DEMAND set "VICTORY_LEVELS_ON_DEMAND=1"
if not defined VICTORY_CARD_BUFFERS_MIN_KB set "VICTORY_CARD_BUFFERS_MIN_KB=64"
if not defined VICTORY_D3D12_DRED set "VICTORY_D3D12_DRED=light"
if not defined VICTORY_D3D12_DEBUG set "VICTORY_D3D12_DEBUG=1"
if not defined VICTORY_TRAIL set "VICTORY_TRAIL=1"
if not defined VICTORY_LOG if not defined RUST_LOG set "VICTORY_LOG=victory_gpu=info,victory_telemetry=info,warn"
if not defined VICTORY_CONSOLE set "VICTORY_CONSOLE=series-s"
if /i not "%VICTORY_CONSOLE%"=="series-s" if /i not "%VICTORY_CONSOLE%"=="series-x" (
  echo Set VICTORY_CONSOLE to series-s or series-x. 1>&2
  exit /b 2
)
"%runner%" run "%VICTORY_TITLE_EXE%" --console %VICTORY_CONSOLE% --execute
exit /b %errorlevel%
