# Launch victory on the 1650 box the way Mandyhub does, then watch until
# crash / world draws / timeout. Writes a machine-readable status file.
param(
    [int]$TimeoutSec = 300,
    [string]$Tag = "observe",
    [switch]$NoDebug
)

$ErrorActionPreference = "Continue"
$user = "C:\Users\ifix4"
$env:USERPROFILE = $user
$env:HOME = $user
$env:LOCALAPPDATA = "$user\AppData\Local"
$env:APPDATA = "$user\AppData\Roaming"

$slug = "ad3lre_victory-nvidia-8gb"
$runner = "$env:LOCALAPPDATA\Mandyhub\target\$slug\diag\victory.exe"
$titleDir = "E:\GTA6.Xbox.v1.0.2.1-CYBERLEEK"
$title = Join-Path $titleDir "game_scarlett_master.exe"
$data = "$env:LOCALAPPDATA\Mandyhub\data\$slug"
$cache = "$env:LOCALAPPDATA\Mandyhub\cache\$slug"
$outDir = "$env:LOCALAPPDATA\Ad3lHub\observe"
New-Item -ItemType Directory -Force -Path $outDir | Out-Null

$status = Join-Path $outDir "status-$Tag.json"
$log = Join-Path $outDir "launch-$Tag.log"
$done = Join-Path $outDir "done-$Tag.txt"
Remove-Item $done -ErrorAction SilentlyContinue
Set-Content -Path $log -Value "" -Encoding UTF8

function W($m) {
    $line = "[{0}] {1}" -f (Get-Date -Format "HH:mm:ss"), $m
    Add-Content -Path $log -Value $line -Encoding UTF8
    Write-Output $line
}

if (-not (Test-Path $runner)) { W "MISSING runner $runner"; Set-Content $done "FAIL missing_runner"; exit 2 }
if (-not (Test-Path $title)) { W "MISSING title $title"; Set-Content $done "FAIL missing_title"; exit 2 }

Get-Process victory -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Start-Sleep -Seconds 2

# Clear leftover VICTORY_* then apply Mandyhub-ish + play.ps1 defaults in THIS process
Get-ChildItem Env:VICTORY_* -ErrorAction SilentlyContinue | ForEach-Object {
    Remove-Item "Env:$($_.Name)" -ErrorAction SilentlyContinue
}

# Fresh runtime log per tag so evidence lines are not lost in a 200MB locked file.
$runtimeLog = Join-Path $outDir "runtime-$Tag.log"
Remove-Item $runtimeLog -ErrorAction SilentlyContinue

$settings = @{
    VICTORY_DATA_DIR = $data
    VICTORY_CACHE_DIR = $cache
    VICTORY_LOG_FILE = $runtimeLog
    VICTORY_TEXTURE_NO_WAIT = "1"
    VICTORY_RELAY = "2"
    VICTORY_WIDE_BACK = "1"
    VICTORY_CARD_REHASH_RUNS = "1"
    VICTORY_JOIN_COPIES = "1"
    VICTORY_INDIRECT_WINDOW = "1"
    VICTORY_RELAY_COPY = "0"
    VICTORY_CARD_STEPS = "2"
    VICTORY_INDIRECT_BATCH = "2"
    VICTORY_MAP_THREADS = "1"
    VICTORY_CARD_AWAKE = "1"
    VICTORY_SHOW_AFTER = "1"
    VICTORY_AMBIENT = "0.6"
    VICTORY_AMBIENT_AIR = "1.5"
    VICTORY_D3D12_DRED = "1"
    VICTORY_D3D12_DEBUG = $(if ($NoDebug) { "0" } else { "1" })
    VICTORY_SKIP_PIPELINES = "none"
    VICTORY_TRAIL = "1"
    VICTORY_THIN = "1"
    VICTORY_JOIN_PARTS = "8"
    VICTORY_UPLOAD_AHEAD = "1"
    VICTORY_WIDE_MERGE = "1"
    VICTORY_CONSOLE = "series-s"
    VICTORY_OWN_UPSCALER = "1"
    VICTORY_RENDER_SCALE = "0.5"
    VICTORY_RESOLUTION = "1920x1080"
    VICTORY_TEXTURE_DROP_MIPS = "2"
    VICTORY_NULL_COLOR = "grey"
    # Scarlett: LEVELS_ON_DEMAND=0 was the largest residency win.
    VICTORY_LEVELS_ON_DEMAND = "0"
    VICTORY_FILL_TITLE = "1"
    VICTORY_BAKE_TEXTURES_MS = "10000"
    VICTORY_OVERLAY = "1"
    VICTORY_TELEMETRY = "1"
    VICTORY_LOG = "victory_gpu=info,victory_gpu::evidence=info,victory_telemetry=info,warn"
}
foreach ($k in $settings.Keys) { Set-Item "Env:$k" ([string]$settings[$k]) }

# Fresh events file so old crashes are not re-uploaded into this run
$events = Join-Path $data "telemetry\victory-events.jsonl"
New-Item -ItemType Directory -Force -Path (Split-Path $events) | Out-Null
Set-Content -Path $events -Value "" -Encoding UTF8

W "launch tag=$Tag runner=$runner debug=$($settings.VICTORY_D3D12_DEBUG)"
W ("exe mtime=" + (Get-Item $runner).LastWriteTime.ToString("o"))
$built = "$env:LOCALAPPDATA\Ad3lHub\target\$slug\built-from.txt"
if (Test-Path $built) { W ("built-from=" + (Get-Content $built -Raw).Trim()) }
# Stamp Scarlett/evidence knobs as the child will see them (inherit).
W ("env LEVELS_ON_DEMAND=$env:VICTORY_LEVELS_ON_DEMAND FILL_TITLE=$env:VICTORY_FILL_TITLE BAKE_TEXTURES_MS=$env:VICTORY_BAKE_TEXTURES_MS DROP_MIPS=$env:VICTORY_TEXTURE_DROP_MIPS")

$started = Get-Date
$arg = "run `"$title`" --execute --console series-s"
# Inherit current env (with VICTORY_* set). Do not redirect stdout: some hosts
# stall when the pipe fills; Mandyhub also runs without a console.
$proc = Start-Process -FilePath $runner -ArgumentList $arg -WorkingDirectory (Split-Path $runner) -PassThru -WindowStyle Normal
W ("pid=" + $proc.Id)

$deadline = $started.AddSeconds($TimeoutSec)
$verdict = "TIMEOUT"
$reason = "timeout"
$maxDraws = 0.0
$samples = 0
$crash = $false
$runtime = $runtimeLog
if (-not $runtime) { $runtime = "$env:LOCALAPPDATA\victory\data\logs\victory-runtime.log" }
$runtimeMark = if (Test-Path $runtime) { (Get-Item $runtime).Length } else { 0 }

while ((Get-Date) -lt $deadline) {
    if ($proc.HasExited) {
        $code = $proc.ExitCode
        W ("exited code=" + $code)
        $crashLogs = @(Get-ChildItem "$data" -Recurse -Filter "*.crash.log" -ErrorAction SilentlyContinue |
            Where-Object { $_.LastWriteTime -gt $started.AddSeconds(-5) } |
            Sort-Object LastWriteTime -Descending)
        if ($crashLogs.Count -gt 0) {
            $crash = $true
            W ("crash log=" + $crashLogs[0].FullName)
        }
        if (-not $crash -and (Test-Path $runtime)) {
            $bytes = [IO.File]::ReadAllBytes($runtime)
            if ($bytes.Length -gt $runtimeMark) {
                $tail = [Text.Encoding]::UTF8.GetString($bytes, [Math]::Max($runtimeMark, $bytes.Length - 50000), [Math]::Min(50000, $bytes.Length - $runtimeMark))
                if ($tail -match "did not close|device removed|host GPU failure") { $crash = $true }
            }
        }
        if ($crash) { $verdict = "CRASH"; $reason = "process_exit_crash" }
        elseif ($maxDraws -ge 64) { $verdict = "PASS"; $reason = "left_intro_then_exit" }
        else { $verdict = "FAIL"; $reason = "early_exit_code_$code" }
        break
    }

    $sampleFiles = @(Get-ChildItem "$data\telemetry\samples-*.jsonl" -ErrorAction SilentlyContinue |
        Where-Object { $_.LastWriteTime -gt $started.AddSeconds(-5) -and $_.Length -gt 0 } |
        Sort-Object LastWriteTime -Descending)
    if ($sampleFiles.Count -gt 0) {
        $lines = Get-Content $sampleFiles[0].FullName -ErrorAction SilentlyContinue
        $samples = @($lines).Count
        foreach ($line in $lines) {
            try {
                $j = $line | ConvertFrom-Json
                if ($j.draws -gt $maxDraws) { $maxDraws = [double]$j.draws }
            } catch {}
        }
        if (($samples % 15) -eq 1) { W ("progress samples=$samples max_draws=$maxDraws") }
        if ($maxDraws -ge 64) {
            W ("WORLD draws max=$maxDraws samples=$samples - leaving intro")
            Start-Sleep -Seconds 25
            if (-not $proc.HasExited) {
                $verdict = "PASS"
                $reason = "world_draws"
                try { Stop-Process -Id $proc.Id -Force } catch {}
            }
            break
        }
    }

    # New runtime ERROR after start
    if (Test-Path $runtime) {
        $bytes = [IO.File]::ReadAllBytes($runtime)
        if ($bytes.Length -gt $runtimeMark) {
            $chunk = [Text.Encoding]::UTF8.GetString($bytes, $runtimeMark, $bytes.Length - $runtimeMark)
            if ($chunk -match "D3D12 debug layer on") { W "debug layer ON" }
            if ($chunk -match "no D3D12 debug layer") { W "debug layer MISSING" }
            if ($chunk -match "did not close|host GPU failure") {
                W "runtime reports GPU failure while still running"
            }
            $swapFails = ([regex]::Matches($chunk, "no swap chain error")).Count
            if ($swapFails -ge 20) {
                W "swap-chain spin ($swapFails); aborting"
                $verdict = "FAIL"
                $reason = "swap_chain_unavailable"
                try { Stop-Process -Id $proc.Id -Force } catch {}
                break
            }
            $runtimeMark = $bytes.Length
        }
    }
    Start-Sleep -Seconds 5
}

if (-not $proc.HasExited) {
    try { Stop-Process -Id $proc.Id -Force } catch {}
    Start-Sleep -Seconds 2
}

$shots = 0
$shotDir = @(Get-ChildItem "$data\telemetry\screenshots-*" -Directory -ErrorAction SilentlyContinue |
    Where-Object { $_.LastWriteTime -gt $started.AddSeconds(-5) } |
    Sort-Object LastWriteTime -Descending | Select-Object -First 1)
if ($shotDir) { $shots = @(Get-ChildItem $shotDir[0].FullName -Filter *.jpg -ErrorAction SilentlyContinue).Count }

$body = @{
    tag = $Tag
    verdict = $verdict
    reason = $reason
    max_draws = $maxDraws
    samples = $samples
    screenshots = $shots
    crash = $crash
    runner = $runner
    finished = (Get-Date).ToString("o")
} | ConvertTo-Json -Compress
Set-Content -Path $status -Value $body -Encoding UTF8
Set-Content -Path $done -Value "$verdict $reason max_draws=$maxDraws" -Encoding UTF8
W "DONE $body"
exit $(if ($verdict -eq "PASS") { 0 } else { 1 })
