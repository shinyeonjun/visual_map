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
  if ($contract.contract_version -ne 2 -or
      $contract.complete_snapshot_contract_version -ne 2 -or
      $contract.metadata_only -ne $true -or
      $contract.row_data_access -ne $false -or
      @($contract.authoritative_outcomes).Count -ne 2 -or
      @($contract.authoritative_outcomes) -notcontains "complete" -or
      @($contract.authoritative_outcomes) -notcontains "failed" -or
      $contract.inventory_limits.offset_pagination -ne $true -or
      $commands -notcontains "inventory" -or
      $commands -notcontains "describe-snapshot" -or
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

  $snapshot = Invoke-JsonCommand -Label "authoritative snapshot" -Arguments @(
    "describe-snapshot", $Alias, "--format", "json", "--cache-path", $CachePath
  )
  if (-not $snapshot -or
      $snapshot.snapshot.contract_version -ne 2 -or
      $snapshot.snapshot.authority -ne "complete" -or
      $snapshot.completeness.status -ne "complete") {
    Write-Output "FAIL authoritative snapshot: complete contract-v2 evidence is missing."
    $script:Failures += 1
    return
  }

  $inventory = Invoke-JsonCommand -Label "bulk inventory" -Arguments @(
    "inventory", $Alias, "--limit", "100", "--format", "json", "--cache-path", $CachePath
  )
  if (-not $inventory) { return }
  $tables = @($inventory.tables)
  if ($inventory.contract_version -ne 2 -or
      $inventory.offset -ne 0 -or
      $inventory.has_more -ne $false -or
      $null -ne $inventory.next_offset -or
      $inventory.truncated -ne $false -or
      $tables.Count -lt 2) {
    Write-Output "FAIL bulk inventory: version/count/truncation contract did not match the smoke schema."
    $script:Failures += 1
    return
  }
  $exhausted = Invoke-JsonCommand -Label "exhausted inventory page" -Arguments @(
    "inventory", $Alias, "--offset", [string]$inventory.total_tables, "--limit", "100",
    "--format", "json", "--cache-path", $CachePath
  )
  if (-not $exhausted -or @($exhausted.tables).Count -ne 0 -or $exhausted.has_more -ne $false) {
    Write-Output "FAIL bulk inventory: exhausted page did not terminate cleanly."
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
  $inventoryDependents = @($orders.dependents)
  if (@($inventoryDependents | Where-Object { $_.kind -eq "view" -and $_.name -eq "active_orders" }).Count -ne 1 -or
      @($inventoryDependents | Where-Object { $_.kind -eq "trigger" -and $_.name -eq "trg_orders_status" }).Count -ne 1) {
    Write-Output "FAIL bulk inventory: direct view or trigger dependent evidence is missing."
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
  $describeDependents = @($describe.dependents)
  if (@($describeDependents | Where-Object { $_.kind -eq "view" -and $_.name -eq "active_orders" }).Count -ne 1 -or
      @($describeDependents | Where-Object { $_.kind -eq "trigger" -and $_.name -eq "trg_orders_status" }).Count -ne 1) {
    Write-Output "FAIL stable-key describe: direct view or trigger dependent evidence is missing."
    $script:Failures += 1
    return
  }
  $impactNodes = @($impact.groups | ForEach-Object { @($_.nodes) })
  if (@($impactNodes | Where-Object { $_.label -eq "View" -and $_.display_name -eq "active_orders" }).Count -ne 1 -or
      @($impactNodes | Where-Object { $_.label -eq "Trigger" -and $_.display_name -eq "trg_orders_status" }).Count -ne 1) {
    Write-Output "FAIL DB evidence commands: SQLite view or trigger impact evidence is missing."
    $script:Failures += 1
    return
  }
  Write-Output "PASS DB evidence contract: bulk inventory, stable describe, impact and trace verified."
}

function Test-NonEmptyInventory {
  param(
    [string]$Label,
    [string]$Alias,
    [string]$CachePath
  )

  $inventory = Invoke-JsonCommand -Label "$Label inventory" -Arguments @(
    "inventory", $Alias, "--limit", "1", "--format", "json", "--cache-path", $CachePath
  )
  if (-not $inventory) { return }
  $tables = @($inventory.tables)
  $firstTable = $tables | Select-Object -First 1
  if ($inventory.contract_version -ne 2 -or
      $inventory.total_tables -lt 1 -or
      $tables.Count -ne 1 -or
      -not $firstTable -or
      [string]::IsNullOrWhiteSpace([string]$firstTable.table_key) -or
      @($firstTable.columns).Count -lt 1) {
    Write-Output "FAIL ${Label}: indexing completed but no table with column metadata was returned."
    $script:Failures += 1
    return
  }
  Write-Output "PASS ${Label}: non-empty table and column metadata verified."
}

function Test-ReservedIdentifierContract {
  $alias = "smoke-reserved"
  $snapshot = "ddl-sqlite:$alias"
  $ddlPath = Join-Path $tempRoot "reserved-identifiers.sql"
  $cachePath = Join-Path $tempRoot "reserved-identifiers.sqlite"
  $stdoutPath = Join-Path $tempRoot "reserved-identifiers.out"
  $stderrPath = Join-Path $tempRoot "reserved-identifiers.err"
  [System.IO.File]::WriteAllText(
    $ddlPath,
    'CREATE TABLE "order:events" ("value:raw%text" TEXT);',
    [System.Text.UTF8Encoding]::new($false)
  )

  & $cli.Source index --format json --source ddl-sqlite --alias $alias --path $ddlPath --cache-path $cachePath `
    1>$stdoutPath 2>$stderrPath
  if ($LASTEXITCODE -ne 0) {
    Write-Output "FAIL reserved identifiers: metadata index failed."
    $script:Failures += 1
    return
  }

  $inventory = Invoke-JsonCommand -Label "reserved identifier inventory" -Arguments @(
    "inventory", $snapshot, "--limit", "10", "--format", "json", "--cache-path", $cachePath
  )
  if (-not $inventory) { return }
  $table = @($inventory.tables) | Where-Object { $_.table -eq "order:events" } | Select-Object -First 1
  $expectedKey = "v2:sqlite:smoke-reserved:main:main:table:order%3Aevents"
  if (-not $table -or $table.table_key -ne $expectedKey) {
    Write-Output "FAIL reserved identifiers: versioned stable table identity was not preserved."
    $script:Failures += 1
    return
  }
  $describe = Invoke-JsonCommand -Label "reserved identifier describe" -Arguments @(
    "describe-table", $snapshot, "--object-key", $expectedKey,
    "--format", "json", "--cache-path", $cachePath
  )
  if (-not $describe -or
      $describe.table -ne "order:events" -or
      @($describe.columns | Where-Object { $_.name -eq "value:raw%text" }).Count -ne 1) {
    Write-Output "FAIL reserved identifiers: stable-key describe did not return the exact table and column."
    $script:Failures += 1
    return
  }
  Write-Output "PASS reserved identifiers: versioned stable identity round-trip verified."
}

function Invoke-IndexSmoke {
  param(
    [string]$Label,
    [string]$Source,
    [string]$Alias,
    [string]$Path,
    [string]$PathEnvVar,
    [string]$ConnectionEnvVar,
    [switch]$ValidateProductContract,
    [switch]$RequireNonEmptyInventory
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
    try {
      $index = Get-Content -LiteralPath $stdoutPath -Raw | ConvertFrom-Json
    } catch {
      Write-Output "FAIL ${Label}: index output was not valid JSON."
      $script:Failures += 1
      return
    }
    if ($index.contract_version -ne 2 -or
        $index.status -ne "complete" -or
        $index.requested_source -ne $Source -or
        $index.analyzed_source -ne $Source -or
        $index.completeness.status -ne "complete") {
      Write-Output "FAIL ${Label}: index did not return an authoritative contract-v2 snapshot."
      $script:Failures += 1
      return
    }
    Write-Output "PASS ${Label}: metadata index completed."
    if ($RequireNonEmptyInventory) {
      $failuresBeforeInventory = $script:Failures
      Test-NonEmptyInventory -Label $Label -Alias "$Source`:$Alias" -CachePath $CachePath
      if ($script:Failures -gt $failuresBeforeInventory) {
        return
      }
    }
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
  Test-ReservedIdentifierContract
  $ddlSmokePath = Join-Path $PSScriptRoot "fixtures\product-smoke-schema.sql"

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
    -ConnectionEnvVar "BACKEND_MAP_TEST_POSTGRES_URL" `
    -RequireNonEmptyInventory

  Invoke-IndexSmoke `
    -Label "YugabyteDB YSQL" `
    -Source "yugabytedb" `
    -Alias "smoke-yugabytedb" `
    -Path "" `
    -ConnectionEnvVar "BACKEND_MAP_TEST_YUGABYTEDB_URL" `
    -RequireNonEmptyInventory

  Invoke-IndexSmoke `
    -Label "MySQL" `
    -Source "mysql" `
    -Alias "smoke-mysql" `
    -Path "" `
    -ConnectionEnvVar "BACKEND_MAP_TEST_MYSQL_URL" `
    -RequireNonEmptyInventory

  Invoke-IndexSmoke `
    -Label "MariaDB" `
    -Source "mariadb" `
    -Alias "smoke-mariadb" `
    -Path "" `
    -ConnectionEnvVar "BACKEND_MAP_TEST_MARIADB_URL" `
    -RequireNonEmptyInventory

  Invoke-IndexSmoke `
    -Label "SQL Server" `
    -Source "sqlserver" `
    -Alias "smoke-sqlserver" `
    -Path "" `
    -ConnectionEnvVar "BACKEND_MAP_TEST_SQLSERVER_URL" `
    -RequireNonEmptyInventory

  Invoke-IndexSmoke `
    -Label "Oracle" `
    -Source "oracle" `
    -Alias "smoke-oracle" `
    -Path "" `
    -ConnectionEnvVar "BACKEND_MAP_TEST_ORACLE_URL" `
    -RequireNonEmptyInventory
} finally {
  if (Test-Path -LiteralPath $tempRoot) {
    $resolvedTempRoot = [IO.Path]::GetFullPath($tempRoot)
    $expectedPrefix = Join-Path ([IO.Path]::GetFullPath([IO.Path]::GetTempPath())) "backend-map-rdb-smoke-"
    if (-not $resolvedTempRoot.StartsWith($expectedPrefix, [StringComparison]::OrdinalIgnoreCase)) {
      throw "Refusing to remove unexpected smoke directory: $resolvedTempRoot"
    }
    Remove-Item -LiteralPath $resolvedTempRoot -Recurse -Force
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
