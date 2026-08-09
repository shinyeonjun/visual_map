[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))
$manifestPath = Join-Path $repoRoot "src-tauri\engines\manifest.json"
$sourceEngine = Join-Path $repoRoot "code_memory\rust\target\release\code-memory-language.exe"
$bundledEngine = Join-Path $repoRoot "src-tauri\engines\code-memory-language.exe"
$contractVerifier = Join-Path $PSScriptRoot "verify-code-engine-contract.ps1"

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

Push-Location $repoRoot
try {
    & cargo +1.96.1 build --locked --release --manifest-path .\code_memory\rust\Cargo.toml
    if ($LASTEXITCODE -ne 0) {
        throw "Development code engine build failed with exit code $LASTEXITCODE."
    }

    & $contractVerifier -EnginePath $sourceEngine
    if ($LASTEXITCODE -ne 0) {
        throw "Development code engine contract verification failed with exit code $LASTEXITCODE."
    }

    $sourceHash = Get-Sha256 $sourceEngine
    $bundledHash = if (Test-Path -LiteralPath $bundledEngine -PathType Leaf) {
        Get-Sha256 $bundledEngine
    } else {
        $null
    }
    if ($sourceHash -ne $bundledHash) {
        $stagedEngine = "$bundledEngine.new-$([guid]::NewGuid().ToString('N'))"
        try {
            Copy-Item -LiteralPath $sourceEngine -Destination $stagedEngine
            Move-Item -LiteralPath $stagedEngine -Destination $bundledEngine -Force
        } finally {
            if (Test-Path -LiteralPath $stagedEngine) {
                Remove-Item -LiteralPath $stagedEngine -Force -ErrorAction SilentlyContinue
            }
        }
        Write-Output "Updated development code engine: $bundledEngine"
    } else {
        Write-Output "Development code engine is current: $bundledEngine"
    }

    $manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
    $entries = @($manifest.engines | Where-Object id -eq "codebase-memory")
    if ($entries.Count -ne 1) {
        throw "Engine manifest must contain exactly one codebase-memory entry."
    }
    $entry = $entries[0]
    $developmentArtifacts = @($entry.developmentArtifacts)
    $manifestNeedsUpdate =
        [string]$entry.version -ne "0.1.0" -or
        [string]$entry.contractVersion -ne "3" -or
        $developmentArtifacts.Count -ne 1 -or
        [string]$developmentArtifacts[0].sha256 -ne $sourceHash
    if ($manifestNeedsUpdate) {
        $entry.version = "0.1.0"
        $entry.contractVersion = "3"
        $entry.developmentArtifacts = @([pscustomobject]@{ sha256 = $sourceHash })
        $normalized = ($manifest | ConvertTo-Json -Depth 12) + "`n"
        $stagedManifest = "$manifestPath.new-$([guid]::NewGuid().ToString('N'))"
        try {
            [IO.File]::WriteAllText($stagedManifest, $normalized, [Text.UTF8Encoding]::new($false))
            Move-Item -LiteralPath $stagedManifest -Destination $manifestPath -Force
        } finally {
            if (Test-Path -LiteralPath $stagedManifest) {
                Remove-Item -LiteralPath $stagedManifest -Force -ErrorAction SilentlyContinue
            }
        }
        $prettier = Join-Path $repoRoot "node_modules\.bin\prettier.cmd"
        if (-not (Test-Path -LiteralPath $prettier -PathType Leaf)) {
            throw "Local Prettier executable was not found: $prettier"
        }
        & $prettier --write --end-of-line lf $manifestPath
        if ($LASTEXITCODE -ne 0) {
            throw "Engine manifest formatting failed with exit code $LASTEXITCODE."
        }
        Write-Output "Updated development engine receipt: $manifestPath"
    }

    if ((Get-Sha256 $bundledEngine) -ne $sourceHash) {
        throw "Bundled development code engine hash does not match the source build."
    }
    & $contractVerifier -EnginePath $bundledEngine
    if ($LASTEXITCODE -ne 0) {
        throw "Bundled development code engine contract verification failed with exit code $LASTEXITCODE."
    }
} finally {
    Pop-Location
}
