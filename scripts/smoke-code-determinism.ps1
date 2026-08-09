[CmdletBinding()]
param(
    [string]$EnginePath,
    [string]$ProvidersRoot,
    [string]$PacksRoot,
    [switch]$KeepFixture
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $PSScriptRoot

if ([string]::IsNullOrWhiteSpace($EnginePath)) {
    $EnginePath = Join-Path $repoRoot "src-tauri\engines\code-memory-language.exe"
}
$EnginePath = [IO.Path]::GetFullPath($EnginePath)
if (-not (Test-Path -LiteralPath $EnginePath -PathType Leaf)) {
    throw "Code engine not found: $EnginePath"
}

$engineRoot = Split-Path -Parent $EnginePath
if ([string]::IsNullOrWhiteSpace($ProvidersRoot)) {
    $candidate = Join-Path $engineRoot "providers"
    if (Test-Path -LiteralPath $candidate -PathType Container) {
        $ProvidersRoot = $candidate
    } else {
        $candidate = Join-Path $repoRoot "code_memory\providers"
        if (Test-Path -LiteralPath $candidate -PathType Container) {
            $ProvidersRoot = $candidate
        }
    }
}
if ([string]::IsNullOrWhiteSpace($PacksRoot)) {
    $candidate = Join-Path $engineRoot "packs"
    if (Test-Path -LiteralPath (Join-Path $candidate "framework") -PathType Container) {
        # The bridge receives the directory that contains packs/, not packs/
        # itself. This keeps --packs-root consistent with the other gates.
        $PacksRoot = $engineRoot
    } else {
        $PacksRoot = Join-Path $repoRoot "code_memory"
    }
}

$tempBase = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
$fixtureRoot = Join-Path $tempBase ("codebase-workspace-determinism-" + [guid]::NewGuid().ToString("N"))
$cacheOne = Join-Path $fixtureRoot "cache-one"
$cacheTwo = Join-Path $fixtureRoot "cache-two"
$sourceRoot = Join-Path $fixtureRoot "repo"
New-Item -ItemType Directory -Path $sourceRoot -Force | Out-Null

function Remove-SafeFixtureTree([string]$Path) {
    $resolved = [IO.Path]::GetFullPath($Path)
    $tempPrefix = $tempBase.TrimEnd([IO.Path]::DirectorySeparatorChar) + [IO.Path]::DirectorySeparatorChar
    if (-not $resolved.StartsWith($tempPrefix, [StringComparison]::OrdinalIgnoreCase) -or
        -not ([IO.Path]::GetFileName($resolved)).StartsWith('codebase-workspace-determinism-', [StringComparison]::Ordinal)) {
        throw "Refusing to remove a path outside the determinism temp scope: $resolved"
    }
    if (-not [IO.Directory]::Exists($resolved)) {
        return
    }
    $deletePath = if ($env:OS -eq 'Windows_NT' -and -not $resolved.StartsWith('\\?\', [StringComparison]::Ordinal)) {
        '\\?\' + $resolved
    } else {
        $resolved
    }
    [IO.Directory]::Delete($deletePath, $true)
}

try {
    New-Item -ItemType Directory -Path (Join-Path $sourceRoot "src") -Force | Out-Null
    [IO.File]::WriteAllText(
        (Join-Path $sourceRoot "src\callee.ts"),
        "export function add(left: number, right: number): number { return left + right; }`n",
        [Text.UTF8Encoding]::new($false)
    )
    [IO.File]::WriteAllText(
        (Join-Path $sourceRoot "src\caller.ts"),
        "import { add } from './callee';`nexport function total(): number { return add(1, 2); }`n",
        [Text.UTF8Encoding]::new($false)
    )
    [IO.File]::WriteAllText(
        (Join-Path $sourceRoot "tsconfig.json"),
        '{"compilerOptions":{"module":"ESNext","moduleResolution":"Bundler","strict":true},"include":["src/**/*.ts"]}',
        [Text.UTF8Encoding]::new($false)
    )

    function Invoke-Index([string]$CacheRoot) {
        $env:CODE_MEMORY_CACHE_ROOT = $CacheRoot
        $env:CODE_MEMORY_STRICT = "1"
        $arguments = @(
            "index",
            "--root", $sourceRoot,
            "--packs-root", ([IO.Path]::GetFullPath($PacksRoot))
        )
        if (-not [string]::IsNullOrWhiteSpace($ProvidersRoot)) {
            $arguments += @("--providers-root", ([IO.Path]::GetFullPath($ProvidersRoot)))
        }
        $previousPreference = $ErrorActionPreference
        $ErrorActionPreference = "Continue"
        try {
            $engineLines = @(& $EnginePath @arguments 2>&1)
            $engineLines | ForEach-Object { Write-Host ([string]$_) }
            $engineExitCode = $LASTEXITCODE
        } finally {
            $ErrorActionPreference = $previousPreference
        }
        if ($engineExitCode -ne 0) {
            throw "Determinism index run failed with exit code $engineExitCode"
        }
        $prefix = "@codebase-workspace-canonical-fact-bundle "
        $marker = @($engineLines | ForEach-Object { [string]$_ } | Where-Object {
            $_.StartsWith($prefix, [StringComparison]::Ordinal)
        }) | Select-Object -Last 1
        if ([string]::IsNullOrWhiteSpace($marker)) {
            throw "Determinism index run did not publish a canonical Fact bundle receipt"
        }
        $artifact = $marker.Substring($prefix.Length) | ConvertFrom-Json
        if ($artifact.schema -ne "codebase-workspace.canonical-fact-bundle-artifact.v1" -or
            -not (Test-Path -LiteralPath $artifact.bundlePath -PathType Leaf) -or
            -not (Test-Path -LiteralPath $artifact.manifestPath -PathType Leaf)) {
            throw "Determinism index run published an invalid canonical Fact artifact"
        }
        $manifest = Get-Content -LiteralPath $artifact.manifestPath -Raw | ConvertFrom-Json
        [pscustomobject]@{
            Artifact = $artifact
            Manifest = $manifest
            BundleHash = (Get-FileHash -LiteralPath $artifact.bundlePath -Algorithm SHA256).Hash
        }
    }

    $first = Invoke-Index $cacheOne
    $second = Invoke-Index $cacheTwo
    $stableManifestFields = @(
        "snapshotId", "sourceManifestDigest", "configDigest", "analysisPlanDigest",
        "providerSetDigest", "executionContextSetDigest", "semanticDigest", "bundleDigest",
        "analysisUnitReceiptCount", "nodeCount", "edgeCount", "evidenceCount",
        "fileCoverageCount", "sourceScopeCoverageCount", "capabilityReceiptCount",
        "gapCount", "issueCount"
    )
    foreach ($field in $stableManifestFields) {
        if ($first.Manifest.$field -ne $second.Manifest.$field) {
            throw "Canonical Fact manifest field changed between identical runs: $field"
        }
    }
    if ($first.BundleHash -ne $second.BundleHash) {
        throw "Canonical SQLite bundle bytes changed between identical independent runs."
    }

    Write-Host "PASS deterministic canonical Fact bundles"
}
finally {
    if (-not $KeepFixture -and (Test-Path -LiteralPath $fixtureRoot)) {
        Remove-SafeFixtureTree $fixtureRoot
    } elseif ($KeepFixture) {
        Write-Host "Kept determinism fixture: $fixtureRoot"
    }
}
