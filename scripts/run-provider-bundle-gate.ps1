[CmdletBinding()]
param(
    [string]$Bridge,
    [switch]$Release,
    [switch]$KeepExtracted
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$repoRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))
if ([string]::IsNullOrWhiteSpace($Bridge)) {
    $Bridge = Join-Path $repoRoot "code_memory\rust\target\release\code-memory-language.exe"
}
$Bridge = [IO.Path]::GetFullPath($Bridge)
$bundleRoot = Join-Path $repoRoot "src-tauri\engines\provider-bundles"
$catalogPath = Join-Path $bundleRoot "providers-manifest.json"
$signaturePath = Join-Path $bundleRoot "providers-manifest.sig"
$tempBase = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
$gateRoot = Join-Path $tempBase ("codebase-workspace-provider-gate-" + [guid]::NewGuid().ToString("N"))
$providersRoot = Join-Path $gateRoot "providers"
$cacheRoot = Join-Path $gateRoot "cache"

function Get-Sha256([string]$Path) {
    $stream = [IO.File]::Open($Path, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
    $algorithm = [Security.Cryptography.SHA256]::Create()
    try {
        return ([BitConverter]::ToString($algorithm.ComputeHash($stream))).Replace("-", "").ToLowerInvariant()
    } finally {
        $algorithm.Dispose()
        $stream.Dispose()
    }
}

if (-not (Test-Path -LiteralPath $Bridge -PathType Leaf)) {
    throw "Release code engine was not found: $Bridge"
}
if (-not (Test-Path -LiteralPath $catalogPath -PathType Leaf) -or
    -not (Test-Path -LiteralPath $signaturePath -PathType Leaf)) {
    throw "Signed provider catalog was not found: $bundleRoot"
}

$publicKey = if ($Release) {
    if ([string]::IsNullOrWhiteSpace($env:CODEBASE_WORKSPACE_PROVIDER_CATALOG_PUBLIC_KEY)) {
        throw "Release provider gates require CODEBASE_WORKSPACE_PROVIDER_CATALOG_PUBLIC_KEY."
    }
    $env:CODEBASE_WORKSPACE_PROVIDER_CATALOG_PUBLIC_KEY.Trim()
} else {
    "IVL40Zt5HSRFMkLhXy6rbLfP+ntqXtMAl5YOBpiB2xI="
}
& cargo run --quiet --locked --manifest-path (Join-Path $repoRoot "src-tauri\Cargo.toml") `
    --bin provider-catalog-sign -- verify --catalog $catalogPath --signature $signaturePath --public-key $publicKey
if ($LASTEXITCODE -ne 0) {
    throw "Provider catalog verification failed with exit code $LASTEXITCODE."
}

$previousCacheRoot = $env:CODE_MEMORY_CACHE_ROOT
$previousManagedPolicy = $env:CODE_MEMORY_REQUIRE_MANAGED_PROVIDERS
$previousOffline = $env:CODE_MEMORY_OFFLINE
try {
    New-Item -ItemType Directory -Path $providersRoot,$cacheRoot -Force | Out-Null
    $catalog = Get-Content -LiteralPath $catalogPath -Raw | ConvertFrom-Json
    $expectedLanguages = [Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
    foreach ($language in "typescript", "javascript", "python", "java", "csharp", "c", "cpp", "go", "rust", "dart") {
        [void]$expectedLanguages.Add($language)
    }
    $actualLanguages = [Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)

    foreach ($pack in @($catalog.packs)) {
        $archivePath = Join-Path $bundleRoot ([string]$pack.fileName)
        if (-not (Test-Path -LiteralPath $archivePath -PathType Leaf)) {
            throw "Reliability gate requires the local archive: $archivePath"
        }
        if ((Get-Item -LiteralPath $archivePath).Length -ne [uint64]$pack.compressedBytes -or
            (Get-Sha256 $archivePath) -ne ([string]$pack.sha256).ToLowerInvariant()) {
            throw "Provider archive checksum mismatch: $archivePath"
        }
        & tar.exe -xf $archivePath -C $providersRoot
        if ($LASTEXITCODE -ne 0) {
            throw "Provider archive extraction failed: $archivePath"
        }
        foreach ($language in @($pack.languages)) {
            if (-not $actualLanguages.Add([string]$language)) {
                throw "Provider language is assigned more than once: $language"
            }
        }
        foreach ($entryPoint in @($pack.entrypoints)) {
            $entryPath = Join-Path $providersRoot ([string]$entryPoint.path)
            if (-not (Test-Path -LiteralPath $entryPath -PathType Leaf) -or
                (Get-Item -LiteralPath $entryPath).Length -ne [uint64]$entryPoint.bytes -or
                (Get-Sha256 $entryPath) -ne ([string]$entryPoint.sha256).ToLowerInvariant()) {
                throw "Extracted provider entrypoint verification failed: $entryPath"
            }
        }
    }

    $languageDiff = @(Compare-Object -ReferenceObject @($expectedLanguages | Sort-Object) -DifferenceObject @($actualLanguages | Sort-Object))
    if ($languageDiff.Count -gt 0) {
        throw "Provider bundle language coverage does not match the 10-language contract."
    }
    $providerManifest = Get-Content -LiteralPath (Join-Path $providersRoot "manifest.json") -Raw | ConvertFrom-Json
    foreach ($provider in @($providerManifest.providers)) {
        $providerPath = Join-Path $providersRoot ([string]$provider.path)
        if (-not (Test-Path -LiteralPath $providerPath -PathType Leaf)) {
            throw "Provider manifest entrypoint is missing after extraction: $providerPath"
        }
    }

    $env:CODE_MEMORY_CACHE_ROOT = $cacheRoot
    $env:CODE_MEMORY_REQUIRE_MANAGED_PROVIDERS = "1"
    $env:CODE_MEMORY_OFFLINE = "1"
    $doctor = @(& $Bridge doctor --providers-root $providersRoot 2>&1)
    if ($LASTEXITCODE -ne 0 -or @($doctor | Select-String "\tMISSING\t").Count -gt 0) {
        throw "Extracted provider doctor failed:`n$($doctor -join [Environment]::NewLine)"
    }
    & (Join-Path $repoRoot "code_memory\tests\gates\run-uniform-core-quality-gate.ps1") `
        -Bridge $Bridge `
        -ProvidersRoot $providersRoot
    if ($LASTEXITCODE -ne 0) {
        throw "10-language canonical provider quality gate failed with exit code $LASTEXITCODE."
    }
    & (Join-Path $repoRoot "scripts\smoke-code-determinism.ps1") `
        -EnginePath $Bridge `
        -ProvidersRoot $providersRoot `
        -PacksRoot (Join-Path $repoRoot "code_memory")
    if ($LASTEXITCODE -ne 0) {
        throw "Canonical Fact determinism gate failed with exit code $LASTEXITCODE."
    }
    Write-Host "Provider bundle gate passed: packs=$(@($catalog.packs).Count) languages=$($actualLanguages.Count)"
} finally {
    $env:CODE_MEMORY_CACHE_ROOT = $previousCacheRoot
    $env:CODE_MEMORY_REQUIRE_MANAGED_PROVIDERS = $previousManagedPolicy
    $env:CODE_MEMORY_OFFLINE = $previousOffline
    if (-not $KeepExtracted) {
        $resolvedGateRoot = [IO.Path]::GetFullPath($gateRoot)
        if ($resolvedGateRoot.StartsWith($tempBase, [StringComparison]::OrdinalIgnoreCase)) {
            Remove-Item -LiteralPath $resolvedGateRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    } else {
        Write-Host "Kept extracted provider gate at $gateRoot"
    }
}
