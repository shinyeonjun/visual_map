[CmdletBinding()]
param(
    [string]$Bridge = (Join-Path $PSScriptRoot '..\..\rust\target\release\code-memory-language.exe'),
    [string]$ProvidersRoot,
    [switch]$AllowMissingProvider,
    [switch]$KeepArtifacts
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
if (Get-Variable PSNativeCommandUseErrorActionPreference -ErrorAction SilentlyContinue) {
    $PSNativeCommandUseErrorActionPreference = $false
}

$fixturesRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\fixtures'))
$codeMemoryRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..'))
$bridgePath = [IO.Path]::GetFullPath($Bridge)
$tempBase = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
$gateRoot = Join-Path $tempBase ('codebase-workspace-canonical-language-gate-' + [guid]::NewGuid().ToString('N'))

function Remove-SafeGateTree([string]$Path) {
    $resolved = [IO.Path]::GetFullPath($Path)
    $tempPrefix = $tempBase.TrimEnd([IO.Path]::DirectorySeparatorChar) + [IO.Path]::DirectorySeparatorChar
    if (-not $resolved.StartsWith($tempPrefix, [StringComparison]::OrdinalIgnoreCase) -or
        -not ([IO.Path]::GetFileName($resolved)).StartsWith('codebase-workspace-canonical-language-gate-', [StringComparison]::Ordinal)) {
        throw "Refusing to remove a path outside the canonical language gate temp scope: $resolved"
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

if (-not (Test-Path -LiteralPath $bridgePath -PathType Leaf)) {
    throw "Bridge not found: $bridgePath"
}
if ([string]::IsNullOrWhiteSpace($ProvidersRoot)) {
    $bundledProviders = Join-Path $codeMemoryRoot 'providers'
    if (Test-Path -LiteralPath $bundledProviders -PathType Container) {
        $ProvidersRoot = $bundledProviders
    }
}

# C and C++ intentionally share one clangd fixture. The language catalogs are
# still compared independently below, so a missing language cannot hide behind
# the shared project run.
$cases = @(
    @{ Id = 'rust'; Path = 'native-lsp-rust' },
    @{ Id = 'typescript'; Path = 'scip-typescript' },
    @{ Id = 'javascript'; Path = 'scip-javascript' },
    @{ Id = 'python'; Path = 'scip-python' },
    @{ Id = 'java'; Path = 'scip-java' },
    @{ Id = 'csharp'; Path = 'scip-dotnet' },
    @{ Id = 'c-cpp'; Path = 'native-lsp-c' },
    @{ Id = 'go'; Path = 'native-lsp-go' },
    @{ Id = 'dart'; Path = 'native-lsp-dart' }
)
$contractLanguages = @('typescript', 'javascript', 'python', 'java', 'csharp', 'c', 'cpp', 'go', 'rust', 'dart')

function Assert-SameLanguageIds {
    param(
        [string]$LeftName,
        [string[]]$Left,
        [string]$RightName,
        [string[]]$Right
    )

    $diff = @(Compare-Object -ReferenceObject @($Left | Sort-Object -Unique) -DifferenceObject @($Right | Sort-Object -Unique))
    if ($diff.Count -gt 0) {
        $details = ($diff | ForEach-Object { "$($_.SideIndicator)$($_.InputObject)" }) -join ', '
        throw "Language catalog drift between ${LeftName} and ${RightName}: $details"
    }
}

function Invoke-CanonicalIndex {
    param(
        [string]$CaseId,
        [string]$SourceRoot
    )

    $cacheRoot = Join-Path $gateRoot ("cache-" + $CaseId)
    New-Item -ItemType Directory -Path $cacheRoot -Force | Out-Null
    $env:CODE_MEMORY_CACHE_ROOT = $cacheRoot
    $env:CODE_MEMORY_STRICT = '1'
    $arguments = @(
        'index',
        '--root', $SourceRoot,
        '--packs-root', $codeMemoryRoot
    )
    if (-not [string]::IsNullOrWhiteSpace($ProvidersRoot)) {
        $arguments += @('--providers-root', [IO.Path]::GetFullPath($ProvidersRoot))
    }

    $previousPreference = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try {
        $engineLines = @(& $bridgePath @arguments 2>&1)
        $exitCode = $LASTEXITCODE
    } finally {
        $ErrorActionPreference = $previousPreference
    }
    if ($exitCode -ne 0) {
        if ($AllowMissingProvider) {
            Write-Host "SKIP ${CaseId}: canonical index failed"
            return $null
        }
        throw "${CaseId}: canonical index failed with exit code $exitCode`n$($engineLines -join [Environment]::NewLine)"
    }

    $prefix = '@codebase-workspace-canonical-fact-bundle '
    $marker = @($engineLines | ForEach-Object { [string]$_ } | Where-Object {
        $_.StartsWith($prefix, [StringComparison]::Ordinal)
    }) | Select-Object -Last 1
    if ([string]::IsNullOrWhiteSpace($marker)) {
        throw "${CaseId}: canonical Fact bundle receipt was not published"
    }
    $artifact = $marker.Substring($prefix.Length) | ConvertFrom-Json
    if ($artifact.schema -ne 'codebase-workspace.canonical-fact-bundle-artifact.v1' -or
        -not (Test-Path -LiteralPath $artifact.bundlePath -PathType Leaf) -or
        -not (Test-Path -LiteralPath $artifact.manifestPath -PathType Leaf)) {
        throw "${CaseId}: canonical Fact artifact is invalid"
    }
    $manifest = Get-Content -LiteralPath $artifact.manifestPath -Raw | ConvertFrom-Json
    foreach ($field in 'snapshotId', 'sourceManifestDigest', 'semanticDigest', 'bundleDigest') {
        if ([string]::IsNullOrWhiteSpace([string]$manifest.$field)) {
            throw "${CaseId}: manifest field is empty: $field"
        }
    }
    foreach ($field in 'analysisUnitReceiptCount', 'nodeCount', 'edgeCount', 'evidenceCount', 'fileCoverageCount', 'capabilityReceiptCount') {
        if ([int64]$manifest.$field -le 0) {
            throw "${CaseId}: canonical manifest has no $field"
        }
    }
    [pscustomobject]@{ Artifact = $artifact; Manifest = $manifest }
}

$previousCacheRoot = $env:CODE_MEMORY_CACHE_ROOT
$previousStrict = $env:CODE_MEMORY_STRICT
try {
    New-Item -ItemType Directory -Path $gateRoot -Force | Out-Null

    $bridgeList = @(& $bridgePath list)
    if ($LASTEXITCODE -ne 0) {
        throw 'Bridge language listing failed'
    }
    $bridgeIds = @($bridgeList | ForEach-Object {
        if ($_ -match '^([^\t]+)\t') { $matches[1] }
    })
    Assert-SameLanguageIds -LeftName 'product contract' -Left $contractLanguages -RightName 'bridge' -Right $bridgeIds

    $catalog = Get-Content -LiteralPath (Join-Path $codeMemoryRoot 'packs\framework\catalog.json') -Raw | ConvertFrom-Json
    $packIds = @($catalog.languages | ForEach-Object id)
    Assert-SameLanguageIds -LeftName 'bridge' -Left $bridgeIds -RightName 'framework catalog' -Right $packIds

    $passed = 0
    $skipped = 0
    foreach ($case in $cases) {
        $fixture = Join-Path $fixturesRoot $case.Path
        if (-not (Test-Path -LiteralPath $fixture -PathType Container)) {
            throw "$($case.Id): fixture is missing: $fixture"
        }
        $result = Invoke-CanonicalIndex -CaseId $case.Id -SourceRoot $fixture
        if ($null -eq $result) {
            $skipped++
            continue
        }
        Write-Host ("PASS {0}: nodes={1} edges={2} evidence={3} coverage={4}" -f
            $case.Id,
            $result.Manifest.nodeCount,
            $result.Manifest.edgeCount,
            $result.Manifest.evidenceCount,
            $result.Manifest.fileCoverageCount)
        $passed++
    }

    Write-Host "canonical 10-language gate: passed=$passed skipped=$skipped projectRuns=$($cases.Count) languages=$($contractLanguages.Count)"
    if (-not $AllowMissingProvider -and $passed -ne $cases.Count) {
        throw 'Not every canonical language project passed.'
    }
} finally {
    $env:CODE_MEMORY_CACHE_ROOT = $previousCacheRoot
    $env:CODE_MEMORY_STRICT = $previousStrict
    if ($KeepArtifacts) {
        Write-Host "Kept canonical language gate artifacts: $gateRoot"
    } else {
        Remove-SafeGateTree $gateRoot
    }
}
