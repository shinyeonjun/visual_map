param(
    [string]$Root = (Join-Path $PSScriptRoot '..\..')
)

$ErrorActionPreference = 'Stop'
$frameworkRoot = Join-Path $Root 'packs\framework'
$catalog = Get-Content (Join-Path $frameworkRoot 'catalog.json') -Raw | ConvertFrom-Json
if ($catalog.schema -ne 'code-memory.framework-pack-catalog.v1') {
    throw "invalid framework catalog schema"
}
$adapterCatalog = Get-Content (Join-Path $frameworkRoot 'adapters.json') -Raw | ConvertFrom-Json
if ($adapterCatalog.schema -ne 'code-memory.framework-adapter-catalog.v1') {
    throw "invalid framework adapter catalog schema"
}

$expected = @{
    typescript = @('react','nextjs','angular','vue','nuxt','sveltekit','express','fastify','nestjs','koa')
    javascript = @('react','nextjs','angular','vue','nuxt','sveltekit','express','fastify','nestjs','koa','tauri')
    python = @('django','flask','fastapi','starlette','sanic')
    java = @('spring','spring-boot','spring-mvc','spring-webflux','jakarta-ee','quarkus','micronaut','play')
    csharp = @('aspnet-core','aspnet-mvc','aspnet-web-api','minimal-api','blazor','dotnet-maui')
    c = @('gtk-glib','qt','libuv','libevent')
    cpp = @('qt','mfc','boost-asio','poco','unreal-engine','drogon','crow','grpc')
    go = @('net-http','gin','echo','fiber','chi','beego','grpc')
    rust = @('axum','actix-web','rocket','warp','poem','tokio','tonic','tauri')
    php = @('laravel','symfony','codeigniter','laminas','slim','cakephp','api-platform')
    ruby = @('rails','sinatra','hanami','rack','grape','roda')
    dart = @('flutter','shelf','serverpod','dart-frog')
}

$seen = @{}
$supportedOutputs = @('HTTP_ROUTE','HANDLES','COMPONENT','RENDERS','EVENT_HANDLER','SERVICE','MIDDLEWARE','DEPENDENCY','ASYNC_CALLS','RPC_ENDPOINT','SERVER_ACTION','SCHEMA','SCHEDULED_JOB')
$supportedAdapters = @('registration-routing','annotation-routing','filesystem-routing','component-events','rpc-service','async-events','event-or-declaration')
$metadataNames = @{
    typescript = @('package.json'); javascript = @('package.json')
    python = @('pyproject.toml'); java = @('pom.xml'); csharp = @('*.csproj')
    c = @('CMakeLists.txt'); cpp = @('CMakeLists.txt'); go = @('go.mod')
    rust = @('Cargo.toml'); php = @('composer.json'); ruby = @('Gemfile'); dart = @('pubspec.yaml')
}
$total = 0
foreach ($language in $catalog.languages) {
    if (-not $expected.ContainsKey($language.id)) { throw "unexpected language: $($language.id)" }
    $path = Join-Path $frameworkRoot $language.file
    $document = Get-Content $path -Raw | ConvertFrom-Json
    if ($document.schema -ne 'code-memory.framework-pack-catalog.v1' -or $document.language -ne $language.id) {
        throw "invalid manifest: $path"
    }
    $ids = @($document.packs | ForEach-Object { $_.id })
    if ((@($ids | Sort-Object) -join ',') -cne (@($expected[$language.id] | Sort-Object) -join ',')) {
        throw "pack list mismatch: $($language.id)"
    }
    foreach ($packRef in $document.packs) {
        $packPath = Join-Path (Split-Path $path) $packRef.path
        $pack = Get-Content $packPath -Raw | ConvertFrom-Json
        if ($pack.schema -ne 'code-memory.framework-pack.v1' -or
            $pack.language -ne $language.id -or $pack.id -ne $packRef.id) {
            throw "invalid pack reference: $packPath"
        }
        $fixturePath = Join-Path (Split-Path $packPath) 'fixture.json'
        $fixture = Get-Content $fixturePath -Raw | ConvertFrom-Json
        if ($fixture.schema -ne 'code-memory.framework-fixture.v1' -or
            $fixture.language -ne $pack.language -or $fixture.framework -ne $pack.id) {
            throw "invalid fixture: $fixturePath"
        }
        if (@($fixture.files).Count -eq 0 -or @($fixture.expected.facts).Count -eq 0) {
            throw "fixture has no files or expected facts: $fixturePath"
        }
        foreach ($fixtureFile in @($fixture.files)) {
            if ([string]::IsNullOrWhiteSpace($fixtureFile.path) -or
                [string]::IsNullOrWhiteSpace($fixtureFile.source)) {
                throw "fixture file is empty: $fixturePath"
            }
        }
        $metadata = @($fixture.files | Where-Object {
            $name = [System.IO.Path]::GetFileName($_.path)
            @($metadataNames[$language.id]) | Where-Object { $_ -eq $name -or ($_.StartsWith('*.') -and $name.EndsWith($_.Substring(1))) }
        })
        if ($metadata.Count -eq 0) { throw "language metadata file is missing: $fixturePath" }
        if ($language.id -notin @('typescript','javascript') -and
            @($fixture.files | Where-Object { $_.path -eq 'package.json' }).Count -gt 0) {
            throw "non-JS fixture has package.json: $fixturePath"
        }
        if ((@($fixture.expected.facts) -join ',') -cne (@($pack.rule_sets) -join ',')) {
            throw "fixture facts do not match rule_sets: $fixturePath"
        }
        if ([string]::IsNullOrWhiteSpace($pack.name) -or [string]::IsNullOrWhiteSpace($pack.kind)) { throw "missing metadata: $($language.id)/$($pack.id)" }
        if (@($pack.signals).Count -eq 0 -or @($pack.outputs).Count -eq 0 -or @($pack.rule_sets).Count -eq 0) { throw "missing rules: $($language.id)/$($pack.id)" }
        foreach ($output in @($pack.outputs)) {
            if ($supportedOutputs -notcontains $output) { throw "unsupported output ${output}: $($language.id)/$($pack.id)" }
            if ($output -ne 'HANDLES' -and @($pack.rule_sets) -notcontains $output) {
                throw "output has no rule_set ${output}: $($language.id)/$($pack.id)"
            }
            if ($output -eq 'HANDLES' -and
                @($pack.rule_sets) -notcontains 'HTTP_ROUTE' -and
                @($pack.rule_sets) -notcontains 'RPC_ENDPOINT') {
                throw "HANDLES has no route or RPC rule: $($language.id)/$($pack.id)"
            }
        }
        $qualified = "$($language.id)/$($pack.id)"
        if ($seen.ContainsKey($qualified)) { throw "duplicate pack: $qualified" }
        $adapterProperty = $adapterCatalog.adapters.psobject.Properties[$qualified]
        $adapter = if ($null -eq $adapterProperty) { $null } else { $adapterProperty.Value }
        if ([string]::IsNullOrWhiteSpace($adapter)) { throw "missing adapter: $qualified" }
        if ($supportedAdapters -notcontains $adapter) { throw "unsupported adapter ${adapter}: $qualified" }
        $seen[$qualified] = $true
        $total++
    }
    Write-Host "PASS $($language.id): $(@($document.packs).Count) packs"
}

if ($total -ne 84) { throw "expected 84 packs, found $total" }
if (@($adapterCatalog.adapters.psobject.Properties).Count -ne 84) { throw "expected 84 adapters" }
Write-Host "framework pack gate: passed=$total total=84"

