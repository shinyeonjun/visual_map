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

function Compress-ProviderRootFiles([string]$SourceDirectory, [string]$ArchivePath, [string[]]$FileNames) {
    $zip = [IO.Compression.ZipFile]::Open(
        $ArchivePath,
        [IO.Compression.ZipArchiveMode]::Create
    )
    try {
        foreach ($fileName in $FileNames) {
            $sourcePath = Join-Path $SourceDirectory $fileName
            [IO.Compression.ZipFileExtensions]::CreateEntryFromFile(
                $zip,
                $sourcePath,
                $fileName.Replace("\", "/"),
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
    $bundledEntries = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
    foreach ($archive in @($bundleManifest.archives)) {
        $archivePath = Join-Path $BundleRoot ([string]$archive.fileName)
        if (-not (Test-Path -LiteralPath $archivePath -PathType Leaf)) {
            throw "Provider bundle archive was not found: $archivePath"
        }
        $actualHash = Get-Sha256 $archivePath
        if ($actualHash -ne [string]$archive.sha256) {
            throw "Provider bundle checksum mismatch: $archivePath"
        }
        $zip = [IO.Compression.ZipFile]::OpenRead($archivePath)
        try {
            foreach ($entry in $zip.Entries) {
                [void]$bundledEntries.Add($entry.FullName.Replace("\", "/"))
            }
        } finally {
            $zip.Dispose()
        }
    }
    foreach ($requiredEntry in "manifest.json", "checksums.json") {
        if (-not $bundledEntries.Contains($requiredEntry)) {
            throw "Provider bundles do not contain required root metadata: $requiredEntry"
        }
    }
    $providerManifest = Get-Content -LiteralPath (Join-Path $DestinationRoot "manifest.json") -Raw | ConvertFrom-Json
    foreach ($provider in @($providerManifest.providers)) {
        $providerPath = ([string]$provider.path).Replace("\", "/")
        if ([string]::IsNullOrWhiteSpace($providerPath) -or $providerPath -match "(^/|\.\.)") {
            throw "Provider manifest contains an unsafe path: $providerPath"
        }
        if (-not (Test-Path -LiteralPath (Join-Path $DestinationRoot $providerPath) -PathType Leaf)) {
            throw "Staged provider executable is missing: $providerPath"
        }
        if (-not $bundledEntries.Contains($providerPath)) {
            throw "Provider bundles do not contain declared executable: $providerPath"
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
$rootMetadata = @("manifest.json", "checksums.json")
if (Test-Path -LiteralPath (Join-Path $DestinationRoot "README.md") -PathType Leaf) {
    $rootMetadata += "README.md"
}
$coreArchiveName = "providers-core.zip"
$coreArchivePath = Join-Path $BundleRoot $coreArchiveName
Write-Host "Compressing provider root metadata"
Compress-ProviderRootFiles $DestinationRoot $coreArchivePath $rootMetadata
$archives = @([pscustomobject]@{
    fileName = $coreArchiveName
    sha256 = Get-Sha256 $coreArchivePath
})
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
