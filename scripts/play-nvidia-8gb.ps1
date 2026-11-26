param(
    [Parameter(Mandatory)][string]$Exe,
    [ValidateRange(0, 4)][int]$DropMips = 0,
    [string]$Resolution = '1920x1080',
    [switch]$Borderless,
    [switch]$Water,
    [switch]$NoOverlay
)

$repoRoot = Split-Path -Parent $PSScriptRoot
$runner = Join-Path $repoRoot 'target\release\victory.exe'
if (-not (Test-Path -LiteralPath $runner -PathType Leaf)) {
    throw "Release executable missing at $runner. Run cargo build --release --locked -p victory."
}
if (-not (Test-Path -LiteralPath $Exe -PathType Leaf)) {
    throw "Title executable missing at $Exe."
}
$agilityInstaller = Join-Path $repoRoot 'scripts\install-agility-runtime.ps1'
if (-not (Test-Path -LiteralPath (Join-Path (Split-Path -Parent $runner) 'D3D12\D3D12Core.dll') -PathType Leaf)) {
    Write-Output 'Preparing the pinned Agility SDK 620 runtime required by older Windows D3D12 systems.'
    & $agilityInstaller -OutputDirectory (Split-Path -Parent $runner)
}
if ($DropMips -gt 0) {
    Write-Warning 'This build uses mip residency on demand. The DropMips setting has no effect.'
}

$settings = @{
    VICTORY_TITLE_EXE = (Resolve-Path -LiteralPath $Exe).ProviderPath
    VICTORY_RUNNER = $runner
    VICTORY_CONSOLE = 'series-x'
    VICTORY_RESOLUTION = $Resolution
}
if ($Borderless) { $settings.VICTORY_BORDERLESS = '1' }
if ($Water) { $settings.VICTORY_TEXTURE_BACK = '1' }
if ($NoOverlay) {
    $settings.VICTORY_OVERLAY = '0'
    $settings.VICTORY_PANEL = '0'
} elseif (-not (Test-Path Env:VICTORY_OVERLAY)) {
    $settings.VICTORY_OVERLAY = '1'
}

$saved = @{}
try {
    foreach ($key in $settings.Keys) {
        $saved[$key] = [Environment]::GetEnvironmentVariable($key)
        [Environment]::SetEnvironmentVariable($key, $settings[$key])
    }
    & (Join-Path $repoRoot 'Play-VIctory.cmd') pareto-residency
} finally {
    foreach ($key in $saved.Keys) {
        [Environment]::SetEnvironmentVariable($key, $saved[$key])
    }
}
