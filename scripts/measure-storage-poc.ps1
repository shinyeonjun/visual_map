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
if (-not $Engine) { $Engine = Join-Path $repoRoot 'code_memory\rust\target\release\code-memory-language.exe' }
if (-not $OutputRoot) {
    $OutputRoot = Join-Path $LabRoot ('_results\storage-poc-' + (Get-Date -Format 'yyyyMMdd-HHmmss'))
}

function Get-TreeBytes([string]$Path) {
    [int64](@(Get-ChildItem -LiteralPath $Path -File -Recurse -ErrorAction SilentlyContinue) |
        Measure-Object -Property Length -Sum).Sum
}

function Test-Sqlite([string]$Path) {
    $expected = [Text.Encoding]::ASCII.GetBytes("SQLite format 3`0")
    $actual = [byte[]]::new($expected.Length)
    $stream = [IO.File]::OpenRead($Path)
    try { [void]$stream.Read($actual, 0, $actual.Length) } finally { $stream.Dispose() }
    return [Linq.Enumerable]::SequenceEqual([byte[]]$actual, [byte[]]$expected)
}

function Invoke-Index([string]$CaseId, [string]$Root, [string]$Store, [string]$RunRoot, [string]$Name) {
    $argsPath = Join-Path $RunRoot "$Name.args.json"
    $stdoutPath = Join-Path $RunRoot "$Name.stdout.log"
    $stderrPath = Join-Path $RunRoot "$Name.stderr.log"
    $payload = @{ repo_path = $Root; name = "storage-poc-$CaseId" } | ConvertTo-Json -Compress
    [IO.File]::WriteAllText($argsPath, $payload, [Text.UTF8Encoding]::new($false))
    $env:CBM_CACHE_DIR = $Store
    $env:CODE_MEMORY_CACHE_ROOT = Join-Path $Store 'runtime'
    $env:CODE_MEMORY_PACKS_ROOT = Join-Path $repoRoot 'code_memory'
    $env:CODE_MEMORY_PROVIDERS_ROOT = Join-Path $repoRoot 'code_memory\providers'
    $watch = [Diagnostics.Stopwatch]::StartNew()
    $process = Start-Process -FilePath $Engine -ArgumentList @('cli', 'index_repository', '--args-file', $argsPath) `
        -PassThru -WindowStyle Hidden -RedirectStandardOutput $stdoutPath -RedirectStandardError $stderrPath
    $peakBytes = 0L
    while (-not $process.HasExited) {
        $process.Refresh()
        if ($process.WorkingSet64 -gt $peakBytes) { $peakBytes = $process.WorkingSet64 }
        Start-Sleep -Milliseconds 200
    }
    $process.WaitForExit()
    $process.Refresh()
    $watch.Stop()
    $exitCode = $process.ExitCode
    if ($null -ne $exitCode -and [int]$exitCode -ne 0) {
        throw "$CaseId/$Name failed: $(@(Get-Content -LiteralPath $stderrPath -Tail 30) -join [Environment]::NewLine)"
    }
    $receiptLine = @(Get-Content -LiteralPath $stdoutPath -Encoding utf8 |
        Where-Object { $_ -like '{*code-memory.generation-receipt.v1*' } | Select-Object -Last 1)
    if (-not $receiptLine) { throw "$CaseId/$Name did not return a generation receipt" }
    $receipt = $receiptLine | ConvertFrom-Json
    if ($receipt.schema -ne 'code-memory.generation-receipt.v1' -or $receipt.status -ne 'complete') {
        throw "$CaseId/$Name returned an invalid generation receipt"
    }
    if (-not (Test-Sqlite ([string]$receipt.databasePath))) {
        throw "$CaseId/$Name did not publish SQLite"
    }
    [pscustomobject]@{
        elapsed_ms = $watch.ElapsedMilliseconds
        engine_peak_working_set_bytes = $peakBytes
        generation_id = [string]$receipt.generationId
        database_bytes = (Get-Item -LiteralPath ([string]$receipt.databasePath)).Length
        counts = $receipt.counts
    }
}

if (-not (Test-Path -LiteralPath $Engine -PathType Leaf)) { throw "Engine not found: $Engine" }
New-Item -ItemType Directory -Path $OutputRoot -Force | Out-Null
$records = [Collections.Generic.List[object]]::new()
foreach ($caseId in $Cases) {
    $root = Join-Path $LabRoot $caseId
    if (-not (Test-Path -LiteralPath $root -PathType Container)) { throw "Project not found: $root" }
    $runRoot = Join-Path $OutputRoot $caseId
    $store = Join-Path $runRoot 'store'
    New-Item -ItemType Directory -Path $store -Force | Out-Null
    $cold = Invoke-Index $caseId $root $store $runRoot 'cold'
    $warm = Invoke-Index $caseId $root $store $runRoot 'warm'
    if (($cold.counts | ConvertTo-Json -Compress) -ne ($warm.counts | ConvertTo-Json -Compress)) {
        throw "$caseId cold/warm counts differ"
    }
    $projectDir = Join-Path $store "compat-projects\storage-poc-$caseId"
    $generationCount = @(Get-ChildItem -LiteralPath (Join-Path $projectDir 'generations') -Directory).Count
    $stagingCount = @(Get-ChildItem -LiteralPath $projectDir -Filter '.staging-*' -Force -ErrorAction SilentlyContinue).Count
    $legacyJsonCount = @(Get-ChildItem -LiteralPath $projectDir -File -Filter '*.json' |
        Where-Object Name -NotIn @('current.json', 'previous.json')).Count
    if ($generationCount -ne 2 -or $stagingCount -ne 0 -or $legacyJsonCount -ne 0) {
        throw "$caseId generation cleanup failed"
    }
    $record = [pscustomobject]@{
        id = $caseId
        root = $root
        cold = $cold
        warm = $warm
        store_bytes = Get-TreeBytes $store
        runtime_cache_bytes = Get-TreeBytes (Join-Path $store 'runtime')
        generations = $generationCount
        staging_directories = $stagingCount
        legacy_result_json = $legacyJsonCount
    }
    $records.Add($record)
    Write-Host "PASS $caseId cold=$($cold.elapsed_ms)ms warm=$($warm.elapsed_ms)ms sqlite=$($warm.database_bytes)B"
}

$summaryPath = Join-Path $OutputRoot 'storage-summary.json'
$records | ConvertTo-Json -Depth 20 | Set-Content -LiteralPath $summaryPath -Encoding utf8
Write-Host "Wrote $summaryPath"
