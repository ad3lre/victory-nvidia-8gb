param(
    [string]$OutputDirectory = (Join-Path (Split-Path -Parent $PSScriptRoot) 'target\release'),
    [switch]$Force,
    [switch]$Check
)
$ErrorActionPreference = 'Stop'
$version = '1.620.2'
$packageId = 'microsoft.direct3d.d3d12'
$baseUri = "https://api.nuget.org/v3-flatcontainer/$packageId/$version/$packageId.$version.nupkg"
$coreEntry = 'build/native/bin/x64/D3D12Core.dll'
$destination = [IO.Path]::GetFullPath($OutputDirectory)
$runtimeDirectory = Join-Path $destination 'D3D12'
if ($Check) {
    Write-Output "package=$packageId $version"
    Write-Output "package_uri=$baseUri"
    Write-Output "runtime=$runtimeDirectory\D3D12Core.dll"
    Write-Output 'mode=check; no download or files changed'
    return
}
if (-not $Force -and (Test-Path -LiteralPath (Join-Path $runtimeDirectory 'D3D12Core.dll') -PathType Leaf)) {
    Write-Output "Agility runtime already exists: $runtimeDirectory\D3D12Core.dll (use -Force to replace it)"
    return
}

[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
$tempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
$work = [IO.Path]::GetFullPath((Join-Path $tempRoot ('victory-agility-' + [Guid]::NewGuid().ToString('N'))))
if (-not $work.StartsWith($tempRoot, [StringComparison]::OrdinalIgnoreCase)) {
    throw "Refusing temporary path outside the OS temp directory: $work"
}
New-Item -ItemType Directory -Path $work | Out-Null
$packagePath = Join-Path $work "$packageId.$version.nupkg"
$hashPath = "$packagePath.sha512"
try {
    Invoke-WebRequest -UseBasicParsing -Uri $baseUri -OutFile $packagePath
    Invoke-WebRequest -UseBasicParsing -Uri "$baseUri.sha512" -OutFile $hashPath
    $expectedHash = (Get-Content -LiteralPath $hashPath -Raw).Trim()
    $sha = [Security.Cryptography.SHA512]::Create()
    try {
        $actualHash = [Convert]::ToBase64String($sha.ComputeHash([IO.File]::ReadAllBytes($packagePath)))
    } finally { $sha.Dispose() }
    if ($actualHash -cne $expectedHash) { throw 'NuGet package SHA-512 did not match the NuGet flat-container checksum.' }

    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $archive = [IO.Compression.ZipFile]::OpenRead($packagePath)
    try {
        $spec = $archive.GetEntry('Microsoft.Direct3D.D3D12.nuspec')
        if ($null -eq $spec) { throw 'Pinned package is missing its nuspec metadata.' }
        $reader = [IO.StreamReader]::new($spec.Open())
        try { [xml]$metadata = $reader.ReadToEnd() } finally { $reader.Dispose() }
        if ($metadata.package.metadata.id -ne 'Microsoft.Direct3D.D3D12' -or $metadata.package.metadata.version -ne $version) {
            throw 'NuGet package identity/version did not match the pinned Microsoft.Direct3D.D3D12 1.620.2 runtime.'
        }
        $core = $archive.GetEntry($coreEntry)
        if ($null -eq $core) { throw "Pinned package is missing $coreEntry" }
        $license = $archive.GetEntry('LICENSE.txt')
        if ($null -eq $license) { throw 'Pinned package is missing LICENSE.txt' }
        $stagedCore = Join-Path $work 'D3D12Core.dll'
        $stagedLicense = Join-Path $work 'LICENSE.txt'
        [IO.Compression.ZipFileExtensions]::ExtractToFile($core, $stagedCore, $true)
        [IO.Compression.ZipFileExtensions]::ExtractToFile($license, $stagedLicense, $true)
    } finally { $archive.Dispose() }

    if ((Get-Item -LiteralPath $stagedCore).Length -lt 1MB) { throw 'Extracted D3D12Core.dll is unexpectedly small.' }
    New-Item -ItemType Directory -Path $runtimeDirectory -Force | Out-Null
    $newCore = Join-Path $runtimeDirectory 'D3D12Core.dll.new'
    Copy-Item -LiteralPath $stagedCore -Destination $newCore -Force
    Move-Item -LiteralPath $newCore -Destination (Join-Path $runtimeDirectory 'D3D12Core.dll') -Force
    Copy-Item -LiteralPath $stagedLicense -Destination (Join-Path $runtimeDirectory 'LICENSE.txt') -Force
    Write-Output "Installed SHA-512-verified Microsoft.Direct3D.D3D12 $version x64 runtime: $runtimeDirectory\D3D12Core.dll"
    Write-Output "D3D12Core SHA256=$((Get-FileHash -LiteralPath (Join-Path $runtimeDirectory 'D3D12Core.dll') -Algorithm SHA256).Hash)"
} finally {
    Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
}
