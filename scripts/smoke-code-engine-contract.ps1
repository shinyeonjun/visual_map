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
    if (($output -join [Environment]::NewLine) -match "raw JSON.+deprecated") {
        throw "$Tool used the deprecated raw JSON transport."
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
from fastapi import FastAPI

app = FastAPI()

def save_order(order):
    sql = "SELECT id, customer_id FROM orders WHERE customer_id = ?"
    return {"order": order, "sql": sql}

@app.post("/orders")
def create_order(request):
    return save_order(request)
'@
    [IO.File]::WriteAllText(
        (Join-Path $sourceRoot "server.py"),
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
    $env:CBM_ALLOWED_ROOT = $sourceRoot
    $index = Invoke-CodeTool "index_repository" @{
        repo_path = $sourceRoot
        mode = "full"
        name = "backend-visual-map-contract"
        persistence = $false
    }
    $project = [string]$index.project
    if ([string]::IsNullOrWhiteSpace($project)) {
        throw "index_repository did not return project."
    }

    $base = @{ project = $project }
    $architecture = Invoke-CodeTool "get_architecture" $base
    if ($null -eq $architecture) {
        throw "get_architecture returned no result."
    }

    $queries = @(
        @{
            Name = "NODES"
            Query = "MATCH (node:Route|Function|Method|Class|Struct|Interface|Trait|Protocol|Record|Enum|Type|Constructor|Subroutine|Procedure|Decorator|Field|Variable|Module|Namespace|Package|Resource|File) RETURN labels(node) AS labels, node.name AS name, node.qualified_name AS qualified_name, node.file_path AS file_path, node.start_line AS start_line, node.start_column AS start_column, node.end_line AS end_line, node.end_column AS end_column, node.method AS method, node.source AS source, node.parent_qualified_name AS parent_qualified_name, node.parent_class AS parent_class, node.module AS module, node.namespace AS namespace, node.package AS package, node.route_path AS route_path, node.route_method AS route_method, node.signature AS signature, node.return_type AS return_type, node.is_test AS is_test LIMIT 100000"
            Columns = @("labels", "name", "qualified_name", "file_path", "start_line", "start_column", "end_line", "end_column", "method", "source", "parent_qualified_name", "parent_class", "module", "namespace", "package", "route_path", "route_method", "signature", "return_type", "is_test")
        },
        @{
            Name = "CALLS"
            Query = "MATCH (caller)-[rel:CALLS]->(callee) RETURN caller.qualified_name AS source, callee.qualified_name AS target, rel.confidence AS confidence, rel.strategy AS strategy, rel.callee AS call_expression LIMIT 100000"
            Columns = @("source", "target", "confidence", "strategy", "call_expression")
        },
        @{
            Name = "HANDLES"
            Query = "MATCH (handler)-[:HANDLES]->(route) RETURN handler.qualified_name AS source, route.qualified_name AS target LIMIT 100000"
            Columns = @("source", "target")
        }
    )
    foreach ($contract in $queries) {
        $payload = $base.Clone()
        $payload.query = $contract.Query
        $result = Invoke-CodeTool "query_graph" $payload
        if ((@($result.columns) -join ",") -ne ($contract.Columns -join ",")) {
            throw "$($contract.Name) columns drifted: $(@($result.columns) -join ',')"
        }
        if ($contract.Name -eq "NODES") {
            foreach ($label in @("Route", "Function", "File")) {
                if (@($result.rows | Where-Object { [string]$_[0] -match $label }).Count -eq 0) {
                    throw "NODES returned no $label node."
                }
            }
            $located = @($result.rows) | Where-Object {
                $_.Count -eq 20 -and [string]$_[3] -eq "server.py" -and [int]$_[4] -gt 0
            }
            if ($located.Count -eq 0) {
                throw "NODES returned no positive line for server.py."
            }
        }
        if ($contract.Name -eq "CALLS") {
            $scored = @($result.rows) | Where-Object {
                $score = 0.0
                $validScore = [double]::TryParse([string]$_[2], [ref]$score)
                $_.Count -eq 5 -and
                $validScore -and
                $score -ge 0 -and $score -le 1 -and
                -not [string]::IsNullOrWhiteSpace([string]$_[3]) -and
                -not [string]::IsNullOrWhiteSpace([string]$_[4])
            }
            if ($scored.Count -eq 0) {
                throw "CALLS returned no scored relationship evidence."
            }
        }
        if ($contract.Name -eq "HANDLES" -and @($result.rows).Count -eq 0) {
            throw "HANDLES returned no route-to-handler evidence."
        }
    }

    foreach ($search in @(
        @{
            Name = "TABLE_TEXT"
            Pattern = "(^|[^A-Za-z0-9_])orders([^A-Za-z0-9_]|$)"
            PathFilter = "^(server\.py|migrations/001_orders\.sql)$"
        },
        @{
            Name = "COLUMN_TEXT"
            Pattern = "(^|[^A-Za-z0-9_])customer_id([^A-Za-z0-9_]|$)"
            PathFilter = "^server\.py$"
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

    $nextIndex = Invoke-CodeTool "index_repository" @{
        repo_path = $sourceRoot
        mode = "full"
        name = "backend-visual-map-contract-next"
        persistence = $false
    }
    $nextProject = [string]$nextIndex.project
    if ([string]::IsNullOrWhiteSpace($nextProject) -or $nextProject -eq $project) {
        throw "A fresh code index did not create a distinct project generation."
    }
    $nextRoutes = Invoke-CodeTool "query_graph" @{
        project = $nextProject
        query = "MATCH (route:Route) RETURN route.qualified_name AS route"
    }
    if (@($nextRoutes.rows).Count -eq 0) {
        throw "The fresh project generation returned no Route node."
    }

    Invoke-CodeTool "delete_project" @{ project = $project } | Out-Null
    $projects = Invoke-CodeTool "list_projects" @{}
    $remaining = @($projects.projects)
    if (@($remaining | Where-Object { [string]$_.name -eq $project }).Count -ne 0) {
        throw "The previous project generation was not removed."
    }
    if (@($remaining | Where-Object { [string]$_.name -eq $nextProject }).Count -ne 1) {
        throw "The fresh project generation was not preserved."
    }

    Write-Host "Code engine contract smoke passed for project $nextProject."
}
finally {
    Remove-Item Env:CBM_CACHE_DIR -ErrorAction SilentlyContinue
    Remove-Item Env:CBM_ALLOWED_ROOT -ErrorAction SilentlyContinue
    if (-not $KeepFixture) {
        $resolvedFixture = [IO.Path]::GetFullPath($fixtureRoot)
        if ($resolvedFixture.StartsWith($tempBase, [StringComparison]::OrdinalIgnoreCase)) {
            Remove-Item -LiteralPath $resolvedFixture -Recurse -Force -ErrorAction SilentlyContinue
        }
    }
}
