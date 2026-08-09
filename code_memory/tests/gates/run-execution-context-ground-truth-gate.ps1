[CmdletBinding()]
param(
    [string]$Bridge = (Join-Path $PSScriptRoot '..\..\rust\target\release\code-memory-language.exe'),
    [string]$ProvidersRoot,
    [int]$Runs = 2,
    [string]$OutputRoot = (Join-Path $PSScriptRoot '..\..\build\execution-context-ground-truth'),
    [ValidateSet('All', 'Positive', 'Partial')]
    [string]$Phase = 'All'
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
if (Get-Variable PSNativeCommandUseErrorActionPreference -ErrorAction SilentlyContinue) {
    $PSNativeCommandUseErrorActionPreference = $false
}

. (Join-Path $PSScriptRoot 'lib\language-ir-stream-authority.ps1')

$fixturesRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\fixtures')).Path
$packsRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$truthPath = Join-Path $PSScriptRoot '..\ground_truth\execution-context.v1.json'
$truth = Get-Content -LiteralPath $truthPath -Raw | ConvertFrom-Json
$Bridge = [IO.Path]::GetFullPath($Bridge)
$OutputRoot = [IO.Path]::GetFullPath($OutputRoot)
$runRoot = Join-Path $OutputRoot ('run-' + [guid]::NewGuid().ToString('N'))
$resultRoot = Join-Path $runRoot 'results'
$variantRoot = Join-Path $runRoot 'fixtures'
$tempBase = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
# Provider cache paths grow again below CODE_MEMORY_CACHE_ROOT. Keeping the
# isolated cache below the already deeply nested report directory can exceed
# the legacy Windows process-path limit before JDTLS starts. The cache remains
# unique and disposable, but its root is intentionally short.
$cacheRoot = Join-Path $tempBase ('cm-ec-' + [guid]::NewGuid().ToString('N'))

if ($Runs -lt 2) {
    throw 'Execution-context ground truth requires at least two runs.'
}
if (-not (Test-Path -LiteralPath $Bridge -PathType Leaf)) {
    throw "Bridge not found: $Bridge"
}
if ([string]::IsNullOrWhiteSpace($ProvidersRoot)) {
    $ProvidersRoot = Join-Path $packsRoot 'providers'
}
$ProvidersRoot = (Resolve-Path $ProvidersRoot).Path
New-Item -ItemType Directory -Force -Path $resultRoot,$variantRoot,$cacheRoot | Out-Null

function Get-Sha256([string]$Path) {
    $stream = [IO.File]::Open($Path, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
    $algorithm = [Security.Cryptography.SHA256]::Create()
    try {
        return ([BitConverter]::ToString($algorithm.ComputeHash($stream))).Replace('-', '').ToLowerInvariant()
    } finally {
        $algorithm.Dispose()
        $stream.Dispose()
    }
}

function Get-DimensionKeys($Dimensions) {
    return @($Dimensions | ForEach-Object { "$($_.kind)=$($_.value)" } | Sort-Object)
}

function Get-ArtifactKeys($Artifacts) {
    return @($Artifacts | ForEach-Object {
            "$($_.path)|$($_.usage)|$($_.contentDigest)"
        } | Sort-Object)
}

function Assert-ExactSet([string]$Context, [object[]]$Expected, [object[]]$Actual) {
    $expectedValues = @($Expected | ForEach-Object { [string]$_ } | Sort-Object)
    $actualValues = @($Actual | ForEach-Object { [string]$_ } | Sort-Object)
    $difference = @(Compare-Object -ReferenceObject $expectedValues -DifferenceObject $actualValues)
    if ($difference.Count -gt 0) {
        throw "$Context mismatch. expected=[$($expectedValues -join ', ')] actual=[$($actualValues -join ', ')]"
    }
}

function Assert-ArtifactFiles([string]$Context, [string]$Fixture, [object[]]$ExpectedArtifacts) {
    foreach ($entry in $ExpectedArtifacts) {
        $parts = ([string]$entry).Split('|')
        if ($parts.Count -ne 3) {
            throw "$Context has an invalid artifact truth key: $entry"
        }
        $path = [IO.Path]::GetFullPath((Join-Path $Fixture $parts[0]))
        $fixturePrefix = [IO.Path]::GetFullPath($Fixture).TrimEnd('\') + '\'
        if (-not $path.StartsWith($fixturePrefix, [StringComparison]::OrdinalIgnoreCase)) {
            throw "$Context artifact escapes its fixture: $($parts[0])"
        }
        if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
            throw "$Context expected artifact is missing: $($parts[0])"
        }
        $actualDigest = Get-Sha256 $path
        if ($actualDigest -ne $parts[2]) {
            throw "$Context fixture digest changed for $($parts[0]): expected=$($parts[2]) actual=$actualDigest"
        }
    }
}

function Invoke-ContextAnalysis([string]$Id, [string]$Fixture, [int]$Run) {
    $caseRoot = Join-Path $resultRoot $Id
    New-Item -ItemType Directory -Force -Path $caseRoot | Out-Null
    $out = Join-Path $caseRoot ("run-$Run.json")
    $arguments = @(
        'index', '--root', $Fixture, '--out', $out,
        '--packs-root', $packsRoot,
        '--providers-root', $ProvidersRoot
    )
    $lines = @(& $Bridge @arguments 2>&1 | ForEach-Object { $_.ToString() })
    $exitCode = $LASTEXITCODE
    if ($exitCode -ne 0) {
        throw "$Id run $Run failed with exit code $exitCode`n$($lines -join [Environment]::NewLine)"
    }

    $contextPrefix = '@codebase-workspace-provider-execution-context '
    $contextLine = @($lines | Where-Object {
            $_.StartsWith($contextPrefix, [StringComparison]::Ordinal)
        }) | Select-Object -Last 1
    if ([string]::IsNullOrWhiteSpace($contextLine)) {
        throw "$Id run $Run emitted no provider execution-context receipt."
    }
    $context = $contextLine.Substring($contextPrefix.Length) | ConvertFrom-Json
    if ($context.schema -ne $truth.receiptSchema) {
        throw "$Id run $Run used execution-context schema $($context.schema), expected $($truth.receiptSchema)."
    }
    if ($context.detailsTruncated) {
        throw "$Id run $Run truncated its execution-context audit."
    }
    if (@($context.executionSample).Count -ne [int]$context.executionCount) {
        throw "$Id run $Run did not expose every small-fixture execution context."
    }

    $irPrefix = '@codebase-workspace-language-ir '
    $irLine = @($lines | Where-Object {
            $_.StartsWith($irPrefix, [StringComparison]::Ordinal)
        }) | Select-Object -Last 1
    if ([string]::IsNullOrWhiteSpace($irLine)) {
        throw "$Id run $Run emitted no Language IR receipt."
    }
    $ir = $irLine.Substring($irPrefix.Length) | ConvertFrom-Json
    if ($ir.schema -ne $truth.languageIrReceiptSchema) {
        throw "$Id run $Run used Language IR schema $($ir.schema), expected $($truth.languageIrReceiptSchema)."
    }
    if ($ir.executionContextSetDigest -ne $context.contextSetDigest) {
        throw "$Id run $Run split execution identity between reconciliation and Language IR."
    }
    $authority = Get-LanguageIrStreamAuthority -BridgeOutput $lines -Receipt $ir -Context "$Id run $Run"
    if (-not (Test-Path -LiteralPath $out -PathType Leaf)) {
        throw "$Id run $Run did not write its index result."
    }
    $result = Get-Content -LiteralPath $out -Raw | ConvertFrom-Json
    return [pscustomobject]@{
        context = $context
        ir = $ir
        authority = $authority
        result = $result
    }
}

function Assert-Executions([string]$Context, $ExpectedProject, $Analysis, [bool]$CheckArtifacts) {
    $ActualReceipt = $Analysis.context
    $expectedExecutions = @($ExpectedProject.executions)
    $actualExecutions = @($ActualReceipt.executionSample)
    if ($actualExecutions.Count -ne $expectedExecutions.Count) {
        throw "$Context execution count mismatch: expected=$($expectedExecutions.Count) actual=$($actualExecutions.Count)"
    }
    foreach ($expected in $expectedExecutions) {
        $matches = @($actualExecutions | Where-Object language -eq $expected.language)
        if ($matches.Count -ne 1) {
            throw "$Context expected exactly one $($expected.language) execution, got $($matches.Count)."
        }
        $actual = $matches[0]
        if ($actual.mode -ne $expected.mode) {
            $languageResult = @($Analysis.result.languages | Where-Object id -eq $expected.language) | Select-Object -First 1
            $diagnostics = @($Analysis.result.diagnostics | Where-Object language -eq $expected.language)
            $failureDetail = [ordered]@{
                status = if ($null -eq $languageResult) { 'missing' } else { [string]$languageResult.status }
                diagnostics = $diagnostics
            } | ConvertTo-Json -Depth 8 -Compress
            throw "$Context $($expected.language) mode is $($actual.mode), expected $($expected.mode). provider=$failureDetail"
        }
        Assert-ExactSet "$Context $($expected.language) dimensions" @($expected.dimensions) @(Get-DimensionKeys $actual.dimensions)
        Assert-ExactSet "$Context $($expected.language) missing dimensions" @($expected.missingDimensions) @($actual.missingDimensions)
        if ($CheckArtifacts) {
            Assert-ExactSet "$Context $($expected.language) config artifacts" @($expected.configArtifacts) @(Get-ArtifactKeys $actual.configArtifacts)
        }
    }
}

function Assert-Deterministic([string]$Context, [object[]]$Runs) {
    foreach ($field in @('contextSetDigest')) {
        if (@($Runs | ForEach-Object { $_.context.$field } | Sort-Object -Unique).Count -ne 1) {
            throw "$Context changed $field across identical runs."
        }
    }
    foreach ($field in @('snapshotId', 'executionContextSetDigest', 'streamSetDigest')) {
        if (@($Runs | ForEach-Object { $_.ir.$field } | Sort-Object -Unique).Count -ne 1) {
            throw "$Context changed $field across identical runs."
        }
    }
    if (@($Runs | ForEach-Object { $_.authority.contentDigest } | Sort-Object -Unique).Count -ne 1) {
        throw "$Context changed the authoritative Language IR bytes across identical runs."
    }
}

function New-PartialFixture($Project) {
    $source = Join-Path $fixturesRoot ([string]$Project.fixture)
    $destination = Join-Path $variantRoot ([string]$Project.id)
    Copy-Item -LiteralPath $source -Destination $destination -Recurse
    $destination = [IO.Path]::GetFullPath($destination)
    $prefix = $destination.TrimEnd('\') + '\'
    foreach ($relative in @($Project.removeFiles)) {
        $target = [IO.Path]::GetFullPath((Join-Path $destination ([string]$relative)))
        if (-not $target.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase)) {
            throw "$($Project.id) removal escapes its isolated fixture: $relative"
        }
        if (-not (Test-Path -LiteralPath $target -PathType Leaf)) {
            throw "$($Project.id) cannot remove missing fixture input: $relative"
        }
        Remove-Item -LiteralPath $target -Force
    }
    return $destination
}

$previousCacheRoot = $env:CODE_MEMORY_CACHE_ROOT
$previousGoOs = $env:GOOS
$previousGoArch = $env:GOARCH
$previousGoFlags = $env:GOFLAGS
$previousCargoTarget = $env:CARGO_BUILD_TARGET
$reportCases = @()
try {
    $env:CODE_MEMORY_CACHE_ROOT = $cacheRoot
    $env:GOOS = [string]$truth.controlledEnvironment.GOOS
    $env:GOARCH = [string]$truth.controlledEnvironment.GOARCH
    $env:GOFLAGS = [string]$truth.controlledEnvironment.GOFLAGS
    $env:CARGO_BUILD_TARGET = $null

    if ($Phase -in @('All', 'Positive')) {
      foreach ($project in @($truth.positiveProjects)) {
        $fixture = Join-Path $fixturesRoot ([string]$project.fixture)
        foreach ($execution in @($project.executions)) {
            Assert-ArtifactFiles "$($project.id) $($execution.language)" $fixture @($execution.configArtifacts)
        }
        $analyses = @()
        for ($run = 1; $run -le $Runs; $run++) {
            $analysis = Invoke-ContextAnalysis ([string]$project.id) $fixture $run
            Assert-Executions "$($project.id) run $run" $project $analysis $true
            if ([int]$analysis.context.exactExecutionCount -ne @($project.executions).Count -or
                [int]$analysis.context.partialExecutionCount -ne 0 -or
                [int]$analysis.context.notExecutedCount -ne 0) {
                throw "$($project.id) run $run did not remain fully exact."
            }
            $analyses += $analysis
        }
        Assert-Deterministic ([string]$project.id) $analyses
        $reportCases += [pscustomobject]@{
            id = [string]$project.id
            class = 'positive'
            languages = @($project.executions | ForEach-Object language)
            contextSetDigest = [string]$analyses[0].context.contextSetDigest
            deterministic = $true
        }
        Write-Host "PASS context positive $($project.id)"
      }
    }

    if ($Phase -in @('All', 'Partial')) {
      foreach ($project in @($truth.partialProjects)) {
        $fixture = New-PartialFixture $project
        $analyses = @()
        for ($run = 1; $run -le $Runs; $run++) {
            $analysis = Invoke-ContextAnalysis ([string]$project.id) $fixture $run
            Assert-Executions "$($project.id) run $run" $project $analysis $false
            if ([int]$analysis.context.exactExecutionCount -ne 0) {
                throw "$($project.id) run $run promoted a missing context to exact."
            }
            $analyses += $analysis
        }
        Assert-Deterministic ([string]$project.id) $analyses
        $reportCases += [pscustomobject]@{
            id = [string]$project.id
            class = 'partial'
            languages = @($project.executions | ForEach-Object language)
            contextSetDigest = [string]$analyses[0].context.contextSetDigest
            deterministic = $true
        }
        Write-Host "PASS context partial $($project.id)"
      }
    }
} finally {
    $env:CODE_MEMORY_CACHE_ROOT = $previousCacheRoot
    $env:GOOS = $previousGoOs
    $env:GOARCH = $previousGoArch
    $env:GOFLAGS = $previousGoFlags
    $env:CARGO_BUILD_TARGET = $previousCargoTarget
    $resolvedCacheRoot = [IO.Path]::GetFullPath($cacheRoot)
    $tempPrefix = $tempBase.TrimEnd('\') + '\'
    if ($resolvedCacheRoot.StartsWith($tempPrefix, [StringComparison]::OrdinalIgnoreCase) -and
        [IO.Path]::GetFileName($resolvedCacheRoot).StartsWith('cm-ec-', [StringComparison]::Ordinal)) {
        Remove-Item -LiteralPath $resolvedCacheRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}

$languageCount = @($truth.positiveProjects.executions | ForEach-Object language | Sort-Object -Unique).Count
$executedPositiveCount = @($reportCases | Where-Object class -eq 'positive').Count
$executedPartialCount = @($reportCases | Where-Object class -eq 'partial').Count
$report = [ordered]@{
    schema = 'codebase-workspace.execution-context-ground-truth-report.v1'
    truthSchema = [string]$truth.schema
    runs = $Runs
    phase = $Phase
    languageCount = $languageCount
    positiveProjectCount = $executedPositiveCount
    partialProjectCount = $executedPartialCount
    cases = $reportCases
}
$reportPath = Join-Path $runRoot 'execution-context-report.json'
[IO.File]::WriteAllText($reportPath, ($report | ConvertTo-Json -Depth 8), [Text.UTF8Encoding]::new($false))
Write-Host "execution context ground-truth gate: languages=$languageCount positive=$executedPositiveCount partial=$executedPartialCount runs=$Runs"
Write-Host "report=$reportPath"
