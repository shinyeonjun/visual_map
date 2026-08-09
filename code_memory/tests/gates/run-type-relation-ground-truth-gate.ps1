param(
    [string]$Bridge = (Join-Path $PSScriptRoot '..\..\rust\target\release\code-memory-language.exe'),
    [string]$ProvidersRoot,
    [string]$Truth = (Join-Path $PSScriptRoot '..\ground_truth\type-relations.v1.json'),
    [string]$FixturesRoot = (Join-Path $PSScriptRoot '..\fixtures\type-relations'),
    [string]$OutputRoot = (Join-Path $PSScriptRoot '..\..\build\type-relation-ground-truth'),
    [ValidateRange(2, 5)]
    [int]$Runs = 2
)

. (Join-Path $PSScriptRoot 'lib\language-ir-stream-authority.ps1')

$ErrorActionPreference = 'Stop'
if (Get-Variable PSNativeCommandUseErrorActionPreference -ErrorAction SilentlyContinue) {
    $PSNativeCommandUseErrorActionPreference = $false
}

function Get-ProjectFileState {
    param([string]$Root)

    $state = [ordered]@{}
    Get-ChildItem -LiteralPath $Root -Recurse -File | Sort-Object FullName | ForEach-Object {
        $relative = $_.FullName.Substring($Root.Length + 1).Replace('\', '/')
        $state[$relative] = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    }
    return $state
}

function Assert-SameFileState {
    param(
        [string]$ProjectId,
        [System.Collections.IDictionary]$Before,
        [System.Collections.IDictionary]$After
    )

    $beforeJson = $Before | ConvertTo-Json -Compress
    $afterJson = $After | ConvertTo-Json -Compress
    if ($beforeJson -cne $afterJson) {
        throw "${ProjectId}: provider analysis mutated the reviewed fixture"
    }
}

function Test-RelationMatch {
    param(
        [pscustomobject]$Expected,
        [pscustomobject]$Actual
    )

    foreach ($property in $Expected.PSObject.Properties) {
        $actualName = switch ($property.Name) {
            'line' { 'startLine' }
            'column' { 'startUtf8Column' }
            default { $property.Name }
        }
        $actualProperty = $Actual.PSObject.Properties[$actualName]
        if ($null -eq $actualProperty -or [string]$actualProperty.Value -cne [string]$property.Value) {
            return $false
        }
    }
    return $true
}

function Invoke-TypeRelationAnalysis {
    param(
        [pscustomobject]$Project,
        [string]$FixtureRoot,
        [string]$RunRoot,
        [string]$PacksRoot
    )

    New-Item -ItemType Directory -Force -Path $RunRoot | Out-Null
    $outputPath = Join-Path $RunRoot "$($Project.id).json"
    $arguments = @('index', '--root', $FixtureRoot, '--out', $outputPath, '--packs-root', $PacksRoot)
    if (-not [string]::IsNullOrWhiteSpace($ProvidersRoot)) {
        $arguments += @('--providers-root', (Resolve-Path $ProvidersRoot).Path)
    }
    $stopwatch = [Diagnostics.Stopwatch]::StartNew()
    $previousErrorActionPreference = $ErrorActionPreference
    try {
        $ErrorActionPreference = 'Continue'
        $bridgeOutput = @(& $Bridge @arguments 2>&1)
        $exitCode = $LASTEXITCODE
    }
    finally {
        $ErrorActionPreference = $previousErrorActionPreference
    }
    $stopwatch.Stop()
    if ($exitCode -ne 0) {
        $detail = ($bridgeOutput | Select-Object -Last 80 | ForEach-Object { $_.ToString() }) -join "`n"
        throw "$($Project.id): provider analysis failed with exit $exitCode`n$detail"
    }
    if (-not (Test-Path -LiteralPath $outputPath -PathType Leaf)) {
        throw "$($Project.id): analysis did not write $outputPath"
    }
    $lines = @($bridgeOutput | ForEach-Object { $_.ToString() })
    $receiptLine = $lines |
        Where-Object { $_.StartsWith('@codebase-workspace-language-ir ', [StringComparison]::Ordinal) } |
        Select-Object -First 1
    if ([string]::IsNullOrWhiteSpace($receiptLine)) {
        throw "$($Project.id): analysis emitted no Language IR receipt"
    }
    $receipt = $receiptLine.Substring('@codebase-workspace-language-ir '.Length) | ConvertFrom-Json
    $authority = Get-LanguageIrStreamAuthority -BridgeOutput $lines -Receipt $receipt -Context $Project.id
    return [pscustomobject]@{
        elapsedMs = $stopwatch.ElapsedMilliseconds
        receipt = $receipt
        authority = $authority
    }
}

function Assert-LanguageResult {
    param(
        [string]$ProjectId,
        [pscustomobject]$Language,
        [pscustomobject]$Receipt
    )

    $summary = @($Receipt.typeRelationLanguageSummaries | Where-Object language -eq $Language.language)
    if ($summary.Count -ne 1) {
        throw "$ProjectId/$($Language.language): expected exactly one type-relation summary, got $($summary.Count)"
    }
    $summary = $summary[0]
    $expected = $Language.expected
    $countMap = [ordered]@{
        relationSetDigest = 'relationSetDigest'
        total = 'relationCount'
        extends = 'extendsCount'
        implements = 'implementsCount'
        mixesIn = 'mixesInCount'
        overrides = 'overridesCount'
        usesType = 'usesTypeCount'
        explicitHierarchySites = 'explicitHierarchySiteCount'
    }
    foreach ($entry in $countMap.GetEnumerator()) {
        $expectedValue = $expected.PSObject.Properties[$entry.Key].Value
        $actualValue = $summary.PSObject.Properties[$entry.Value].Value
        if ([string]$expectedValue -cne [string]$actualValue) {
            throw "$ProjectId/$($Language.language): $($entry.Value) expected $expectedValue, got $actualValue"
        }
    }
    if ([int64]$summary.matchedExplicitHierarchySiteCount -ne [int64]$expected.explicitHierarchySites -or
        [int64]$summary.unmatchedExplicitHierarchySiteCount -ne 0 -or
        [int64]$summary.inventoryFailedFileCount -ne 0) {
        throw "$ProjectId/$($Language.language): hierarchy inventory or provider matching is incomplete"
    }

    $relations = @($Receipt.typeRelationAuditSample | Where-Object language -eq $Language.language)
    if ($relations.Count -ne [int]$expected.total) {
        throw "$ProjectId/$($Language.language): exhaustive audit expected $($expected.total) relations, got $($relations.Count)"
    }
    $allowedKinds = @('extends', 'implements', 'mixes_in', 'overrides', 'uses_type')
    $unexpectedKind = $relations | Where-Object { $allowedKinds -notcontains $_.kind } | Select-Object -First 1
    if ($null -ne $unexpectedKind) {
        throw "$ProjectId/$($Language.language): unsupported canonical relation kind $($unexpectedKind.kind)"
    }
    foreach ($required in @($Language.requiredRelations)) {
        if (-not ($relations | Where-Object { Test-RelationMatch $required $_ } | Select-Object -First 1)) {
            throw "$ProjectId/$($Language.language): required reviewed relation is missing: $($required | ConvertTo-Json -Compress)"
        }
    }
    foreach ($forbidden in @($Language.forbiddenRelations)) {
        if ($relations | Where-Object { Test-RelationMatch $forbidden $_ } | Select-Object -First 1) {
            throw "$ProjectId/$($Language.language): forbidden relation was emitted: $($forbidden | ConvertTo-Json -Compress)"
        }
    }
}

if (-not (Test-Path -LiteralPath $Bridge -PathType Leaf)) {
    throw "Bridge not found: $Bridge"
}
if (-not (Test-Path -LiteralPath $Truth -PathType Leaf)) {
    throw "Type-relation ground truth not found: $Truth"
}

$contract = Get-Content -Raw -LiteralPath $Truth | ConvertFrom-Json
if ($contract.schema -ne 'codebase-workspace.type-relation-ground-truth.v1') {
    throw "Unsupported type-relation ground-truth schema: $($contract.schema)"
}
$languageNames = @($contract.projects.languages | ForEach-Object { [string]$_.language } | Sort-Object -Unique)
$requiredLanguages = @('c', 'cpp', 'csharp', 'dart', 'go', 'java', 'javascript', 'python', 'rust', 'typescript')
if ($languageNames.Count -ne 10 -or (Compare-Object $requiredLanguages $languageNames)) {
    throw "Type-relation ground truth must cover the exact ten-language contract"
}

$fixturesRoot = (Resolve-Path $FixturesRoot).Path
$packsRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$bundledProviders = Join-Path $packsRoot 'providers'
if ([string]::IsNullOrWhiteSpace($ProvidersRoot) -and (Test-Path -LiteralPath $bundledProviders)) {
    $ProvidersRoot = $bundledProviders
}
New-Item -ItemType Directory -Force -Path $OutputRoot | Out-Null

$initialStates = @{}
foreach ($project in $contract.projects) {
    $fixtureRoot = (Resolve-Path (Join-Path $fixturesRoot $project.fixture)).Path
    $state = Get-ProjectFileState $fixtureRoot
    $initialStates[$project.id] = $state
    if ($state.Count -ne @($project.sourceFiles).Count) {
        throw "$($project.id): reviewed fixture file count changed; expected $(@($project.sourceFiles).Count), got $($state.Count)"
    }
    foreach ($source in $project.sourceFiles) {
        if (-not $state.Contains($source.path)) {
            throw "$($project.id): reviewed source is missing: $($source.path)"
        }
        if ([string]$state[$source.path] -cne [string]$source.sha256) {
            throw "$($project.id): source hash changed for $($source.path)"
        }
    }
}

$runResults = @()
for ($run = 1; $run -le $Runs; $run++) {
    foreach ($project in $contract.projects) {
        $fixtureRoot = (Resolve-Path (Join-Path $fixturesRoot $project.fixture)).Path
        $runRoot = Join-Path $OutputRoot "run-$run"
        $result = Invoke-TypeRelationAnalysis $project $fixtureRoot $runRoot $packsRoot
        if ($result.receipt.schema -ne 'codebase-workspace.language-ir-migration-receipt.v6') {
            throw "$($project.id): unsupported receipt schema"
        }
        if ($result.receipt.detailsTruncated -or [int64]$result.receipt.issueCount -ne 0) {
            throw "$($project.id): type relation evidence was truncated or analysis emitted issues"
        }
        foreach ($language in $project.languages) {
            Assert-LanguageResult $project.id $language $result.receipt
        }
        Assert-SameFileState $project.id $initialStates[$project.id] (Get-ProjectFileState $fixtureRoot)
        $runResults += [pscustomobject]@{
            run = $run
            project = [string]$project.id
            elapsedMs = $result.elapsedMs
            sourceManifestDigest = [string]$result.receipt.sourceManifestDigest
            analysisPlanDigest = [string]$result.receipt.analysisPlanDigest
            streamSetDigest = [string]$result.receipt.streamSetDigest
            artifactContentDigest = [string]$result.authority.contentDigest
            semanticPayloadSetDigest = [string]$result.receipt.semanticPayloadSetDigest
            typeRelationDigests = @($result.receipt.typeRelationLanguageSummaries |
                Sort-Object language |
                ForEach-Object { "$($_.language):$($_.relationSetDigest)" }) -join '|'
        }
    }
}

foreach ($project in $contract.projects) {
    $results = @($runResults | Where-Object project -eq $project.id | Sort-Object run)
    $baseline = $results[0]
    foreach ($candidate in $results | Select-Object -Skip 1) {
        foreach ($field in @('sourceManifestDigest', 'analysisPlanDigest', 'streamSetDigest', 'artifactContentDigest', 'semanticPayloadSetDigest', 'typeRelationDigests')) {
            if ($candidate.$field -cne $baseline.$field) {
                throw "$($project.id): non-deterministic $field between run 1 and run $($candidate.run)"
            }
        }
    }
}

$languages = @($contract.projects.languages)
$expectedRelationCount = [int64](($languages | ForEach-Object { [int64]$_.expected.total } | Measure-Object -Sum).Sum)
$requiredRelationCount = [int64](($languages | ForEach-Object { @($_.requiredRelations).Count } | Measure-Object -Sum).Sum)
$forbiddenRelationCount = [int64](($languages | ForEach-Object { @($_.forbiddenRelations).Count } | Measure-Object -Sum).Sum)
$report = [ordered]@{
    schema = 'codebase-workspace.type-relation-quality-report.v1'
    reviewedProjectCount = @($contract.projects).Count
    reviewedLanguageCount = $languageNames.Count
    reviewedRelationCount = $expectedRelationCount
    requiredRepresentativeCount = $requiredRelationCount
    reviewedNegativeCount = $forbiddenRelationCount
    truePositive = $expectedRelationCount
    falsePositive = 0
    falseNegative = 0
    precision = 1.0
    recall = 1.0
    f1 = 1.0
    exactEvidence = 1.0
    deterministicRuns = $Runs
    sourceMutationCount = 0
    kindCounts = [ordered]@{
        extends = [int64](($languages | ForEach-Object { [int64]$_.expected.extends } | Measure-Object -Sum).Sum)
        implements = [int64](($languages | ForEach-Object { [int64]$_.expected.implements } | Measure-Object -Sum).Sum)
        mixesIn = [int64](($languages | ForEach-Object { [int64]$_.expected.mixesIn } | Measure-Object -Sum).Sum)
        overrides = [int64](($languages | ForEach-Object { [int64]$_.expected.overrides } | Measure-Object -Sum).Sum)
        usesType = [int64](($languages | ForEach-Object { [int64]$_.expected.usesType } | Measure-Object -Sum).Sum)
    }
    runs = $runResults
}
$reportPath = Join-Path $OutputRoot 'type-relation-quality-report.json'
$report | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $reportPath -Encoding utf8

Write-Host "Type relation ground-truth gate passed: $($languageNames.Count)/10 languages, $expectedRelationCount/$expectedRelationCount exact relations, $forbiddenRelationCount reviewed negatives, $Runs deterministic runs."
Write-Host "Report: $reportPath"
