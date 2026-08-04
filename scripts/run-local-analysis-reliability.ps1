[CmdletBinding()]
param(
    [string]$Engine = '',
    [string]$LabRoot = 'D:\visual_map_reliability_lab',
    [string]$OutputRoot = '',
    [string[]]$Cases = @('simplebank', 'java-spring-petclinic-microservices', 'plane')
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$repoRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
if (-not $Engine) { $Engine = Join-Path $repoRoot 'code_memory\rust\target\debug\code-memory-language.exe' }
if (-not $OutputRoot) {
    $OutputRoot = Join-Path $LabRoot ('_results\local-analysis-dag-' + (Get-Date -Format 'yyyyMMdd-HHmmss'))
}
$packsRoot = Join-Path $repoRoot 'code_memory'
$providersRoot = Join-Path $repoRoot 'code_memory\providers'

function Get-Sha256([byte[]]$Bytes) {
    $sha = [Security.Cryptography.SHA256]::Create()
    try { return ([BitConverter]::ToString($sha.ComputeHash($Bytes))).Replace('-', '').ToLowerInvariant() }
    finally { $sha.Dispose() }
}

function Get-FileSha256([string]$Path) {
    $sha = [Security.Cryptography.SHA256]::Create()
    $stream = [IO.File]::OpenRead($Path)
    try { return ([BitConverter]::ToString($sha.ComputeHash($stream))).Replace('-', '').ToLowerInvariant() }
    finally {
        $stream.Dispose()
        $sha.Dispose()
    }
}

function Get-SemanticIndexHash([string]$Path) {
    $value = Get-Content -LiteralPath $Path -Raw -Encoding UTF8 | ConvertFrom-Json
    [void]$value.PSObject.Properties.Remove('timings')
    foreach ($unit in @($value.analysis_units)) {
        [void]$unit.PSObject.Properties.Remove('execution')
        [void]$unit.PSObject.Properties.Remove('elapsed_ms')
    }
    $json = $value | ConvertTo-Json -Compress -Depth 100
    return Get-Sha256 ([Text.Encoding]::UTF8.GetBytes($json))
}

function Assert-OutputContract($Index, $Architecture) {
    if ($Index.schema -ne 'code-memory.language-index.v2') { throw "Unexpected index schema: $($Index.schema)" }
    if ($Architecture.schema -ne 'code-memory.architecture-index.v3') { throw "Unexpected architecture schema: $($Architecture.schema)" }
    $ids = [Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
    foreach ($node in @($Architecture.nodes)) { [void]$ids.Add([string]$node.id) }
    foreach ($edge in @($Architecture.edges)) {
        if (-not $ids.Contains([string]$edge.from) -or -not $ids.Contains([string]$edge.to)) {
            throw "Dangling architecture edge: $($edge.id)"
        }
    }
    if (@($Index.analysis_units | Where-Object { $_.execution -eq 'not-run' }).Count) {
        throw 'At least one analysis unit has no execution result.'
    }
}

function Invoke-EngineRun([string]$CaseId, [string]$Root, [string]$RunName, [string]$OutputName, [string]$RunRoot) {
    $indexPath = Join-Path $RunRoot "$OutputName.language-index.json"
    $architecturePath = Join-Path $RunRoot "$OutputName.architecture-index.json"
    $stdoutPath = Join-Path $RunRoot "$RunName.stdout.log"
    $stderrPath = Join-Path $RunRoot "$RunName.stderr.log"
    $arguments = @(
        'index', '--root', $Root, '--out', $indexPath,
        '--architecture-out', $architecturePath,
        '--packs-root', $packsRoot, '--providers-root', $providersRoot
    )
    $watch = [Diagnostics.Stopwatch]::StartNew()
    $process = Start-Process -FilePath $Engine -ArgumentList $arguments -PassThru -WindowStyle Hidden `
        -RedirectStandardOutput $stdoutPath -RedirectStandardError $stderrPath
    $peakBytes = 0L
    while (-not $process.HasExited) {
        $process.Refresh()
        if ($process.WorkingSet64 -gt $peakBytes) { $peakBytes = $process.WorkingSet64 }
        Start-Sleep -Milliseconds 200
    }
    $process.WaitForExit()
    $watch.Stop()
    $exitCode = $process.ExitCode
    if ($null -ne $exitCode -and [int]$exitCode -ne 0) {
        $tail = @(Get-Content -LiteralPath $stderrPath -Tail 30 -ErrorAction SilentlyContinue) -join [Environment]::NewLine
        throw "$CaseId/$RunName failed with exit code ${exitCode}:`n$tail"
    }
    if (-not (Test-Path -LiteralPath $indexPath -PathType Leaf) -or
        -not (Test-Path -LiteralPath $architecturePath -PathType Leaf)) {
        throw "$CaseId/$RunName did not produce both output files"
    }
    $index = Get-Content -LiteralPath $indexPath -Raw -Encoding UTF8 | ConvertFrom-Json
    $architecture = Get-Content -LiteralPath $architecturePath -Raw -Encoding UTF8 | ConvertFrom-Json
    Assert-OutputContract $index $architecture
    $stderr = @(Get-Content -LiteralPath $stderrPath -ErrorAction SilentlyContinue)
    return [pscustomobject]@{
        name = $RunName
        elapsed_ms = $watch.ElapsedMilliseconds
        engine_peak_working_set_bytes = $peakBytes
        progress_events = @($stderr | Where-Object { $_ -like '@visual-map-progress *' }).Count
        index_hash = Get-SemanticIndexHash $indexPath
        architecture_hash = Get-FileSha256 $architecturePath
        files_found = [int](@($index.languages) | Measure-Object -Property files_found -Sum).Sum
        files_indexed = [int](@($index.languages) | Measure-Object -Property files_indexed -Sum).Sum
        files_missing = [int](@($index.languages) | Measure-Object -Property files_missing -Sum).Sum
        analysis_units = @($index.analysis_units).Count
        cache_units = @($index.analysis_units | Where-Object { $_.execution -eq 'cache' }).Count
        provider_units = @($index.analysis_units | Where-Object { $_.execution -eq 'provider' }).Count
        error_diagnostics = @($index.diagnostics | Where-Object { $_.level -eq 'error' }).Count
        endpoints = @($architecture.nodes | Where-Object { $_.kind -eq 'ENDPOINT' }).Count
        resolved_handlers = @($architecture.nodes | Where-Object {
            $_.kind -eq 'ENDPOINT' -and $_.properties.handler_resolution -eq 'resolved'
        }).Count
    }
}

if (-not (Test-Path -LiteralPath $Engine -PathType Leaf)) { throw "Engine not found: $Engine" }
New-Item -ItemType Directory -Path $OutputRoot -Force | Out-Null
$records = [Collections.Generic.List[object]]::new()
foreach ($caseId in $Cases) {
    $root = Join-Path $LabRoot $caseId
    if (-not (Test-Path -LiteralPath $root -PathType Container)) { throw "Project not found: $root" }
    $runRoot = Join-Path $OutputRoot $caseId
    if (Test-Path -LiteralPath $runRoot) { throw "Case output already exists; choose a fresh OutputRoot: $runRoot" }
    New-Item -ItemType Directory -Path $runRoot | Out-Null
    $cacheRoot = Join-Path $runRoot 'cache'
    New-Item -ItemType Directory -Path $cacheRoot -Force | Out-Null
    $env:CODE_MEMORY_CACHE_ROOT = $cacheRoot
    $cold = Invoke-EngineRun $caseId $root 'cold' 'current' $runRoot
    Copy-Item -LiteralPath (Join-Path $runRoot 'current.language-index.json') `
        -Destination (Join-Path $runRoot 'cold.language-index.json') -Force
    Copy-Item -LiteralPath (Join-Path $runRoot 'current.architecture-index.json') `
        -Destination (Join-Path $runRoot 'cold.architecture-index.json') -Force
    $warm = Invoke-EngineRun $caseId $root 'warm' 'current' $runRoot
    $temporaryFiles = @(Get-ChildItem -LiteralPath $runRoot -Filter '*.tmp' -Recurse -File -ErrorAction SilentlyContinue).Count
    $record = [pscustomobject]@{
        id = $caseId
        root = $root
        deterministic = $cold.index_hash -eq $warm.index_hash -and $cold.architecture_hash -eq $warm.architecture_hash
        temporary_files = $temporaryFiles
        cold = $cold
        warm = $warm
    }
    if (-not $record.deterministic) { throw "$caseId cold/warm semantic output differs" }
    if ($temporaryFiles) { throw "$caseId left $temporaryFiles temporary files" }
    $records.Add($record)
    Write-Host "PASS $caseId cold=$($cold.elapsed_ms)ms warm=$($warm.elapsed_ms)ms units=$($cold.analysis_units)"
}

$summaryPath = Join-Path $OutputRoot 'reliability-summary.json'
$records | ConvertTo-Json -Depth 20 | Set-Content -LiteralPath $summaryPath -Encoding utf8
Write-Host "Wrote $summaryPath"
