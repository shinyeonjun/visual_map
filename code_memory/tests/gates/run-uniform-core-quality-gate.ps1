param(
    [string]$Bridge = (Join-Path $PSScriptRoot '..\..\rust\target\release\code-memory-language.exe'),
    [string]$ProvidersRoot,
    [switch]$AllowMissingProvider
)

$ErrorActionPreference = 'Stop'
if (Get-Variable PSNativeCommandUseErrorActionPreference -ErrorAction SilentlyContinue) {
    $PSNativeCommandUseErrorActionPreference = $false
}

$root = (Resolve-Path (Join-Path $PSScriptRoot '..\fixtures')).Path
$packsRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$bundledProviders = Join-Path $packsRoot 'providers'
if ([string]::IsNullOrWhiteSpace($ProvidersRoot) -and (Test-Path $bundledProviders)) {
    $ProvidersRoot = $bundledProviders
}
$outputRoot = Join-Path $PSScriptRoot '..\..\build\uniform-core-quality'

$cases = @(
    # ponytail: keep the active fixture contract explicit until one canonical language manifest exists;
    # compare it with the bridge and framework catalog so drift fails loudly.
    @{ Id = 'typescript'; Path = 'scip-typescript'; Target = 'add\(\)' },
    @{ Id = 'javascript'; Path = 'scip-javascript'; Target = 'add\(\)' },
    @{ Id = 'python'; Path = 'scip-python'; Target = '#add@' },
    @{ Id = 'java'; Path = 'scip-java'; Target = '#add\(' },
    @{ Id = 'csharp'; Path = 'scip-dotnet'; Target = '#Add\(' },
    @{ Id = 'c'; Path = 'native-lsp-c'; Target = '#add@' },
    @{ Id = 'cpp'; Path = 'native-lsp-c'; Target = '#multiply@' },
    @{ Id = 'go'; Path = 'native-lsp-go'; Target = '#Add@' },
    @{ Id = 'rust'; Path = 'native-lsp-rust'; Target = '#add@' },
    @{ Id = 'php'; Path = 'scip-php'; Target = 'add\(\)' },
    @{ Id = 'ruby'; Path = 'native-lsp-ruby'; Target = '#add@' },
    @{ Id = 'dart'; Path = 'native-lsp-dart'; Target = '#add@' }
)

function Assert-SameLanguageIds {
    param(
        [string]$LeftName,
        [string[]]$Left,
        [string]$RightName,
        [string[]]$Right
    )

    $diff = @(Compare-Object -ReferenceObject @($Left | Sort-Object -Unique) -DifferenceObject @($Right | Sort-Object -Unique))
    if ($diff.Count -gt 0) {
        $details = ($diff | ForEach-Object { "$($_.SideIndicator)$($_.InputObject)" }) -join ', '
        throw "Language catalog drift between ${LeftName} and ${RightName}: $details"
    }
}

if (-not (Test-Path $Bridge)) {
    throw "Bridge not found: $Bridge"
}

$caseIds = @($cases | ForEach-Object Id)
$bridgeList = @(& $Bridge list)
if ($LASTEXITCODE -ne 0) {
    throw 'Bridge language listing failed'
}
$bridgeIds = @($bridgeList | ForEach-Object {
        if ($_ -match '^([^\t]+)\t') { $matches[1] }
    })
if ($bridgeIds.Count -eq 0) {
    throw 'Bridge language listing returned no language ids'
}
$catalogPath = Join-Path $packsRoot 'packs\framework\catalog.json'
$catalog = Get-Content -LiteralPath $catalogPath -Raw | ConvertFrom-Json
$packIds = @($catalog.languages | ForEach-Object id)
Assert-SameLanguageIds -LeftName 'uniform fixtures' -Left $caseIds -RightName 'bridge' -Right $bridgeIds
Assert-SameLanguageIds -LeftName 'bridge' -Left $bridgeIds -RightName 'framework catalog' -Right $packIds

if (Test-Path $outputRoot) {
    Remove-Item -LiteralPath $outputRoot -Recurse -Force
}
New-Item -ItemType Directory -Force -Path $outputRoot | Out-Null

$passed = 0
$skipped = 0
foreach ($case in $cases) {
    $fixture = Join-Path $root $case.Path
    $out = Join-Path $outputRoot "$($case.Id).json"
    $arguments = @('index', '--root', $fixture, '--out', $out, '--packs-root', $packsRoot)
    if (-not [string]::IsNullOrWhiteSpace($ProvidersRoot)) {
        $arguments += @('--providers-root', (Resolve-Path $ProvidersRoot).Path)
    }

    & $Bridge @arguments
    if ($LASTEXITCODE -ne 0) {
        if ($AllowMissingProvider) {
            Write-Host "SKIP $($case.Id): bridge failed"
            $skipped++
            continue
        }
        throw "$($case.Id): bridge failed"
    }
    if (-not (Test-Path $out)) {
        throw "$($case.Id): index output was not written"
    }

    $result = Get-Content $out -Raw | ConvertFrom-Json
    $language = @($result.languages | Where-Object id -eq $case.Id) | Select-Object -First 1
    if ($null -eq $language -or $language.status -ne 'indexed') {
        $status = if ($null -eq $language) { 'missing-language-output' } else { $language.status }
        if ($AllowMissingProvider -and $status -in @('missing-tool', 'indexer-failed', 'provider-failed')) {
            Write-Host "SKIP $($case.Id): $status"
            $skipped++
            continue
        }
        throw "$($case.Id): status is not indexed ($status)"
    }

    $documents = @($result.documents)
    $relations = @($result.relations)
    $calls = @($relations | Where-Object kind -eq 'CALLS')
    if ($documents.Count -eq 0) { throw "$($case.Id): no semantic documents" }
    if ($calls.Count -eq 0) { throw "$($case.Id): no CALLS relation" }
    if (@($result.diagnostics | Where-Object level -eq 'error').Count -gt 0) {
        throw "$($case.Id): error-level provider diagnostic was emitted"
    }

    $duplicateDocuments = @($documents | Group-Object path | Where-Object Count -gt 1)
    if ($duplicateDocuments.Count -gt 0) {
        throw "$($case.Id): duplicate document path $($duplicateDocuments[0].Name)"
    }

    $logicalRelations = @{}
    foreach ($relation in $relations) {
        if ([string]::IsNullOrWhiteSpace($relation.from) -or
            [string]::IsNullOrWhiteSpace($relation.to)) {
            throw "$($case.Id): relation lacks common endpoint evidence"
        }
        if ($relation.kind -eq 'CALLS' -and
            ([string]::IsNullOrWhiteSpace($relation.path) -or @($relation.range).Count -lt 3)) {
            throw "$($case.Id): CALLS relation lacks source-range evidence"
        }
        $key = "$($relation.kind)|$($relation.from)|$($relation.to)|$($relation.path)|$(@($relation.range) -join ',')"
        if ($logicalRelations.ContainsKey($key)) {
            throw "$($case.Id): duplicate logical relation $key"
        }
        $logicalRelations[$key] = $true
    }

    if (-not ($calls | Where-Object {
        $_.to -match $case.Target -and
        ((Test-Path (Join-Path $fixture $_.path)) -or (Test-Path $_.path))
    })) {
        throw "$($case.Id): expected resolved target was not found"
    }
    Write-Host "PASS $($case.Id): documents=$($documents.Count) relations=$($relations.Count) calls=$($calls.Count)"
    $passed++
}

Write-Host "uniform core quality gate: passed=$passed skipped=$skipped total=$($cases.Count)"
if (-not $AllowMissingProvider -and $passed -ne $cases.Count) {
    exit 1
}
