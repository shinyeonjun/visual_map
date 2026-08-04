[CmdletBinding()]
param(
    [string]$Bridge = 'D:\project\visual_map\code_memory\rust\target\debug\code-memory-language.exe',
    [string]$LabRoot = 'D:\visual_map_reliability_lab',
    [string]$OutputRoot = 'D:\visual_map_reliability_lab\_results\engine-baseline-20260803'
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$cases = @(
    [pscustomobject]@{ Id = 'plane'; Root = Join-Path $LabRoot 'plane'; Label = 'Django + DRF + TypeScript' },
    [pscustomobject]@{ Id = 'spring-petclinic'; Root = Join-Path $LabRoot 'java-spring-petclinic-microservices'; Label = 'Spring Boot / Java' },
    [pscustomobject]@{ Id = 'nopcommerce'; Root = Join-Path $LabRoot 'nopCommerce'; Label = 'ASP.NET / C#' },
    [pscustomobject]@{ Id = 'fastapi-template'; Root = Join-Path $LabRoot 'fastapi-full-stack-fastapi-template'; Label = 'FastAPI + TypeScript' },
    [pscustomobject]@{ Id = 'nushell'; Root = Join-Path $LabRoot 'nushell'; Label = 'Rust' }
)

function Invoke-Code([string]$Tool, [hashtable]$Payload, [string]$RunRoot) {
    $argsPath = Join-Path $RunRoot ("args-" + [guid]::NewGuid().ToString('N') + '.json')
    [IO.File]::WriteAllText($argsPath, ($Payload | ConvertTo-Json -Compress -Depth 16), [Text.UTF8Encoding]::new($false))
    try {
        $output = & $Bridge cli $Tool --args-file $argsPath 2>&1
        if ($LASTEXITCODE -ne 0) { throw (($output | ForEach-Object { [string]$_ }) -join [Environment]::NewLine) }
        foreach ($line in @($output)) {
            $text = [string]$line
            if ($text.TrimStart().StartsWith('{') -or $text.TrimStart().StartsWith('[')) {
                try { return ($text | ConvertFrom-Json) } catch { }
            }
        }
        throw "$Tool returned no JSON"
    } finally {
        Remove-Item -LiteralPath $argsPath -Force -ErrorAction SilentlyContinue
    }
}

function Count-Values($values, [string]$property) {
    $counts = [ordered]@{}
    foreach ($value in @($values)) {
        $key = [string]$value.$property
        if ($key) {
            $current = 0
            if ($counts.Contains($key)) { $current = [int]$counts[$key] }
            $counts[$key] = 1 + $current
        }
    }
    return $counts
}

New-Item -ItemType Directory -Path $OutputRoot -Force | Out-Null
$records = [Collections.Generic.List[object]]::new()
foreach ($case in $cases) {
    if (-not (Test-Path -LiteralPath $case.Root -PathType Container)) { throw "Missing project: $($case.Root)" }
    $runRoot = Join-Path $OutputRoot "cache-$($case.Id)"
    New-Item -ItemType Directory -Path $runRoot -Force | Out-Null
    $env:CBM_CACHE_DIR = $runRoot
    $env:CODE_MEMORY_CACHE_ROOT = $runRoot
    $env:CODE_MEMORY_PROVIDERS_ROOT = 'D:\project\visual_map\code_memory\providers'
    $env:CODE_MEMORY_PACKS_ROOT = 'D:\project\visual_map\code_memory'
    $project = "baseline-$($case.Id)"
    $watch = [Diagnostics.Stopwatch]::StartNew()
    [void](Invoke-Code 'index_repository' @{ repo_path = $case.Root; mode = 'full'; name = $project; persistence = $false } $runRoot)
    $watch.Stop()
    $architecture = Invoke-Code 'get_architecture' @{ project = $project } $runRoot
    # Route labels in the compatibility graph include legacy aggregate rows.
    # Architecture ENDPOINT nodes are the product route surface and carry the
    # same handler-resolution evidence used by the desktop projection.
    $routeRows = @($architecture.nodes | Where-Object { $_.kind -eq 'ENDPOINT' })
    $handleRows = @($routeRows | Where-Object {
        $_.properties.handler_resolution -eq 'resolved'
    })
    $cacheProject = Join-Path $runRoot "compat-projects\$project"
    $languagePath = Join-Path $cacheProject 'language-index.json'
    $architecturePath = Join-Path $cacheProject 'architecture.json'
    if (Test-Path $languagePath) { Copy-Item $languagePath (Join-Path $OutputRoot "$($case.Id).language-index.json") -Force }
    if (Test-Path $architecturePath) { Copy-Item $architecturePath (Join-Path $OutputRoot "$($case.Id).architecture-index.json") -Force }
    $index = Get-Content $languagePath -Raw | ConvertFrom-Json
    $routeCount = $routeRows.Count
    $anyCount = @($routeRows | Where-Object {
        [string]$_.properties.method -eq 'ANY'
    }).Count
    $handledCount = $handleRows.Count
    $records.Add([pscustomobject]@{
        id = $case.Id
        label = $case.Label
        root = $case.Root
        elapsed_ms = $watch.ElapsedMilliseconds
        language_coverage = @($index.languages)
        framework_facts = @($architecture.frameworks | ForEach-Object { [pscustomobject]@{ id = $_.id; fact_count = [int]$_.fact_count } })
        route_count = $routeCount
        any_count = $anyCount
        any_ratio = if ($routeCount) { [math]::Round($anyCount / $routeCount, 4) } else { 0 }
        handles_count = $handledCount
        handles_rate = if ($routeCount) { [math]::Round($handledCount / $routeCount, 4) } else { 0 }
        architecture_node_kinds = Count-Values @($architecture.nodes) 'kind'
        architecture_edge_kinds = Count-Values @($architecture.edges) 'kind'
        diagnostics = @($index.diagnostics)
        timings = @($index.timings)
    })
    Write-Host "PASS $($case.Id): $($watch.ElapsedMilliseconds)ms, routes=$routeCount, ANY=$anyCount, HANDLES=$handledCount"
}

$jsonPath = Join-Path $OutputRoot 'engine-baseline.json'
$records | ConvertTo-Json -Depth 18 | Set-Content -LiteralPath $jsonPath -Encoding utf8
$markdown = [Collections.Generic.List[string]]::new()
$markdown.Add('# Engine baseline — 2026-08-03')
$markdown.Add('')
$markdown.Add('Generated by `scripts/measure-engine-baseline.ps1`. Raw `language-index` and `architecture-index` files are stored beside this report.')
$markdown.Add('')
$markdown.Add('| Repository | Languages | Product endpoints | ANY | Resolved handlers | Elapsed |')
$markdown.Add('|---|---|---:|---:|---:|---:|')
$markdown.Add('')
$markdown.Add('`Product endpoints` and `Resolved handlers` come from architecture `ENDPOINT` nodes. The compatibility graph also contains legacy aggregate route rows, so it is not used for this acceptance metric.')
foreach ($record in $records) {
    $languages = (@($record.language_coverage) | ForEach-Object { "$($_.id):$($_.files_indexed)/$($_.files_found) ($($_.status))" }) -join '<br>'
    $markdown.Add("| $($record.label) | $languages | $($record.route_count) | $($record.any_count) / $($record.route_count) ($($record.any_ratio)) | $($record.handles_count) / $($record.route_count) ($($record.handles_rate)) | $($record.elapsed_ms) ms |")
}
$markdown.Add('')
$markdown.Add('## Per-repository details')
foreach ($record in $records) {
    $markdown.Add("")
    $markdown.Add("### $($record.label)")
    $markdown.Add("")
    $markdown.Add('- Root: ' + '`' + $record.root + '`')
    $markdown.Add("- Frameworks: " + ((@($record.framework_facts) | ForEach-Object { "$($_.id) ($($_.fact_count) facts)" }) -join ', '))
    $markdown.Add("- Node kinds: " + (($record.architecture_node_kinds | ConvertTo-Json -Compress)))
    $markdown.Add("- Edge kinds: " + (($record.architecture_edge_kinds | ConvertTo-Json -Compress)))
    $markdown.Add("- Diagnostics: " + ((@($record.diagnostics) | ForEach-Object { "$($_.code): $($_.message)" }) -join ' | '))
    $markdown.Add("- Timings: " + ((@($record.timings) | ForEach-Object { "$($_.stage)=$($_.elapsed_ms)ms" }) -join ', '))
}
$markdown | Set-Content -LiteralPath (Join-Path $OutputRoot 'engine-baseline-2026-08-03.md') -Encoding utf8
Write-Host "Wrote $OutputRoot\engine-baseline-2026-08-03.md"
