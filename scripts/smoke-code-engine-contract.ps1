[CmdletBinding()]
param(
    [string]$EnginePath,
    [switch]$KeepFixture
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

$tempBase = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
$fixtureRoot = Join-Path $tempBase ("backend-visual-map-code-contract-" + [guid]::NewGuid().ToString("N"))
$sourceRoot = Join-Path $fixtureRoot "repo"
$cacheRoot = Join-Path $fixtureRoot "cache"
$sidecarRunner = Join-Path $PSScriptRoot "run-sidecar-json.mjs"
New-Item -ItemType Directory -Path $sourceRoot,$cacheRoot -Force | Out-Null

function Invoke-CodeTool([string]$Tool, [hashtable]$Payload) {
    $json = $Payload | ConvertTo-Json -Compress -Depth 10
    $argsPath = Join-Path $fixtureRoot ("args-" + [guid]::NewGuid().ToString("N") + ".json")
    [IO.File]::WriteAllText($argsPath, $json, [Text.UTF8Encoding]::new($false))
    $previousErrorAction = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try {
        $output = & node $sidecarRunner $EnginePath $Tool $argsPath 2>&1
        $exitCode = $LASTEXITCODE
    }
    finally {
        $ErrorActionPreference = $previousErrorAction
    }
    if ($exitCode -ne 0) {
        throw "$Tool failed: $($output -join [Environment]::NewLine)"
    }
    foreach ($line in @($output)) {
        $text = [string]$line
        if ($text.TrimStart().StartsWith("{") -or $text.TrimStart().StartsWith("[")) {
            try {
                return $text | ConvertFrom-Json
            }
            catch {
                continue
            }
        }
    }
    throw "$Tool did not return JSON."
}

try {
    $fixtureSource = @'
const express = require("express");
const app = express();

function saveOrder(order) {
  const sql = "SELECT id, customer_id FROM orders WHERE customer_id = ?";
  return { order, sql };
}

function createOrder(request, response) {
  const saved = saveOrder(request.body);
  response.json(saved);
}

app.post("/orders", createOrder);
'@
    [IO.File]::WriteAllText(
        (Join-Path $sourceRoot "server.js"),
        $fixtureSource,
        [Text.UTF8Encoding]::new($false)
    )
    $migrationRoot = Join-Path $sourceRoot "migrations"
    New-Item -ItemType Directory -Path $migrationRoot -Force | Out-Null
    [IO.File]::WriteAllText(
        (Join-Path $migrationRoot "001_orders.sql"),
        "CREATE INDEX idx_orders_customer_id ON orders(customer_id);",
        [Text.UTF8Encoding]::new($false)
    )

    $env:CBM_CACHE_DIR = $cacheRoot
    $index = Invoke-CodeTool "index_repository" @{
        repo_path = $sourceRoot
        path = $sourceRoot
        project = "backend-visual-map-contract"
        project_name = "backend-visual-map-contract"
        cache_path = $cacheRoot
        cache_dir = $cacheRoot
    }
    $project = [string]$index.project
    if ([string]::IsNullOrWhiteSpace($project)) {
        throw "index_repository did not return project."
    }

    $base = @{ project = $project; cache_path = $cacheRoot; cache_dir = $cacheRoot }
    foreach ($label in @("Route", "Function", "File")) {
        $payload = $base.Clone()
        $payload.label = $label
        $payload.limit = 500
        $payload.offset = 0
        $result = Invoke-CodeTool "search_graph" $payload
        foreach ($item in @($result.results)) {
            if ($item.label -ne $label) {
                throw "search_graph label drift: expected $label, got $($item.label)"
            }
        }
    }

    $queries = @(
        @{
            Name = "CALLS"
            Query = "MATCH (caller)-[:CALLS]->(callee) RETURN caller.qualified_name AS source, callee.qualified_name AS target LIMIT 100000"
            Columns = @("source", "target")
        },
        @{
            Name = "HANDLES"
            Query = "MATCH (handler)-[:HANDLES]->(route) RETURN handler.qualified_name AS source, route.qualified_name AS target LIMIT 100000"
            Columns = @("source", "target")
        },
        @{
            Name = "SOURCE_LOCATIONS"
            Query = "MATCH (node) RETURN node.qualified_name AS source, node.file_path AS path, node.start_line AS start_line, node.start_column AS start_column, node.end_line AS end_line, node.end_column AS end_column LIMIT 100000"
            Columns = @("source", "path", "start_line", "start_column", "end_line", "end_column")
        }
    )
    foreach ($contract in $queries) {
        $payload = $base.Clone()
        $payload.query = $contract.Query
        $result = Invoke-CodeTool "query_graph" $payload
        if ((@($result.columns) -join ",") -ne ($contract.Columns -join ",")) {
            throw "$($contract.Name) columns drifted: $(@($result.columns) -join ',')"
        }
        if ($contract.Name -eq "SOURCE_LOCATIONS") {
            $located = @($result.rows) | Where-Object {
                $_.Count -eq 6 -and [string]$_[1] -eq "server.js" -and [int]$_[2] -gt 0
            }
            if ($located.Count -eq 0) {
                throw "SOURCE_LOCATIONS returned no positive line for server.js."
            }
        }
    }

    foreach ($search in @(
        @{
            Name = "TABLE_TEXT"
            Pattern = "(^|[^A-Za-z0-9_])orders([^A-Za-z0-9_]|$)"
            PathFilter = "^(server\.js|migrations/001_orders\.sql)$"
        },
        @{
            Name = "COLUMN_TEXT"
            Pattern = "(^|[^A-Za-z0-9_])customer_id([^A-Za-z0-9_]|$)"
            PathFilter = "^server\.js$"
        }
    )) {
        $payload = $base.Clone()
        $payload.pattern = $search.Pattern
        $payload.path_filter = $search.PathFilter
        $payload.regex = $true
        $payload.mode = "compact"
        $payload.context = 0
        $payload.limit = 32
        $result = Invoke-CodeTool "search_code" $payload
        if ([int]$result.total_grep_matches -lt 1) {
            throw "$($search.Name) search_code returned no grep match."
        }
        $typed = @(@($result.results) | Where-Object {
            -not [string]::IsNullOrWhiteSpace([string]$_.qualified_name) -and
            -not [string]::IsNullOrWhiteSpace([string]$_.label) -and
            -not [string]::IsNullOrWhiteSpace([string]$_.file) -and
            [int]$_.start_line -gt 0 -and
            [int]$_.end_line -ge [int]$_.start_line -and
            @($_.match_lines).Count -gt 0
        })
        if ($typed.Count -eq 0) {
            throw "$($search.Name) search_code returned no typed graph location."
        }
        if (@($result.results | Where-Object {
            $_.PSObject.Properties.Name -contains "source" -or
            $_.PSObject.Properties.Name -contains "context"
        }).Count -gt 0) {
            throw "$($search.Name) compact search_code leaked source/context."
        }
    }

    Write-Host "Code engine contract smoke passed for project $project."
}
finally {
    Remove-Item Env:CBM_CACHE_DIR -ErrorAction SilentlyContinue
    if (-not $KeepFixture) {
        $resolvedFixture = [IO.Path]::GetFullPath($fixtureRoot)
        if ($resolvedFixture.StartsWith($tempBase, [StringComparison]::OrdinalIgnoreCase)) {
            Remove-Item -LiteralPath $resolvedFixture -Recurse -Force -ErrorAction SilentlyContinue
        }
    }
}
