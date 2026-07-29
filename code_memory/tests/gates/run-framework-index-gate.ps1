param(
    [string]$Bridge = (Join-Path $PSScriptRoot '..\..\rust\target\release\code-memory-language.exe'),
    [string]$Root = (Join-Path $PSScriptRoot '..\..'),
    [string]$ProvidersRoot = '',
    [string]$CppProvidersRoot = ''
)

$ErrorActionPreference = 'Stop'
$bundledProviders = Join-Path $Root 'providers'
if ([string]::IsNullOrWhiteSpace($ProvidersRoot) -and (Test-Path $bundledProviders)) {
    $ProvidersRoot = $bundledProviders
}
$fixture = Join-Path $Root 'tests\fixtures\framework\typescript-express'
$out = Join-Path $Root 'build\framework-gate\typescript-express.json'
New-Item -ItemType Directory -Force -Path (Split-Path $out) | Out-Null
$providerArgs = @()
if (-not [string]::IsNullOrWhiteSpace($ProvidersRoot)) {
    $providerArgs = @('--providers-root', (Resolve-Path $ProvidersRoot).Path)
}
$cppProviderArgs = $providerArgs
if (-not [string]::IsNullOrWhiteSpace($CppProvidersRoot)) {
    $cppProviderArgs = @('--providers-root', (Resolve-Path $CppProvidersRoot).Path)
}

& $Bridge index --root $fixture --out $out --packs-root $Root @providerArgs
if ($LASTEXITCODE -ne 0) { throw 'framework index command failed' }

$result = Get-Content $out -Raw | ConvertFrom-Json
$pack = @($result.frameworks | Where-Object id -eq 'express') | Select-Object -First 1
if ($null -eq $pack -or $pack.status -ne 'detected') { throw 'Express pack was not detected' }
if ($pack.adapter -ne 'registration-routing') { throw 'Express adapter was not selected' }
$fact = @($pack.facts | Where-Object { $_.kind -eq 'HTTP_ROUTE' -and $_.path -eq '/health' }) | Select-Object -First 1
if ($null -eq $fact -or $fact.symbol -notmatch 'health') { throw 'Express route fact was not resolved' }
if (@($fact.source_range).Count -ne 4) { throw 'Express route source range was not recorded' }
if (@($fact.evidence).Count -eq 0) { throw 'Express route evidence was not recorded' }
$edge = @($result.framework_relations | Where-Object { $_.kind -eq 'HANDLES' -and $_.framework -eq 'express' }) | Select-Object -First 1
if ($null -eq $edge -or $edge.from -notmatch 'health' -or $edge.to -ne $fact.id) { throw 'Express HANDLES relation was not emitted' }
$unknown = @($pack.facts | Where-Object { $_.kind -eq 'HTTP_ROUTE' -and $_.path -eq '/unknown' }) | Select-Object -First 1
if ($null -eq $unknown -or $null -ne $unknown.symbol) { throw 'unresolved handler was incorrectly fabricated' }
if (@($result.framework_relations | Where-Object { $_.to -eq $unknown.id }).Count -ne 0) { throw 'unresolved route received HANDLES' }
$middleware = @($pack.facts | Where-Object {
    $_.kind -eq 'MIDDLEWARE' -and $_.properties.target -eq 'authMiddleware'
}) | Select-Object -First 1
if ($null -eq $middleware -or $middleware.symbol -notmatch 'authMiddleware' -or
    $middleware.properties.target -ne 'authMiddleware') { throw 'Express middleware was not resolved' }
if (@($result.framework_relations | Where-Object { $_.kind -eq 'USES_MIDDLEWARE' -and $_.from -match 'authMiddleware' }).Count -eq 0) {
    throw 'Express middleware relation was not emitted'
}

Write-Host "PASS typescript/express: $($fact.method) $($fact.path) -> $($fact.symbol)"

$rustFixture = Join-Path $Root 'tests\fixtures\framework\rust-axum'
$rustOut = Join-Path $Root 'build\framework-gate\rust-axum.json'
& $Bridge index --root $rustFixture --out $rustOut --packs-root $Root @providerArgs
if ($LASTEXITCODE -ne 0) { throw 'Rust/Axum framework index command failed' }

$rustResult = Get-Content $rustOut -Raw | ConvertFrom-Json
$axum = @($rustResult.frameworks | Where-Object id -eq 'axum') | Select-Object -First 1
if ($null -eq $axum -or $axum.status -ne 'detected') { throw 'Axum pack was not detected' }
$axumFact = @($axum.facts | Where-Object { $_.kind -eq 'HTTP_ROUTE' -and $_.path -eq '/health' }) | Select-Object -First 1
if ($null -eq $axumFact -or $axumFact.symbol -notmatch 'health') { throw 'Axum cross-file route was not resolved' }
if (@($axumFact.source_range).Count -ne 4) { throw 'Axum route source range was not recorded' }
$axumEdge = @($rustResult.framework_relations | Where-Object { $_.kind -eq 'HANDLES' -and $_.framework -eq 'axum' }) | Select-Object -First 1
if ($null -eq $axumEdge -or $axumEdge.from -notmatch 'health' -or $axumEdge.to -ne $axumFact.id) { throw 'Axum HANDLES relation was not emitted' }

Write-Host "PASS rust/axum: $($axumFact.method) $($axumFact.path) -> $($axumFact.symbol)"

$pythonFixture = Join-Path $Root 'tests\fixtures\framework\python-flask'
$pythonOut = Join-Path $Root 'build\framework-gate\python-flask.json'
& $Bridge index --root $pythonFixture --out $pythonOut --packs-root $Root @providerArgs
if ($LASTEXITCODE -ne 0) { throw 'Python/Flask framework index command failed' }

$pythonResult = Get-Content $pythonOut -Raw | ConvertFrom-Json
$flask = @($pythonResult.frameworks | Where-Object id -eq 'flask') | Select-Object -First 1
if ($null -eq $flask -or $flask.status -ne 'detected') { throw 'Flask pack was not detected' }
$flaskFact = @($flask.facts | Where-Object { $_.kind -eq 'HTTP_ROUTE' -and $_.path -eq '/health' }) | Select-Object -First 1
if ($null -eq $flaskFact -or $flaskFact.symbol -notmatch 'handlers.*health') { throw 'Flask cross-file route was not resolved' }
$flaskEdge = @($pythonResult.framework_relations | Where-Object { $_.kind -eq 'HANDLES' -and $_.framework -eq 'flask' }) | Select-Object -First 1
if ($null -eq $flaskEdge -or $flaskEdge.from -notmatch 'health' -or $flaskEdge.to -ne $flaskFact.id) { throw 'Flask HANDLES relation was not emitted' }

Write-Host "PASS python/flask: $($flaskFact.method) $($flaskFact.path) -> $($flaskFact.symbol)"

if (-not [string]::IsNullOrWhiteSpace($CppProvidersRoot)) {
    $cppFixture = Join-Path $Root 'tests\fixtures\framework\cpp-crow'
    $cppOut = Join-Path $Root 'build\framework-gate\cpp-crow.json'
    & $Bridge index --root $cppFixture --out $cppOut --packs-root $Root @cppProviderArgs
    if ($LASTEXITCODE -ne 0) { throw 'C++/Crow framework index command failed' }

    $cppResult = Get-Content $cppOut -Raw | ConvertFrom-Json
    $crow = @($cppResult.frameworks | Where-Object id -eq 'crow') | Select-Object -First 1
    if ($null -eq $crow -or $crow.status -ne 'detected') { throw 'Crow pack was not detected' }
    $crowFact = @($crow.facts | Where-Object { $_.kind -eq 'HTTP_ROUTE' -and $_.path -eq '/health' }) | Select-Object -First 1
    if ($null -eq $crowFact -or $crowFact.symbol -notmatch 'handlers\.cpp#health') {
        throw 'Crow cross-file route was not resolved to the implementation symbol'
    }
    $crowEdge = @($cppResult.framework_relations | Where-Object { $_.kind -eq 'HANDLES' -and $_.framework -eq 'crow' }) | Select-Object -First 1
    if ($null -eq $crowEdge -or $crowEdge.from -notmatch 'handlers\.cpp#health' -or $crowEdge.to -ne $crowFact.id) {
        throw 'Crow HANDLES relation was not emitted'
    }
    Write-Host "PASS cpp/crow: $($crowFact.method) $($crowFact.path) -> $($crowFact.symbol)"
}
