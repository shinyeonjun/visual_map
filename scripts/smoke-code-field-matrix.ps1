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
  Join-Path $tempBase ("bvm-" + [guid]::NewGuid().ToString("N").Substring(0, 8))
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
    $diagnostic = (@($output) | ForEach-Object { [string]$_ }) -join [Environment]::NewLine
    $diagnostic = $diagnostic.Replace($matrixRoot, "<matrix-root>", [StringComparison]::OrdinalIgnoreCase)
    $diagnostic = $diagnostic.Replace($EnginePath, "<engine>", [StringComparison]::OrdinalIgnoreCase)
    $diagnostic = $diagnostic.Replace($env:USERPROFILE, "<user-profile>", [StringComparison]::OrdinalIgnoreCase)
    if ($diagnostic.Length -gt 2000) { $diagnostic = $diagnostic.Substring($diagnostic.Length - 2000) }
    throw "$Tool failed for <run-root>.`n$diagnostic"
  }
  if (($output -join [Environment]::NewLine) -match "raw JSON.+deprecated") {
    throw "$Tool used the deprecated raw JSON transport."
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
    $env:CBM_ALLOWED_ROOT = $sourceRoot
    $watch = [Diagnostics.Stopwatch]::StartNew()
    $index = Invoke-CodeTool "index_repository" @{
      repo_path = $sourceRoot
      mode = "full"
      name = "bvm-field-$($entry.Name)"
      persistence = $false
    } $cacheRoot
    $watch.Stop()
    $project = [string]$index.project
    if ([string]::IsNullOrWhiteSpace($project)) {
      throw "$($entry.Name) returned no project id."
    }
    $base = @{ project = $project }
    $architecture = Invoke-CodeTool "get_architecture" $base $cacheRoot
    if ($null -eq $architecture) {
      throw "$($entry.Name) returned no architecture."
    }

    $nodePayload = $base.Clone()
    $nodePayload.query = "MATCH (node:Route|Function|Method|Class|Struct|Interface|Trait|Protocol|Record|Enum|Type|Constructor|Subroutine|Procedure|Decorator|Field|Variable|Module|Namespace|Package|Resource|File) RETURN labels(node) AS labels, node.name AS name, node.qualified_name AS qualified_name, node.file_path AS file_path, node.start_line AS start_line, node.start_column AS start_column, node.end_line AS end_line, node.end_column AS end_column, node.method AS method, node.source AS source, node.parent_qualified_name AS parent_qualified_name, node.parent_class AS parent_class, node.module AS module, node.namespace AS namespace, node.package AS package, node.route_path AS route_path, node.route_method AS route_method, node.signature AS signature, node.return_type AS return_type, node.is_test AS is_test LIMIT 100000"
    $nodes = Invoke-CodeTool "query_graph" $nodePayload $cacheRoot
    $nodeColumns = "labels,name,qualified_name,file_path,start_line,start_column,end_line,end_column,method,source,parent_qualified_name,parent_class,module,namespace,package,route_path,route_method,signature,return_type,is_test"
    if ((@($nodes.columns) -join ",") -ne $nodeColumns) {
      throw "$($entry.Name) node columns drifted: $(@($nodes.columns) -join ',')"
    }
    if ([int]$nodes.total -ge 100000) {
      throw "$($entry.Name) node inventory reached the adapter safety limit."
    }
    $nodeRows = @($nodes.rows)
    $counts = @{}
    foreach ($label in "File", "Function", "Method", "Class", "Route") {
      $counts[$label] = @($nodeRows | Where-Object {
        [string]$_[0] -match ('"' + $label + '"')
      }).Count
    }
    $codeNodes = @($nodeRows | Where-Object {
      [string]$_[0] -notmatch '"(File|Route)"'
    }).Count
    if ($counts.File -eq 0 -or $codeNodes -eq 0) {
      throw "$($entry.Name) produced no usable file/code inventory."
    }
    $located = @($nodeRows | Where-Object {
      $_.Count -eq 20 -and $_[3] -and [int]$_[4] -gt 0
    }).Count
    if ($located -eq 0) {
      throw "$($entry.Name) returned no positive source location."
    }

    $callPayload = $base.Clone()
    $callPayload.query = "MATCH (caller)-[rel:CALLS]->(callee) RETURN caller.qualified_name AS source, callee.qualified_name AS target, rel.confidence AS confidence, rel.strategy AS strategy, rel.callee AS call_expression LIMIT 100000"
    $calls = Invoke-CodeTool "query_graph" $callPayload $cacheRoot
    if ((@($calls.columns) -join ",") -ne "source,target,confidence,strategy,call_expression") {
      throw "$($entry.Name) CALLS columns drifted: $(@($calls.columns) -join ',')"
    }
    if ([int]$calls.total -ge 100000) {
      throw "$($entry.Name) CALLS reached the adapter safety limit."
    }
    $validCallRows = @($calls.rows | Where-Object { $_.Count -eq 5 -and $_[0] -and $_[1] })
    $confirmedCalls = 0
    $candidateCalls = 0
    $unknownCalls = 0
    foreach ($row in $validCallRows) {
      $score = 0.0
      if (-not [double]::TryParse([string]$row[2], [ref]$score)) {
        $unknownCalls += 1
      } elseif ($score -ge 0.85) {
        $confirmedCalls += 1
      } elseif ($score -ge 0.70) {
        $candidateCalls += 1
      } else {
        $unknownCalls += 1
      }
    }
    if ($validCallRows.Count -gt 0 -and $confirmedCalls -eq 0) {
      throw "$($entry.Name) returned no high-confidence CALLS relationship."
    }

    $handlePayload = $base.Clone()
    $handlePayload.query = "MATCH (handler)-[:HANDLES]->(route) RETURN handler.qualified_name AS source, route.qualified_name AS target LIMIT 100000"
    $handles = Invoke-CodeTool "query_graph" $handlePayload $cacheRoot
    if ((@($handles.columns) -join ",") -ne "source,target") {
      throw "$($entry.Name) HANDLES columns drifted: $(@($handles.columns) -join ',')"
    }
    if ([int]$handles.total -ge 100000) {
      throw "$($entry.Name) HANDLES reached the adapter safety limit."
    }
    $validHandleRows = @($handles.rows | Where-Object {
      $_.Count -eq 2 -and $_[0] -and $_[1]
    })
    $handleTargets = [System.Collections.Generic.HashSet[string]]::new(
      [StringComparer]::Ordinal
    )
    foreach ($row in $validHandleRows) {
      [void]$handleTargets.Add([string]$row[1])
    }
    $usableEngineRoutes = @($nodeRows | Where-Object {
      [string]$_[0] -match '"Route"' -and
      [string]$_[1] -notmatch '://' -and
      (
        ($_[3] -and [int]$_[4] -gt 0) -or
        $handleTargets.Contains([string]$_[2])
      )
    }).Count
    if ($entry.Name -ne "clean-architecture" -and
        ($usableEngineRoutes -eq 0 -or $validHandleRows.Count -eq 0)) {
      throw "$($entry.Name) produced no source-backed API route/HANDLES path."
    }

    $results.Add([pscustomobject][ordered]@{
      repository = $entry.Name
      languages = $entry.Languages
      commit = $actualCommit
      indexMs = $watch.ElapsedMilliseconds
      files = $counts.File
      functions = $counts.Function
      methods = $counts.Method
      classes = $counts.Class
      rawRoutes = $counts.Route
      usableEngineRoutes = $usableEngineRoutes
      productAdapterRoutes = 0
      engineHandles = $validHandleRows.Count
      productAdapterHandles = 0
      calls = $validCallRows.Count
      confirmedCalls = $confirmedCalls
      candidateCalls = $candidateCalls
      unknownCalls = $unknownCalls
      located = $located
    })
  }

  $csharpRepo = Join-Path $matrixRoot "clean-architecture"
  $manifestPath = Join-Path $repoRoot "src-tauri\Cargo.toml"
  $env:BACKEND_MAP_TEST_CODE_REPO = $csharpRepo
  $env:BACKEND_MAP_TEST_CODE_ENGINE = $EnginePath
  $previousErrorAction = $ErrorActionPreference
  $ErrorActionPreference = "Continue"
  try {
    $productTestOutput = @(
      & cargo test --locked --manifest-path $manifestPath code_field_fastendpoints_adapter_proves_real_routes_and_handlers -- --ignored --nocapture 2>&1
    )
    $productTestExitCode = $LASTEXITCODE
  } finally {
    $ErrorActionPreference = $previousErrorAction
  }
  $productTestOutput | ForEach-Object { Write-Output ([string]$_) }
  if ($productTestExitCode -ne 0) {
    throw "The product FastEndpoints adapter field test failed."
  }
  $productRouteCount = $null
  $productHandleCount = $null
  foreach ($line in $productTestOutput) {
    if ([string]$line -match 'product FastEndpoints routes=(\d+) handles=(\d+)') {
      $productRouteCount = [int]$Matches[1]
      $productHandleCount = [int]$Matches[2]
      break
    }
  }
  if ($null -eq $productRouteCount -or $null -eq $productHandleCount) {
    throw "The product FastEndpoints adapter returned no count receipt."
  }
  $csharpResult = $results |
    Where-Object { $_.repository -eq "clean-architecture" } |
    Select-Object -First 1
  $csharpResult.productAdapterRoutes = $productRouteCount
  $csharpResult.productAdapterHandles = $productHandleCount

  $env:BACKEND_MAP_TEST_CODE_REPO = Join-Path $matrixRoot "full-stack-fastapi-template"
  & cargo test --locked --manifest-path $manifestPath code_field_fastapi_adapter_proves_real_import_calls -- --ignored --nocapture
  if ($LASTEXITCODE -ne 0) {
    throw "The product FastAPI static import field test failed."
  }

  $results | Format-Table -AutoSize | Out-String | Write-Output
  $results | ConvertTo-Json -Depth 4 | Write-Output
  Write-Output "PASS: pinned multi-language code field matrix completed."
} finally {
  Remove-Item Env:CBM_CACHE_DIR -ErrorAction SilentlyContinue
  Remove-Item Env:CBM_ALLOWED_ROOT -ErrorAction SilentlyContinue
  Remove-Item Env:BACKEND_MAP_TEST_CODE_REPO -ErrorAction SilentlyContinue
  Remove-Item Env:BACKEND_MAP_TEST_CODE_ENGINE -ErrorAction SilentlyContinue
  if ($ownsRoot -and -not $Keep) {
    $resolvedRoot = [IO.Path]::GetFullPath($matrixRoot)
    if ($resolvedRoot.StartsWith($tempBase, [StringComparison]::OrdinalIgnoreCase)) {
      Remove-Item -LiteralPath $resolvedRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
  }
}
