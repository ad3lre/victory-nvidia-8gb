# Starts the title with the settings this fork is played with (README.md, "How to run it").
#   scripts\play.ps1 -Exe <path of your own copy's executable>
# -Safe         without the newer, faster ways (the first thing to try when something is wrong)
# -RayTracing   the title's own ray-traced indirect light: lit interiors, about a third slower
# -Build        target\diag (the default, the build all measurements are of) or target\release
# -More         other settings: -More @{VICTORY_PANEL='1'}
# Run it from a normal window, not as administrator: the driver then keeps its shader cache elsewhere.
param(
    [Parameter(Mandatory)][string]$Exe,
    [switch]$Safe,
    [switch]$RayTracing,
    [ValidateSet('diag', 'release')][string]$Build = 'diag',
    [hashtable]$More = @{}
)
$settings = @{
    VICTORY_TEXTURE_NO_WAIT = '1'; VICTORY_RELAY = '2'; VICTORY_WIDE_BACK = '1'; VICTORY_CARD_REHASH_RUNS = '1'
    # RELAY_COPY off: unfixed copy-queue fetch races the drawing queue (NVIDIA
    # 0x887A0001 device-removed / AMD amdxc64 null). Re-enable only with the Wait fix.
    VICTORY_JOIN_COPIES = '1'; VICTORY_INDIRECT_WINDOW = '1'; VICTORY_RELAY_COPY = '0'; VICTORY_CARD_STEPS = '2'
    VICTORY_INDIRECT_BATCH = '2'; VICTORY_MAP_THREADS = '1'; VICTORY_TITLE_PRIORITIES = '0'; VICTORY_CARD_AWAKE = '1'
    VICTORY_SHOW_AFTER = '1'; VICTORY_AMBIENT = '0.8'; VICTORY_D3D12_DRED = '1'; VICTORY_SKIP_PIPELINES = 'none'
    # Scarlett/cool_fox evidence: LEVELS_ON_DEMAND=1 thrashed re-uploads; off
    # cut remakes ~6.4 GB -> 14 MB. FILL_TITLE skips waits on title-written
    # constants; longer bake wait for character/cutscene textures.
    VICTORY_LEVELS_ON_DEMAND = '0'; VICTORY_FILL_TITLE = '1'; VICTORY_BAKE_TEXTURES_MS = '10000'
    # Aggressive 4 GB (GTX 1650) pack that got a moving Series-S world on screen:
    # drop finest mips, cap uploads, shrink target reservation (default 3072 MB
    # made room() false ~1 GB uploaded), half Series-S scale again.
    VICTORY_TEXTURE_DROP_MIPS = '4'
    VICTORY_TEXTURE_MAX_DIM = '256'
    VICTORY_NULL_COLOR = 'grey'
    VICTORY_OWN_UPSCALER = '1'
    VICTORY_RENDER_SCALE = '0.25'
    VICTORY_CARD_ROOM_FOR_TARGETS_MB = '512'
    VICTORY_CARD_ROOM_FOR_BUFFERS = '0'
    VICTORY_CARD_WINDOWS_MB = '192'
    VICTORY_CARD_BUFFERS_MB = '256'
    VICTORY_TEXTURE_ROOM_PERCENT = '92'
    VICTORY_OVERLAY = '0'
    # Observability: name INVALID_CALL on device-removed; keep GPU trail through boot.
    # victory_gpu::evidence target: residency/compose/clamp line every second.
    VICTORY_D3D12_DEBUG = '1'; VICTORY_TRAIL = '1'
    VICTORY_LOG = 'victory_gpu=info,victory_gpu::evidence=info,victory_hle=info,warn'
}
if ($Safe) {
    $settings.VICTORY_UNDER_WIDE = '0'; $settings.VICTORY_LAND_ECHO = '0'
} else {
    $settings.VICTORY_THIN = '1'; $settings.VICTORY_JOIN_PARTS = '8'; $settings.VICTORY_UPLOAD_AHEAD = '1'
}
if ($RayTracing) { $settings.VICTORY_BVH_BUILDER = '1' }
foreach ($k in $More.Keys) { $settings[$k] = $More[$k] }

$victory = Join-Path $PSScriptRoot "..\target\$Build\victory.exe"
if (-not (Test-Path $victory)) { Write-Host "No $victory. Build it first (README.md)."; return }
if (-not (Test-Path $Exe)) { Write-Host "No $Exe."; return }
# (Other settings left over in this window go; where your files are stays.)
$paths = 'VICTORY_CONFIG', 'VICTORY_GAME_DIR', 'VICTORY_CACHE_DIR', 'VICTORY_DATA_DIR'
Get-ChildItem Env:VICTORY_* | Where-Object { $_.Name -notin $paths } | ForEach-Object { Remove-Item "Env:$($_.Name)" }
foreach ($k in $settings.Keys) { Set-Item "Env:$k" $settings[$k] }
$Exe = (Resolve-Path $Exe).Path
Push-Location (Join-Path $PSScriptRoot '..')   # victory.toml is looked for here
try { & $victory run $Exe --execute } finally { Pop-Location }
