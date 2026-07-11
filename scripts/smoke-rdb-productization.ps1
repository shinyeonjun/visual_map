param(
  [string]$DatabaseMemory = "database-memory",
  [switch]$RequireReleaseMatrix
)

$ErrorActionPreference = "Stop"
$cli = Get-Command $DatabaseMemory -ErrorAction SilentlyContinue

if (-not $cli) {
  Write-Output "SKIP all: database-memory CLI was not found on PATH."
  exit 0
}

$tempRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("backend-map-rdb-smoke-" + [System.Guid]::NewGuid().ToString("N"))
$script:Failures = 0
$script:PassedSources = [System.Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)

function Invoke-JsonCommand {
  param(
    [string]$Label,
    [string[]]$Arguments
  )

  $stdoutPath = Join-Path $tempRoot ("json-" + [System.Guid]::NewGuid().ToString("N") + ".out")
  $stderrPath = Join-Path $tempRoot ("json-" + [System.Guid]::NewGuid().ToString("N") + ".err")
  & $cli.Source @Arguments 1>$stdoutPath 2>$stderrPath
  if ($LASTEXITCODE -ne 0) {
    Write-Output "FAIL ${Label}: database-memory exited with code $LASTEXITCODE. Output suppressed."
    $script:Failures += 1
    return $null
  }
  try {
    return Get-Content -LiteralPath $stdoutPath -Raw | ConvertFrom-Json
  } catch {
    Write-Output "FAIL ${Label}: output was not valid JSON."
    $script:Failures += 1
    return $null
  }
}

function Test-ProductContract {
  $contract = Invoke-JsonCommand -Label "CLI contract" -Arguments @("contract", "--format", "json")
  if (-not $contract) { return }
  $commands = @($contract.commands)
  if ($contract.contract_version -ne 1 -or
      $contract.metadata_only -ne $true -or
      $contract.row_data_access -ne $false -or
      $commands -notcontains "inventory" -or
      $commands -notcontains "impact-analysis" -or
      $commands -notcontains "trace-relationships") {
    Write-Output "FAIL CLI contract: required metadata-only product fields or commands are missing."
    $script:Failures += 1
    return
  }
  Write-Output "PASS CLI contract: versioned metadata-only boundary and product commands verified."
}

function Test-IndexedProductContract {
  param(
    [string]$Alias,
    [string]$CachePath
  )

  $inventory = Invoke-JsonCommand -Label "bulk inventory" -Arguments @(
    "inventory", $Alias, "--limit", "100", "--format", "json", "--cache-path", $CachePath
  )
  if (-not $inventory) { return }
  $tables = @($inventory.tables)
  if ($inventory.contract_version -ne 1 -or $inventory.truncated -ne $false -or $tables.Count -lt 2) {
    Write-Output "FAIL bulk inventory: version/count/truncation contract did not match the smoke schema."
    $script:Failures += 1
    return
  }
  $orders = $tables | Where-Object { $_.table -eq "orders" } | Select-Object -First 1
  if (-not $orders -or [string]::IsNullOrWhiteSpace($orders.table_key)) {
    Write-Output "FAIL bulk inventory: stable orders table key is missing."
    $script:Failures += 1
    return
  }
  if (@($orders.constraints).Count -lt 1 -or @($orders.foreign_keys.outbound).Count -lt 1) {
    Write-Output "FAIL bulk inventory: direct constraint/FK evidence is missing."
    $script:Failures += 1
    return
  }

  $describe = Invoke-JsonCommand -Label "stable-key describe" -Arguments @(
    "describe-table", $Alias, "--object-key", [string]$orders.table_key,
    "--format", "json", "--cache-path", $CachePath
  )
  $impact = Invoke-JsonCommand -Label "bounded impact" -Arguments @(
    "impact-analysis", $Alias, "--object-key", [string]$orders.table_key,
    "--max-depth", "2", "--limit", "20", "--cache-path", $CachePath
  )
  $trace = Invoke-JsonCommand -Label "bounded relationship trace" -Arguments @(
    "trace-relationships", $Alias, [string]$orders.table_key,
    "--max-depth", "2", "--limit", "20", "--cache-path", $CachePath
  )
  if (-not $describe -or -not $impact -or -not $trace) { return }
  if ($describe.table_key -ne $orders.table_key -or
      $impact.object_key -ne $orders.table_key -or
      $trace.start_object_key -ne $orders.table_key) {
    Write-Output "FAIL DB evidence commands: stable object identity changed between commands."
    $script:Failures += 1
    return
  }
  Write-Output "PASS DB evidence contract: bulk inventory, stable describe, impact and trace verified."
}

function Invoke-IndexSmoke {
  param(
    [string]$Label,
    [string]$Source,
    [string]$Alias,
    [string]$Path,
    [string]$PathEnvVar,
    [string]$ConnectionEnvVar,
    [switch]$ValidateProductContract
  )

  $cachePath = Join-Path $tempRoot "$Alias.sqlite"
  $stdoutPath = Join-Path $tempRoot "$Alias.out"
  $stderrPath = Join-Path $tempRoot "$Alias.err"
  $args = @("index", "--format", "json", "--source", $Source, "--alias", $Alias, "--cache-path", $cachePath)

  if ($Path) {
    if (-not (Test-Path -LiteralPath $Path)) {
      Write-Output "SKIP ${Label}: input path is missing."
      return
    }
    $args += @("--path", $Path)
  } elseif ($PathEnvVar) {
    Write-Output "SKIP ${Label}: $PathEnvVar is not set."
    return
  } else {
    $connectionString = [System.Environment]::GetEnvironmentVariable($ConnectionEnvVar)
    if ([string]::IsNullOrWhiteSpace($connectionString)) {
      Write-Output "SKIP ${Label}: $ConnectionEnvVar is not set."
      return
    }
    $args += @("--connection-string", $connectionString)
  }

  & $cli.Source @args 1>$stdoutPath 2>$stderrPath
  if ($LASTEXITCODE -eq 0) {
    Write-Output "PASS ${Label}: metadata index completed."
    [void]$script:PassedSources.Add($Source)
    if ($ValidateProductContract) {
      Test-IndexedProductContract -Alias "$Source`:$Alias" -CachePath $cachePath
    }
  } else {
    Write-Output "FAIL ${Label}: database-memory exited with code $LASTEXITCODE. Output suppressed to avoid leaking secrets."
    $script:Failures += 1
  }
}

try {
  New-Item -ItemType Directory -Force -Path $tempRoot | Out-Null
  Test-ProductContract
  $ddlSmokePath = Join-Path $tempRoot "schema.sql"
  @"
CREATE TABLE users (
  id INTEGER PRIMARY KEY,
  email TEXT NOT NULL
);
CREATE TABLE orders (
  id INTEGER PRIMARY KEY,
  user_id INTEGER NOT NULL REFERENCES users(id)
);
"@ | Set-Content -LiteralPath $ddlSmokePath -Encoding UTF8

  Invoke-IndexSmoke `
    -Label "SQLite" `
    -Source "sqlite" `
    -Alias "smoke-sqlite" `
    -Path ([System.Environment]::GetEnvironmentVariable("BACKEND_MAP_TEST_SQLITE_PATH")) `
    -PathEnvVar "BACKEND_MAP_TEST_SQLITE_PATH" `
    -ConnectionEnvVar ""

  Invoke-IndexSmoke `
    -Label "SQLite DDL" `
    -Source "ddl-sqlite" `
    -Alias "smoke-ddl-sqlite" `
    -Path $ddlSmokePath `
    -ConnectionEnvVar "" `
    -ValidateProductContract

  Invoke-IndexSmoke `
    -Label "PostgreSQL" `
    -Source "postgres" `
    -Alias "smoke-postgres" `
    -Path "" `
    -ConnectionEnvVar "BACKEND_MAP_TEST_POSTGRES_URL"

  Invoke-IndexSmoke `
    -Label "MySQL/MariaDB" `
    -Source "mysql" `
    -Alias "smoke-mysql" `
    -Path "" `
    -ConnectionEnvVar "BACKEND_MAP_TEST_MYSQL_URL"

  Invoke-IndexSmoke `
    -Label "SQL Server" `
    -Source "sqlserver" `
    -Alias "smoke-sqlserver" `
    -Path "" `
    -ConnectionEnvVar "BACKEND_MAP_TEST_SQLSERVER_URL"

  Invoke-IndexSmoke `
    -Label "Oracle" `
    -Source "oracle" `
    -Alias "smoke-oracle" `
    -Path "" `
    -ConnectionEnvVar "BACKEND_MAP_TEST_ORACLE_URL"
} finally {
  if (Test-Path -LiteralPath $tempRoot) {
    Remove-Item -LiteralPath $tempRoot -Recurse -Force
  }
}

if ($script:Failures -gt 0) {
  exit 1
}

if ($RequireReleaseMatrix) {
  if (-not $script:PassedSources.Contains("postgres")) {
    Write-Output "FAIL release matrix: PostgreSQL smoke is required."
    exit 1
  }
  if (-not @("mysql", "sqlserver", "oracle").Where({ $script:PassedSources.Contains($_) }, "First")) {
    Write-Output "FAIL release matrix: one additional network DB smoke is required."
    exit 1
  }
}

exit 0
