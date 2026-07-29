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
$outputRoot = Join-Path $PSScriptRoot '..\..\build\semantic-gate'
$cases = @(
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

if (-not (Test-Path $Bridge)) {
    throw "Bridge not found: $Bridge"
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

    $result = Get-Content $out -Raw | ConvertFrom-Json
    $language = @($result.languages | Where-Object id -eq $case.Id) | Select-Object -First 1
    if ($null -eq $language -or $language.status -ne 'indexed') {
        if ($AllowMissingProvider -and $language.status -in @('missing-tool', 'indexer-failed')) {
            Write-Host "SKIP $($case.Id): $($language.status)"
            $skipped++
            continue
        }
        throw "$($case.Id): status is not indexed"
    }
    if (@($result.documents).Count -eq 0 -or @($result.relations).Count -eq 0) {
        throw "$($case.Id): no semantic documents or relations"
    }
    if (-not (@($result.relations | Where-Object kind -eq 'CALLS').Count -gt 0)) {
        throw "$($case.Id): no CALLS relation"
    }
    $calls = @($result.relations | Where-Object kind -eq 'CALLS')
    if (-not ($calls | Where-Object {
                $_.to -match $case.Target -and
                @($_.range).Count -ge 3 -and
                ((Test-Path (Join-Path $fixture $_.path)) -or (Test-Path $_.path))
            })) {
        throw "$($case.Id): expected resolved target/range was not found"
    }
    if ($case.Id -eq 'cpp') {
        if (-not (@($result.relations | Where-Object {
                    $_.kind -eq 'IMPLEMENTATION' -and
                    $_.from -match 'BoxValue' -and
                    $_.to -match 'Base'
                }).Count -gt 0)) {
            throw 'cpp: expected class inheritance relation was not found'
        }
        if (-not (@($result.documents | Where-Object {
                    $_.language -eq 'cpp' -and $_.path -eq 'types.h'
                }).Count -eq 1)) {
            throw 'cpp: expected one indexed project header document'
        }
    }
    $duplicateDocuments = @($result.documents | Group-Object path | Where-Object Count -gt 1)
    if ($duplicateDocuments.Count -gt 0) {
        throw "$($case.Id): duplicate semantic documents were emitted for $($duplicateDocuments[0].Name)"
    }
    if ($case.Id -in @('typescript', 'javascript')) {
        $imports = @($result.file_relations | Where-Object kind -eq 'IMPORTS')
        if ($imports.Count -eq 0) {
            throw "$($case.Id): project model produced no resolved file import relation"
        }
        if (-not ($imports | Where-Object { $_.properties.resolution -eq 'internal' })) {
            throw "$($case.Id): no internal TypeScript/JavaScript module import relation"
        }
    }
    Write-Host "PASS $($case.Id)"
    $passed++
}

Write-Host "semantic gate: passed=$passed skipped=$skipped total=$($cases.Count)"
if (-not $AllowMissingProvider -and $passed -ne $cases.Count) {
    exit 1
}
