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

  $expectedCommands = @(
    "contract",
    "index",
    "list-snapshots",
    "describe-snapshot",
    "list-objects",
    "find-objects",
    "describe-object"
  )
  $commands = @($contract.commands)
  $missingCommands = @($expectedCommands | Where-Object { $commands -notcontains $_ })
  $legacyProperties = @($contract.PSObject.Properties.Name | Where-Object {
      $_ -in @("traversal_limits", "inventory_limits")
    })
  if ($contract.contract_version -ne 3 -or
      $contract.complete_snapshot_contract_version -ne 2 -or
      $contract.metadata_only -ne $true -or
      $contract.row_data_access -ne $false -or
      @($contract.authoritative_outcomes).Count -ne 2 -or
      @($contract.authoritative_outcomes) -notcontains "complete" -or
      @($contract.authoritative_outcomes) -notcontains "failed" -or
      $commands.Count -ne $expectedCommands.Count -or
      $missingCommands.Count -ne 0 -or
      $legacyProperties.Count -ne 0) {
    Write-Output "FAIL CLI contract: canonical metadata-only adapter boundary did not match contract v3."
    $script:Failures += 1
    return
  }
  Write-Output "PASS CLI contract: one canonical metadata-only adapter surface verified."
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
    Write-Output "FAIL authoritative snapshot: complete snapshot contract-v2 evidence is missing behind adapter v3."
    $script:Failures += 1
    return
  }

  $page = Invoke-JsonCommand -Label "table object page" -Arguments @(
    "list-objects", $Alias, "--kind", "table", "--limit", "100",
    "--format", "json", "--cache-path", $CachePath
  )
  if (-not $page) { return }
  $tables = @($page.objects)
  if ($page.contract_version -ne 3 -or
      $page.page.offset -ne 0 -or
      $page.page.has_more -ne $false -or
      $tables.Count -lt 2) {
    Write-Output "FAIL table object page: version, pagination, or table count did not match the smoke schema."
    $script:Failures += 1
    return
  }

  $exhausted = Invoke-JsonCommand -Label "exhausted object page" -Arguments @(
    "list-objects", $Alias, "--kind", "table", "--offset", [string]$page.page.total,
    "--limit", "100", "--format", "json", "--cache-path", $CachePath
  )
  if (-not $exhausted -or @($exhausted.objects).Count -ne 0 -or $exhausted.page.has_more -ne $false) {
    Write-Output "FAIL table object page: exhausted page did not terminate cleanly."
    $script:Failures += 1
    return
  }

  $orders = $tables | Where-Object { $_.object_name -eq "orders" } | Select-Object -First 1
  if (-not $orders -or [string]::IsNullOrWhiteSpace([string]$orders.object_key)) {
    Write-Output "FAIL table object page: stable orders table key is missing."
    $script:Failures += 1
    return
  }

  $detail = Invoke-JsonCommand -Label "stable-key object detail" -Arguments @(
    "describe-object", $Alias, [string]$orders.object_key,
    "--relationship-limit", "200", "--format", "json", "--cache-path", $CachePath
  )
  if (-not $detail) { return }
  $relationships = @($detail.incoming) + @($detail.outgoing)
  if ($detail.contract_version -ne 3 -or
      $detail.object.object_key -ne $orders.object_key -or
      @($relationships | Where-Object { $_.edge_type -eq "VIEW_DEPENDS_ON_TABLE" }).Count -lt 1 -or
      @($relationships | Where-Object { $_.edge_type -eq "TRIGGER_ON_TABLE" }).Count -lt 1) {
    Write-Output "FAIL object detail: stable identity or direct view/trigger relationship evidence is missing."
    $script:Failures += 1
    return
  }

  Write-Output "PASS DB evidence contract: bounded object pages and evidence-backed relationships verified."
}

function Test-NonEmptyObjects {
  param(
    [string]$Label,
    [string]$Alias,
    [string]$CachePath
  )

  $tables = Invoke-JsonCommand -Label "$Label table objects" -Arguments @(
    "list-objects", $Alias, "--kind", "table", "--limit", "1",
    "--format", "json", "--cache-path", $CachePath
  )
  $columns = Invoke-JsonCommand -Label "$Label column objects" -Arguments @(
    "list-objects", $Alias, "--kind", "column", "--limit", "1",
    "--format", "json", "--cache-path", $CachePath
  )
  if (-not $tables -or -not $columns) { return }
  $firstTable = @($tables.objects) | Select-Object -First 1
  $firstColumn = @($columns.objects) | Select-Object -First 1
  if ($tables.contract_version -ne 3 -or
      $tables.page.total -lt 1 -or
      $columns.page.total -lt 1 -or
      -not $firstTable -or
      -not $firstColumn -or
      [string]::IsNullOrWhiteSpace([string]$firstTable.object_key) -or
      [string]::IsNullOrWhiteSpace([string]$firstColumn.object_key)) {
    Write-Output "FAIL ${Label}: indexing completed but table/column facts were not returned."
    $script:Failures += 1
    return
  }
  Write-Output "PASS ${Label}: non-empty table and column facts verified."
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

  $tables = Invoke-JsonCommand -Label "reserved identifier table objects" -Arguments @(
    "list-objects", $snapshot, "--kind", "table", "--limit", "10",
    "--format", "json", "--cache-path", $cachePath
  )
  if (-not $tables) { return }
  $table = @($tables.objects) | Where-Object { $_.object_name -eq "order:events" } | Select-Object -First 1
  $expectedKey = "v2:sqlite:smoke-reserved:main:main:table:order%3Aevents"
  if (-not $table -or $table.object_key -ne $expectedKey) {
    Write-Output "FAIL reserved identifiers: versioned stable table identity was not preserved."
    $script:Failures += 1
    return
  }

  $detail = Invoke-JsonCommand -Label "reserved identifier detail" -Arguments @(
    "describe-object", $snapshot, $expectedKey,
    "--format", "json", "--cache-path", $cachePath
  )
  $columns = Invoke-JsonCommand -Label "reserved identifier column search" -Arguments @(
    "find-objects", $snapshot, "value:raw%text", "--kind", "column", "--limit", "10",
    "--format", "json", "--cache-path", $cachePath
  )
  if (-not $detail -or -not $columns -or
      $detail.object.object_name -ne "order:events" -or
      @($columns.objects | Where-Object { $_.sub_object -eq "value:raw%text" }).Count -ne 1) {
    Write-Output "FAIL reserved identifiers: exact table/column identity did not round-trip."
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
    [switch]$RequireNonEmptyObjects
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
  if ($LASTEXITCODE -ne 0) {
    Write-Output "FAIL ${Label}: database-memory exited with code $LASTEXITCODE. Output suppressed to avoid leaking secrets."
    $script:Failures += 1
    return
  }

  try {
    $index = Get-Content -LiteralPath $stdoutPath -Raw | ConvertFrom-Json
  } catch {
    Write-Output "FAIL ${Label}: index output was not valid JSON."
    $script:Failures += 1
    return
  }
  if ($index.contract_version -ne 3 -or
      $index.status -ne "complete" -or
      $index.requested_source -ne $Source -or
      $index.analyzed_source -ne $Source -or
      $index.completeness.status -ne "complete") {
    Write-Output "FAIL ${Label}: index did not return an authoritative contract-v3 snapshot."
    $script:Failures += 1
    return
  }

  Write-Output "PASS ${Label}: metadata index completed."
  if ($RequireNonEmptyObjects) {
    $failuresBeforeObjects = $script:Failures
    Test-NonEmptyObjects -Label $Label -Alias "$Source`:$Alias" -CachePath $cachePath
    if ($script:Failures -gt $failuresBeforeObjects) { return }
  }
  [void]$script:PassedSources.Add($Source)
  if ($ValidateProductContract) {
    Test-IndexedProductContract -Alias "$Source`:$Alias" -CachePath $cachePath
  }
}

try {
  New-Item -ItemType Directory -Force -Path $tempRoot | Out-Null
  Test-ProductContract
  Test-ReservedIdentifierContract
  $ddlSmokePath = Join-Path $PSScriptRoot "fixtures\product-smoke-schema.sql"

  Invoke-IndexSmoke -Label "SQLite" -Source "sqlite" -Alias "smoke-sqlite" `
    -Path ([System.Environment]::GetEnvironmentVariable("CODEBASE_WORKSPACE_TEST_SQLITE_PATH")) `
    -PathEnvVar "CODEBASE_WORKSPACE_TEST_SQLITE_PATH" -ConnectionEnvVar ""
  Invoke-IndexSmoke -Label "SQLite DDL" -Source "ddl-sqlite" -Alias "smoke-ddl-sqlite" `
    -Path $ddlSmokePath -ConnectionEnvVar "" -ValidateProductContract
  Invoke-IndexSmoke -Label "PostgreSQL" -Source "postgres" -Alias "smoke-postgres" `
    -Path "" -ConnectionEnvVar "CODEBASE_WORKSPACE_TEST_POSTGRES_URL" -RequireNonEmptyObjects
  Invoke-IndexSmoke -Label "YugabyteDB YSQL" -Source "yugabytedb" -Alias "smoke-yugabytedb" `
    -Path "" -ConnectionEnvVar "CODEBASE_WORKSPACE_TEST_YUGABYTEDB_URL" -RequireNonEmptyObjects
  Invoke-IndexSmoke -Label "MySQL" -Source "mysql" -Alias "smoke-mysql" `
    -Path "" -ConnectionEnvVar "CODEBASE_WORKSPACE_TEST_MYSQL_URL" -RequireNonEmptyObjects
  Invoke-IndexSmoke -Label "MariaDB" -Source "mariadb" -Alias "smoke-mariadb" `
    -Path "" -ConnectionEnvVar "CODEBASE_WORKSPACE_TEST_MARIADB_URL" -RequireNonEmptyObjects
  Invoke-IndexSmoke -Label "SQL Server" -Source "sqlserver" -Alias "smoke-sqlserver" `
    -Path "" -ConnectionEnvVar "CODEBASE_WORKSPACE_TEST_SQLSERVER_URL" -RequireNonEmptyObjects
  Invoke-IndexSmoke -Label "Oracle" -Source "oracle" -Alias "smoke-oracle" `
    -Path "" -ConnectionEnvVar "CODEBASE_WORKSPACE_TEST_ORACLE_URL" -RequireNonEmptyObjects
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

if ($script:Failures -gt 0) { exit 1 }

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
