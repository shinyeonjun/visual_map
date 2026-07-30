[CmdletBinding()]
param(
    [string]$SourceRoot,
    [string]$DestinationRoot,
    [switch]$VerifyOnly
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.IO.Compression.FileSystem
Add-Type -AssemblyName System.IO.Compression

$repoRoot = Split-Path -Parent $PSScriptRoot
if ([string]::IsNullOrWhiteSpace($SourceRoot)) {
    $SourceRoot = $env:VISUAL_MAP_PROVIDERS_ROOT
}
if ([string]::IsNullOrWhiteSpace($SourceRoot)) {
    $SourceRoot = Join-Path $repoRoot "code_memory\providers"
}
if ([string]::IsNullOrWhiteSpace($DestinationRoot)) {
    $DestinationRoot = Join-Path $repoRoot "src-tauri\engines\providers"
}

$SourceRoot = [IO.Path]::GetFullPath($SourceRoot)
$DestinationRoot = [IO.Path]::GetFullPath($DestinationRoot)
$BundleRoot = [IO.Path]::GetFullPath((Join-Path $repoRoot "src-tauri\engines\provider-bundles"))

function Compress-ProviderDirectory([string]$SourceDirectory, [string]$ArchivePath) {
    $parent = Split-Path -Parent $SourceDirectory
    $zip = [IO.Compression.ZipFile]::Open(
        $ArchivePath,
        [IO.Compression.ZipArchiveMode]::Create
    )
    try {
        foreach ($file in @(Get-ChildItem -LiteralPath $SourceDirectory -File -Recurse -Force)) {
            $relativePath = $file.FullName.Substring($parent.Length + 1).Replace("\", "/")
            [IO.Compression.ZipFileExtensions]::CreateEntryFromFile(
                $zip,
                $file.FullName,
                $relativePath,
                [IO.Compression.CompressionLevel]::Fastest
            ) | Out-Null
        }
    } finally {
        $zip.Dispose()
    }
}

function Get-Sha256([string]$Path) {
    $stream = [IO.File]::Open($Path, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
    $algorithm = [Security.Cryptography.SHA256]::Create()
    try {
        return ([BitConverter]::ToString($algorithm.ComputeHash($stream))).Replace("-", "")
    } finally {
        $algorithm.Dispose()
        $stream.Dispose()
    }
}

if (-not (Test-Path -LiteralPath $SourceRoot -PathType Container)) {
    throw "Provider source directory was not found: $SourceRoot"
}

$requiredFiles = @(
    "manifest.json",
    "checksums.json",
    "node\project-model.cjs",
    "node\runtime\node.exe"
)
foreach ($relativePath in $requiredFiles) {
    $path = Join-Path $SourceRoot $relativePath
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "Required provider file was not found: $path"
    }
}

if ($VerifyOnly) {
    if (-not (Test-Path -LiteralPath $DestinationRoot -PathType Container)) {
        throw "Staged provider directory was not found: $DestinationRoot"
    }
    foreach ($relativePath in $requiredFiles) {
        $path = Join-Path $DestinationRoot $relativePath
        if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
            throw "Required staged provider file was not found: $path"
        }
    }
    if (-not (Test-Path -LiteralPath $BundleRoot -PathType Container)) {
        throw "Provider bundle directory was not found: $BundleRoot"
    }
    $bundleManifestPath = Join-Path $BundleRoot "providers-manifest.json"
    if (-not (Test-Path -LiteralPath $bundleManifestPath -PathType Leaf)) {
        throw "Provider bundle manifest was not found: $bundleManifestPath"
    }
    $bundleManifest = Get-Content -LiteralPath $bundleManifestPath -Raw | ConvertFrom-Json
    foreach ($archive in @($bundleManifest.archives)) {
        $archivePath = Join-Path $BundleRoot ([string]$archive.fileName)
        if (-not (Test-Path -LiteralPath $archivePath -PathType Leaf)) {
            throw "Provider bundle archive was not found: $archivePath"
        }
        $actualHash = Get-Sha256 $archivePath
        if ($actualHash -ne [string]$archive.sha256) {
            throw "Provider bundle checksum mismatch: $archivePath"
        }
    }
    Write-Host "Verified staged providers: $DestinationRoot"
    exit 0
}

New-Item -ItemType Directory -Path $DestinationRoot -Force | Out-Null
$reparseDirectories = @(
    Get-ChildItem -LiteralPath $SourceRoot -Directory -Recurse -Force -Attributes ReparsePoint |
        ForEach-Object { $_.FullName }
)
if ($reparseDirectories.Count -gt 0) {
    Write-Warning "Skipping $($reparseDirectories.Count) provider junction(s); staged resources must be real files."
}

$robocopyArguments = @(
    $SourceRoot, $DestinationRoot,
    "/E", "/COPY:DAT", "/DCOPY:DAT", "/R:1", "/W:1", "/NP", "/NFL", "/NDL"
)
foreach ($directory in $reparseDirectories) {
    $robocopyArguments += @("/XD", $directory)
}
& robocopy @robocopyArguments
$exitCode = $LASTEXITCODE
if ($exitCode -gt 7) {
    throw "Provider staging failed with robocopy exit code $exitCode"
}

if (Test-Path -LiteralPath $BundleRoot) {
    Remove-Item -LiteralPath $BundleRoot -Recurse -Force
}
New-Item -ItemType Directory -Path $BundleRoot -Force | Out-Null
$archives = @()
foreach ($providerDirectory in @(Get-ChildItem -LiteralPath $DestinationRoot -Directory -Force | Sort-Object Name)) {
    $archiveName = "providers-$($providerDirectory.Name).zip"
    $archivePath = Join-Path $BundleRoot $archiveName
    Write-Host "Compressing provider: $($providerDirectory.Name)"
    Compress-ProviderDirectory $providerDirectory.FullName $archivePath
    $archives += [pscustomobject]@{
        fileName = $archiveName
        sha256 = Get-Sha256 $archivePath
    }
}
$manifest = [pscustomobject]@{
    schemaVersion = 1
    archives = $archives
}
$manifestJson = $manifest | ConvertTo-Json -Depth 5
[IO.File]::WriteAllText(
    (Join-Path $BundleRoot "providers-manifest.json"),
    $manifestJson,
    [Text.UTF8Encoding]::new($false)
)

Write-Host "Staged providers from $SourceRoot to $DestinationRoot"
Write-Host "Created $($archives.Count) compressed provider bundles: $BundleRoot"
