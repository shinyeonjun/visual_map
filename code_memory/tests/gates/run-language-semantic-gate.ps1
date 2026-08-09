param(
    [string]$Bridge = (Join-Path $PSScriptRoot '..\..\rust\target\release\code-memory-language.exe'),
    [string]$ProvidersRoot,
    [switch]$AllowMissingProvider
)

. (Join-Path $PSScriptRoot 'lib\language-ir-stream-authority.ps1')

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
    $bridgeOutput = @(& $Bridge @arguments 2>&1)
    $bridgeExitCode = $LASTEXITCODE
    $bridgeOutput | ForEach-Object { Write-Host $_ }
    if ($bridgeExitCode -ne 0) {
        if ($AllowMissingProvider) {
            Write-Host "SKIP $($case.Id): bridge failed"
            $skipped++
            continue
        }
        throw "$($case.Id): bridge failed"
    }

    $receiptPrefix = '@codebase-workspace-language-ir '
    $receiptLine = @($bridgeOutput | ForEach-Object { $_.ToString() } | Where-Object {
            $_.StartsWith($receiptPrefix, [StringComparison]::Ordinal)
        }) | Select-Object -Last 1
    if ([string]::IsNullOrWhiteSpace($receiptLine)) {
        throw "$($case.Id): Language IR receipt is missing"
    }
    $receipt = $receiptLine.Substring($receiptPrefix.Length) | ConvertFrom-Json
    if ($receipt.schema -ne 'codebase-workspace.language-ir-migration-receipt.v6') {
        throw "$($case.Id): unsupported Language IR receipt schema $($receipt.schema)"
    }
    $null = Get-LanguageIrStreamAuthority -BridgeOutput $bridgeOutput -Receipt $receipt -Context $case.Id

    $contextPrefix = '@codebase-workspace-provider-execution-context '
    $contextLine = @($bridgeOutput | ForEach-Object { $_.ToString() } | Where-Object {
            $_.StartsWith($contextPrefix)
        }) | Select-Object -Last 1
    if ([string]::IsNullOrWhiteSpace($contextLine)) {
        throw "$($case.Id): provider execution context receipt is missing"
    }
    $context = $contextLine.Substring($contextPrefix.Length) | ConvertFrom-Json
    if ($context.schema -ne 'codebase-workspace.provider-execution-context-reconciliation.v3') {
        throw "$($case.Id): unsupported provider execution context schema $($context.schema)"
    }
    if ($context.executionCount -lt 1 -or
        $context.exactExecutionCount -ne $context.executionCount -or
        $context.partialExecutionCount -ne 0 -or
        $context.notExecutedCount -ne 0) {
        throw "$($case.Id): reviewed fixture did not use an exact provider execution context"
    }
    if ($context.contextSetDigest -notmatch '^[0-9a-f]{64}$') {
        throw "$($case.Id): provider execution context digest is invalid"
    }
    if ($receipt.executionContextSetDigest -ne $context.contextSetDigest) {
        throw "$($case.Id): Language IR and execution-context receipts disagree"
    }
    if ($context.detailsTruncated -or @($context.executionSample).Count -ne [int]$context.executionCount) {
        throw "$($case.Id): reviewed fixture execution-context evidence is incomplete"
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
    # One physical header can legitimately have distinct C and C++ semantic
    # documents. Provider document identity is (language, path), not path.
    $duplicateDocuments = @($result.documents |
        Group-Object { "$($_.language)`t$($_.path)" } |
        Where-Object Count -gt 1)
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
