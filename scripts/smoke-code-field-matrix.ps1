[CmdletBinding()]
param(
  [string]$EnginePath,
  [string]$ReuseRoot,
  [switch]$Keep
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $PSScriptRoot
if ([string]::IsNullOrWhiteSpace($EnginePath)) {
  $EnginePath = Join-Path $repoRoot "src-tauri\engines\codebase-memory-mcp.exe"
}
$EnginePath = [IO.Path]::GetFullPath($EnginePath)
if (-not (Test-Path -LiteralPath $EnginePath -PathType Leaf)) {
  throw "Code engine not found: $EnginePath"
}

$matrix = @(
  [pscustomobject]@{
    Name = "spring-petclinic"
    Languages = "Java/Spring"
    Url = "https://github.com/spring-projects/spring-petclinic.git"
    Commit = "51045d1648dad955df586150c1a1a6e22ef400c2"
  },
  [pscustomobject]@{
    Name = "clean-architecture"
    Languages = "C#/.NET"
    Url = "https://github.com/ardalis/CleanArchitecture.git"
    Commit = "a064d0b369b719ba03da71da1560d208d7e02e03"
  },
  [pscustomobject]@{
    Name = "full-stack-fastapi-template"
    Languages = "Python/FastAPI + TypeScript monorepo"
    Url = "https://github.com/fastapi/full-stack-fastapi-template.git"
    Commit = "4cd0d9e51aebd1af6f82d91ad0df4c9e41f4dea2"
  }
)

$tempBase = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
$ownsRoot = [string]::IsNullOrWhiteSpace($ReuseRoot)
$matrixRoot = if ($ownsRoot) {
  Join-Path $tempBase ("backend-visual-map-field-matrix-" + [guid]::NewGuid().ToString("N"))
} else {
  [IO.Path]::GetFullPath($ReuseRoot)
}
$sidecarRunner = Join-Path $PSScriptRoot "run-sidecar-json.mjs"
New-Item -ItemType Directory -Path $matrixRoot -Force | Out-Null

function Invoke-CodeTool([string]$Tool, [hashtable]$Payload, [string]$RunRoot) {
  $argsPath = Join-Path $RunRoot ("args-" + [guid]::NewGuid().ToString("N") + ".json")
  [IO.File]::WriteAllText(
    $argsPath,
    ($Payload | ConvertTo-Json -Compress -Depth 10),
    [Text.UTF8Encoding]::new($false)
  )
  $previousErrorAction = $ErrorActionPreference
  $ErrorActionPreference = "Continue"
  try {
    $output = & node $sidecarRunner $EnginePath $Tool $argsPath 2>&1
    $exitCode = $LASTEXITCODE
  } finally {
    $ErrorActionPreference = $previousErrorAction
    Remove-Item -LiteralPath $argsPath -Force -ErrorAction SilentlyContinue
  }
  if ($exitCode -ne 0) {
    throw "$Tool failed for $RunRoot. Sidecar output was suppressed; rerun this smoke locally for diagnostics."
  }
  foreach ($line in @($output)) {
    $text = [string]$line
    if ($text.TrimStart().StartsWith("{") -or $text.TrimStart().StartsWith("[")) {
      try { return $text | ConvertFrom-Json } catch { continue }
    }
  }
  throw "$Tool returned no JSON for $RunRoot."
}

$results = [System.Collections.Generic.List[object]]::new()
try {
  foreach ($entry in $matrix) {
    $sourceRoot = Join-Path $matrixRoot $entry.Name
    if (-not (Test-Path -LiteralPath (Join-Path $sourceRoot ".git") -PathType Container)) {
      if (-not $ownsRoot) {
        throw "Pinned repository is missing from -ReuseRoot: $sourceRoot"
      }
      & git -c core.longpaths=true clone --filter=blob:none --no-checkout $entry.Url $sourceRoot
      if ($LASTEXITCODE -ne 0) { throw "Clone failed: $($entry.Name)" }
      & git -C $sourceRoot -c core.longpaths=true checkout --detach $entry.Commit
      if ($LASTEXITCODE -ne 0) { throw "Pinned checkout failed: $($entry.Name) $($entry.Commit)" }
    }
    $actualCommit = (& git -C $sourceRoot rev-parse HEAD).Trim()
    if ($actualCommit -ne $entry.Commit) {
      throw "$($entry.Name) commit mismatch: expected $($entry.Commit), got $actualCommit"
    }

    $cacheRoot = Join-Path $matrixRoot ("cache-" + $entry.Name)
    New-Item -ItemType Directory -Path $cacheRoot -Force | Out-Null
    $env:CBM_CACHE_DIR = $cacheRoot
    $watch = [Diagnostics.Stopwatch]::StartNew()
    $index = Invoke-CodeTool "index_repository" @{
      repo_path = $sourceRoot
      path = $sourceRoot
      project = "bvm-field-$($entry.Name)"
      project_name = "bvm-field-$($entry.Name)"
      cache_path = $cacheRoot
      cache_dir = $cacheRoot
    } $cacheRoot
    $watch.Stop()
    $project = [string]$index.project
    if ([string]::IsNullOrWhiteSpace($project)) {
      throw "$($entry.Name) returned no project id."
    }
    $base = @{ project = $project; cache_path = $cacheRoot; cache_dir = $cacheRoot }
    $counts = @{}
    foreach ($label in "File", "Function", "Class", "Route") {
      $payload = $base.Clone()
      $payload.label = $label
      $payload.limit = 10000
      $payload.offset = 0
      $response = Invoke-CodeTool "search_graph" $payload $cacheRoot
      $items = @($response.results)
      if (@($items | Where-Object { $_.label -ne $label }).Count -gt 0) {
        throw "$($entry.Name) returned a non-$label item for the $label query."
      }
      $counts[$label] = $items.Count
    }
    if ($counts.File -eq 0 -or ($counts.Function + $counts.Class) -eq 0) {
      throw "$($entry.Name) produced no usable file/code inventory."
    }

    $callPayload = $base.Clone()
    $callPayload.query = "MATCH (caller)-[:CALLS]->(callee) RETURN caller.qualified_name AS source, callee.qualified_name AS target LIMIT 100000"
    $calls = Invoke-CodeTool "query_graph" $callPayload $cacheRoot
    if ((@($calls.columns) -join ",") -ne "source,target") {
      throw "$($entry.Name) CALLS columns drifted: $(@($calls.columns) -join ',')"
    }
    $validCalls = @($calls.rows | Where-Object { $_.Count -eq 2 -and $_[0] -and $_[1] }).Count

    $locationPayload = $base.Clone()
    $locationPayload.query = "MATCH (node) RETURN node.qualified_name AS source, node.file_path AS path, node.start_line AS start_line, node.start_column AS start_column, node.end_line AS end_line, node.end_column AS end_column LIMIT 100000"
    $locations = Invoke-CodeTool "query_graph" $locationPayload $cacheRoot
    if ((@($locations.columns) -join ",") -ne "source,path,start_line,start_column,end_line,end_column") {
      throw "$($entry.Name) source-location columns drifted."
    }
    $located = @($locations.rows | Where-Object { $_.Count -eq 6 -and $_[1] -and [int]$_[2] -gt 0 }).Count
    if ($located -eq 0) {
      throw "$($entry.Name) returned no positive source location."
    }

    $results.Add([pscustomobject][ordered]@{
      repository = $entry.Name
      languages = $entry.Languages
      commit = $actualCommit
      indexMs = $watch.ElapsedMilliseconds
      files = $counts.File
      functions = $counts.Function
      classes = $counts.Class
      routes = $counts.Route
      calls = $validCalls
      located = $located
    })
  }
  $results | Format-Table -AutoSize | Out-String | Write-Output
  $results | ConvertTo-Json -Depth 4 | Write-Output
  Write-Output "PASS: pinned multi-language code field matrix completed."
} finally {
  Remove-Item Env:CBM_CACHE_DIR -ErrorAction SilentlyContinue
  if ($ownsRoot -and -not $Keep) {
    $resolvedRoot = [IO.Path]::GetFullPath($matrixRoot)
    if ($resolvedRoot.StartsWith($tempBase, [StringComparison]::OrdinalIgnoreCase)) {
      Remove-Item -LiteralPath $resolvedRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
  }
}
