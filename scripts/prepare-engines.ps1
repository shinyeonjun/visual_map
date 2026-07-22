[CmdletBinding()]
param(
    [switch]$VerifyOnly,
    [switch]$AllowDevelopmentArtifact,
    [switch]$Release,
    [switch]$Force,
    [string]$ManifestPath,
    [string]$DestinationPath
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if ($VerifyOnly -and $Force) {
    throw "-VerifyOnly and -Force cannot be used together."
}

$repoRoot = Split-Path -Parent $PSScriptRoot
if ([string]::IsNullOrWhiteSpace($ManifestPath)) {
    $ManifestPath = Join-Path $repoRoot "src-tauri\engines\manifest.json"
}
if ([string]::IsNullOrWhiteSpace($DestinationPath)) {
    $DestinationPath = Join-Path $repoRoot "src-tauri\engines"
}

$ManifestPath = [IO.Path]::GetFullPath($ManifestPath)
$DestinationPath = [IO.Path]::GetFullPath($DestinationPath)
$manifest = Get-Content -LiteralPath $ManifestPath -Raw | ConvertFrom-Json

if ($manifest.schemaVersion -ne 1) {
    throw "Unsupported engine manifest schema: $($manifest.schemaVersion)"
}

function Get-Sha256([string]$Path) {
    # Keep this release check usable in stripped-down Windows PowerShell environments where
    # Get-FileHash is not available through module autoloading.
    $stream = [IO.File]::Open($Path, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
    $algorithm = [Security.Cryptography.SHA256]::Create()
    try {
        ([BitConverter]::ToString($algorithm.ComputeHash($stream))).Replace("-", "")
    }
    finally {
        $algorithm.Dispose()
        $stream.Dispose()
    }
}

function Test-SafeLeafName([string]$Value, [string]$Field) {
    if ([string]::IsNullOrWhiteSpace($Value) -or [IO.Path]::GetFileName($Value) -ne $Value) {
        throw "Engine manifest field '$Field' must be a file name without a path."
    }
}

function Test-SafeArchiveEntry([string]$Value) {
    if ([string]::IsNullOrWhiteSpace($Value) -or [IO.Path]::IsPathRooted($Value)) {
        throw "Engine archive entry must be a relative path."
    }
    $segments = $Value -split "[\\/]"
    if ($segments -contains "..") {
        throw "Engine archive entry cannot traverse outside the archive root."
    }
}

function Get-ArtifactState($Engine, [string]$Path) {
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        return [pscustomobject]@{ Kind = "missing"; Hash = $null }
    }

    $hash = Get-Sha256 $Path
    if ($hash -eq $Engine.executable.sha256.ToUpperInvariant()) {
        return [pscustomobject]@{ Kind = "release"; Hash = $hash }
    }

    foreach ($artifact in @($Engine.developmentArtifacts)) {
        if ($hash -eq $artifact.sha256.ToUpperInvariant()) {
            return [pscustomobject]@{ Kind = "development"; Hash = $hash }
        }
    }

    [pscustomobject]@{ Kind = "mismatch"; Hash = $hash }
}

$engines = @($manifest.engines)
if ($engines.Count -eq 0) {
    throw "Engine manifest contains no engines."
}

foreach ($engine in $engines) {
    Test-SafeLeafName $engine.archive.fileName "$($engine.id).archive.fileName"
    Test-SafeLeafName $engine.executable.fileName "$($engine.id).executable.fileName"
    Test-SafeArchiveEntry $engine.archive.entry
    if ($engine.archive.sha256 -notmatch "^[0-9A-Fa-f]{64}$" -or $engine.executable.sha256 -notmatch "^[0-9A-Fa-f]{64}$") {
        throw "Engine '$($engine.id)' has an invalid SHA-256 value."
    }
    if ($Release -and $engine.releaseReady -ne $true) {
        throw "Engine '$($engine.id)' is not marked releaseReady in the manifest."
    }
}

if ($VerifyOnly) {
    foreach ($engine in $engines) {
        $target = Join-Path $DestinationPath $engine.executable.fileName
        $state = Get-ArtifactState $engine $target
        if ($state.Kind -eq "release") {
            if ($engine.releaseReady -ne $true -and -not $AllowDevelopmentArtifact) {
                throw "Engine '$($engine.id)' is an unpublished pinned artifact. Use -AllowDevelopmentArtifact for local/internal validation."
            }
            if ($engine.releaseReady -ne $true) {
                Write-Warning "Verified unpublished pinned artifact for $($engine.id): $target. It is not releasable."
                continue
            }
            Write-Host "Verified release engine $($engine.id) $($engine.version): $target"
            continue
        }
        if ($state.Kind -eq "development" -and $AllowDevelopmentArtifact) {
            Write-Warning "Verified declared development artifact for $($engine.id): $target. It is not releasable."
            continue
        }
        if ($state.Kind -eq "missing") {
            throw "Missing engine '$($engine.id)': $target"
        }
        throw "Engine '$($engine.id)' checksum is not the pinned release checksum: $($state.Hash)"
    }
    return
}

New-Item -ItemType Directory -Path $DestinationPath -Force | Out-Null
$tempRoot = Join-Path ([IO.Path]::GetTempPath()) ("backend-visual-map-engines-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $tempRoot | Out-Null

try {
    foreach ($engine in $engines) {
        $target = Join-Path $DestinationPath $engine.executable.fileName
        $state = Get-ArtifactState $engine $target
        if ($state.Kind -eq "release") {
            if ($engine.releaseReady -ne $true -and -not $AllowDevelopmentArtifact) {
                throw "Engine '$($engine.id)' is not published. Supply its pinned artifact and use -AllowDevelopmentArtifact."
            }
            if ($engine.releaseReady -ne $true) {
                Write-Warning "Keeping unpublished pinned artifact for $($engine.id). It cannot be used for a public release."
                continue
            }
            Write-Host "Release engine already prepared: $($engine.id) $($engine.version)"
            continue
        }
        if ($state.Kind -eq "development" -and $AllowDevelopmentArtifact -and -not $Force) {
            Write-Warning "Keeping declared development artifact for $($engine.id). Use -Force to replace it with the release artifact."
            continue
        }
        if ($state.Kind -ne "missing" -and -not $Force) {
            throw "Refusing to replace '$target' without -Force. Current SHA-256: $($state.Hash)"
        }
        if ($engine.releaseReady -ne $true) {
            throw "Engine '$($engine.id)' has no published archive yet. Build source commit $($engine.sourceCommit), place '$($engine.executable.fileName)' in the engine directory, and use -AllowDevelopmentArtifact."
        }

        $engineTemp = Join-Path $tempRoot $engine.id
        $extractPath = Join-Path $engineTemp "extract"
        $archivePath = Join-Path $engineTemp $engine.archive.fileName
        New-Item -ItemType Directory -Path $engineTemp,$extractPath -Force | Out-Null

        Write-Host "Downloading $($engine.id) $($engine.version)..."
        Invoke-WebRequest -UseBasicParsing -Uri $engine.archive.url -OutFile $archivePath
        $archiveHash = Get-Sha256 $archivePath
        if ($archiveHash -ne $engine.archive.sha256.ToUpperInvariant()) {
            throw "Archive checksum mismatch for '$($engine.id)': $archiveHash"
        }

        Expand-Archive -LiteralPath $archivePath -DestinationPath $extractPath -Force
        $source = Join-Path $extractPath $engine.archive.entry
        if (-not (Test-Path -LiteralPath $source -PathType Leaf)) {
            throw "Archive for '$($engine.id)' does not contain '$($engine.archive.entry)'."
        }
        $executableHash = Get-Sha256 $source
        if ($executableHash -ne $engine.executable.sha256.ToUpperInvariant()) {
            throw "Executable checksum mismatch for '$($engine.id)': $executableHash"
        }

        $staged = "$target.new-$([guid]::NewGuid().ToString('N'))"
        Copy-Item -LiteralPath $source -Destination $staged
        Move-Item -LiteralPath $staged -Destination $target -Force
        Write-Host "Prepared release engine $($engine.id) $($engine.version): $target"
    }
}
finally {
    $tempBase = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
    $resolvedTempRoot = [IO.Path]::GetFullPath($tempRoot)
    if ($resolvedTempRoot.StartsWith($tempBase, [StringComparison]::OrdinalIgnoreCase)) {
        Remove-Item -LiteralPath $resolvedTempRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}
