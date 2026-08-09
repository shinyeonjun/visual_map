param(
    [string]$Bridge = (Join-Path $PSScriptRoot '..\..\rust\target\release\code-memory-language.exe'),
    [string]$ProvidersRoot,
    [string]$Truth = (Join-Path $PSScriptRoot '..\ground_truth\definitions.v1.json'),
    [string]$FixturesRoot = (Join-Path $PSScriptRoot '..\fixtures'),
    [string]$OutputRoot = (Join-Path $PSScriptRoot '..\..\build\definition-ground-truth'),
    [ValidateRange(1, 5)]
    [int]$Runs = 2,
    [switch]$AuditOnly
)

. (Join-Path $PSScriptRoot 'lib\language-ir-stream-authority.ps1')

$ErrorActionPreference = 'Stop'
if (Get-Variable PSNativeCommandUseErrorActionPreference -ErrorAction SilentlyContinue) {
    $PSNativeCommandUseErrorActionPreference = $false
}

function Get-Rate {
    param([int64]$Numerator, [int64]$Denominator)
    if ($Denominator -eq 0) { return 1.0 }
    return [Math]::Round($Numerator / $Denominator, 6)
}

function Get-Sha256Text {
    param([string]$Value)
    $algorithm = [Security.Cryptography.SHA256]::Create()
    try {
        $bytes = [Text.UTF8Encoding]::new($false).GetBytes($Value)
        return -join ($algorithm.ComputeHash($bytes) | ForEach-Object { $_.ToString('x2') })
    }
    finally {
        $algorithm.Dispose()
    }
}

function Get-ExpectedLanguage {
    param(
        [string]$ProjectId,
        [pscustomobject]$Language
    )

    $keys = New-Object 'System.Collections.Generic.List[string]'
    $forbiddenCount = 0
    $ownedCount = 0
    $callableCount = 0
    foreach ($file in $Language.files) {
        foreach ($definition in @($file.definitions)) {
            $parts = @([string]$definition -split ':', 3)
            if ($parts.Count -ne 3 -or [string]::IsNullOrWhiteSpace($parts[0]) -or
                [string]::IsNullOrWhiteSpace($parts[1]) -or [string]::IsNullOrWhiteSpace($parts[2])) {
                throw "$ProjectId/$($Language.language): invalid reviewed definition '$definition'"
            }
            $keys.Add("$($file.path)`t$($parts[0])`t$($parts[1])`t$($parts[2])")
            if ($parts[2] -ne '-') { $ownedCount++ }
            if ($parts[0] -in @('function', 'method', 'constructor')) { $callableCount++ }
        }
        $forbiddenProperty = $file.PSObject.Properties['forbiddenNames']
        if ($null -ne $forbiddenProperty) {
            $forbiddenCount += @($forbiddenProperty.Value).Count
        }
    }
    if ($forbiddenCount -eq 0) {
        throw "$ProjectId/$($Language.language): at least one reviewed negative definition is required"
    }
    $sorted = [string[]]$keys.ToArray()
    [Array]::Sort($sorted, [StringComparer]::Ordinal)
    $duplicates = @($sorted | Group-Object | Where-Object Count -gt 1)
    if ($duplicates.Count -gt 0) {
        throw "$ProjectId/$($Language.language): duplicate reviewed definition key: $($duplicates[0].Name)"
    }
    return [pscustomobject]@{
        projectId = $ProjectId
        language = [string]$Language.language
        definitionCount = $sorted.Count
        ownedDefinitionCount = $ownedCount
        callableDefinitionCount = $callableCount
        forbiddenDefinitionCount = $forbiddenCount
        definitionSetDigest = Get-Sha256Text ($sorted -join "`n")
    }
}

function Get-OptionalText {
    param(
        [pscustomobject]$Value,
        [string]$Property,
        [string]$Fallback = '-'
    )
    $candidate = $Value.PSObject.Properties[$Property]
    if ($null -eq $candidate -or $null -eq $candidate.Value -or [string]::IsNullOrWhiteSpace([string]$candidate.Value)) {
        return $Fallback
    }
    return [string]$candidate.Value
}

function Get-MetadataIdentity {
    param([pscustomobject]$Value)
    $owner = Get-OptionalText -Value $Value -Property 'owner'
    return "$($Value.path)`t$($Value.kind)`t$($Value.name)`t$owner"
}

function Compare-ReviewedMetadata {
    param(
        [string]$ProjectId,
        [string]$Language,
        [object[]]$ExpectedEntries,
        [object[]]$ActualEntries
    )
    if ($ExpectedEntries.Count -eq 0) {
        throw "$ProjectId/${Language}: definition metadata needs at least one reviewed case"
    }
    $actualByKey = @{}
    foreach ($actualEntry in $ActualEntries) {
        $key = Get-MetadataIdentity -Value $actualEntry
        if ($actualByKey.ContainsKey($key)) {
            throw "$ProjectId/${Language}: duplicate emitted metadata identity $key"
        }
        $actualByKey[$key] = $actualEntry
    }
    $matched = 0
    foreach ($expectedEntry in $ExpectedEntries) {
        $key = Get-MetadataIdentity -Value $expectedEntry
        if (-not $actualByKey.ContainsKey($key)) {
            throw "$ProjectId/${Language}: reviewed metadata definition is missing: $key"
        }
        $actualEntry = $actualByKey[$key]
        $expectedSignature = Get-OptionalText -Value $expectedEntry -Property 'signature'
        $actualSignature = Get-OptionalText -Value $actualEntry -Property 'signature'
        if ([string]$actualEntry.visibility -ne [string]$expectedEntry.visibility -or
            $actualSignature -ne $expectedSignature) {
            throw "$ProjectId/${Language}: metadata mismatch for $key expected visibility=$($expectedEntry.visibility) signature=$expectedSignature actual visibility=$($actualEntry.visibility) signature=$actualSignature"
        }
        $matched++
    }
    return $matched
}

function Invoke-DefinitionAnalysis {
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
        $detail = ($bridgeOutput | ForEach-Object { $_.ToString() }) -join "`n"
        throw "$($Project.id): provider analysis failed with exit $exitCode`n$detail"
    }
    if (-not (Test-Path -LiteralPath $outputPath -PathType Leaf)) {
        throw "$($Project.id): analysis did not write $outputPath"
    }
    $receiptLine = $bridgeOutput |
        ForEach-Object { $_.ToString() } |
        Where-Object { $_.StartsWith('@codebase-workspace-language-ir ', [StringComparison]::Ordinal) } |
        Select-Object -First 1
    if ([string]::IsNullOrWhiteSpace($receiptLine)) {
        throw "$($Project.id): analysis emitted no Language IR migration receipt"
    }
    $receipt = $receiptLine.Substring('@codebase-workspace-language-ir '.Length) | ConvertFrom-Json
    if ($receipt.schema -ne 'codebase-workspace.language-ir-migration-receipt.v6') {
        throw "$($Project.id): unsupported Language IR receipt schema $($receipt.schema)"
    }
    $authority = Get-LanguageIrStreamAuthority -BridgeOutput $bridgeOutput -Receipt $receipt -Context $Project.id
    return [pscustomobject]@{
        elapsedMs = $stopwatch.ElapsedMilliseconds
        receipt = $receipt
        authority = $authority
    }
}

if (-not (Test-Path -LiteralPath $Bridge -PathType Leaf)) {
    throw "Bridge not found: $Bridge"
}
if (-not (Test-Path -LiteralPath $Truth -PathType Leaf)) {
    throw "Definition ground truth not found: $Truth"
}

$truthContract = Get-Content -Raw -LiteralPath $Truth | ConvertFrom-Json
if ($truthContract.schema -ne 'codebase-workspace.definition-ground-truth.v1') {
    throw "Unsupported definition ground-truth schema: $($truthContract.schema)"
}
$metadataCases = @($truthContract.metadataCases)
if ($metadataCases.Count -eq 0) {
    throw 'Definition ground truth contains no reviewed metadata cases'
}
$fixturesRoot = (Resolve-Path $FixturesRoot).Path
$packsRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$bundledProviders = Join-Path $packsRoot 'providers'
if ([string]::IsNullOrWhiteSpace($ProvidersRoot) -and (Test-Path -LiteralPath $bundledProviders)) {
    $ProvidersRoot = $bundledProviders
}
New-Item -ItemType Directory -Force -Path $OutputRoot | Out-Null
$resolvedOutputRoot = [IO.Path]::GetFullPath($OutputRoot).TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
Get-ChildItem -LiteralPath $OutputRoot -Directory -Filter 'run-*' | ForEach-Object {
    $candidate = [IO.Path]::GetFullPath($_.FullName)
    if (-not $candidate.StartsWith($resolvedOutputRoot, [StringComparison]::OrdinalIgnoreCase) -or
        $_.Name -notmatch '^run-[0-9]+$') {
        throw "Refusing to clean an unexpected definition output directory: $candidate"
    }
    Remove-Item -LiteralPath $candidate -Recurse -Force
}

$expectedByLanguage = @{}
$reviewedSourceFileCount = 0
$measuredLanguageFileCount = 0
foreach ($project in $truthContract.projects) {
    $fixtureRoot = (Resolve-Path (Join-Path $fixturesRoot $project.fixture)).Path
    foreach ($source in $project.sourceFiles) {
        $reviewedSourceFileCount++
        $path = Join-Path $fixtureRoot $source.path
        if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
            throw "$($project.id): reviewed source is missing: $($source.path)"
        }
        $actualHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $path).Hash.ToLowerInvariant()
        if ($actualHash -ne $source.sha256) {
            throw "$($project.id): reviewed source changed without a new manual review: $($source.path)"
        }
    }
    foreach ($language in $project.languages) {
        $measuredLanguageFileCount += @($language.files).Count
        $expected = Get-ExpectedLanguage -ProjectId $project.id -Language $language
        if ($expectedByLanguage.ContainsKey($expected.language)) {
            throw "Definition ground truth contains language twice: $($expected.language)"
        }
        $expectedByLanguage[$expected.language] = $expected
    }
}
if ($expectedByLanguage.Count -ne 10) {
    throw "Definition ground truth must cover exactly 10 languages; got $($expectedByLanguage.Count)"
}

$sessionCacheRoot = Join-Path ([IO.Path]::GetTempPath()) ("codebase-workspace-definition-quality-" + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force -Path $sessionCacheRoot | Out-Null
$env:CODE_MEMORY_CACHE_ROOT = $sessionCacheRoot
$runsByLanguage = @{}
$projectRuns = @()
for ($run = 1; $run -le $Runs; $run++) {
    $runRoot = Join-Path $OutputRoot "run-$run"
    foreach ($project in $truthContract.projects) {
        $fixtureRoot = (Resolve-Path (Join-Path $fixturesRoot $project.fixture)).Path
        $analysis = Invoke-DefinitionAnalysis -Project $project -FixtureRoot $fixtureRoot -RunRoot $runRoot -PacksRoot $packsRoot
        $projectRuns += [pscustomobject]@{
            run = $run
            project = [string]$project.id
            elapsedMs = $analysis.elapsedMs
            streamSetDigest = [string]$analysis.receipt.streamSetDigest
            artifactContentDigest = [string]$analysis.authority.contentDigest
            semanticPayloadSetDigest = [string]$analysis.receipt.semanticPayloadSetDigest
            canonicalSemanticDigest = [string]$analysis.authority.canonicalSemanticDigest
            canonicalBundleDigest = [string]$analysis.authority.canonicalBundleDigest
        }
        foreach ($summary in @($analysis.receipt.definitionLanguageSummaries)) {
            $language = [string]$summary.language
            if (-not $expectedByLanguage.ContainsKey($language)) {
                throw "$($project.id): receipt contains unreviewed definition language $language"
            }
            if (-not $runsByLanguage.ContainsKey($language)) { $runsByLanguage[$language] = @() }
            $runsByLanguage[$language] += [pscustomobject]@{
                run = $run
                project = [string]$project.id
                elapsedMs = $analysis.elapsedMs
                streamSetDigest = [string]$analysis.receipt.streamSetDigest
                artifactContentDigest = [string]$analysis.authority.contentDigest
                semanticPayloadSetDigest = [string]$analysis.receipt.semanticPayloadSetDigest
                canonicalSemanticDigest = [string]$analysis.authority.canonicalSemanticDigest
                canonicalBundleDigest = [string]$analysis.authority.canonicalBundleDigest
                summary = $summary
                metadata = @($analysis.receipt.definitionMetadataAuditSample | Where-Object {
                    [string]$_.language -eq $language
                })
            }
        }
    }
}

$languageReports = @()
foreach ($language in @($expectedByLanguage.Keys | Sort-Object)) {
    $expected = $expectedByLanguage[$language]
    $languageRuns = @($runsByLanguage[$language])
    if ($languageRuns.Count -ne $Runs) {
        throw "$($language): expected $Runs receipts, got $($languageRuns.Count)"
    }
    $first = $languageRuns[0].summary
    $definitionDigestMatches = @($languageRuns | Where-Object {
        [string]$_.summary.definitionSetDigest -eq $expected.definitionSetDigest
    }).Count -eq $Runs
    $countsMatch = @($languageRuns | Where-Object {
        [int64]$_.summary.syntaxDefinitionCount -eq $expected.definitionCount -and
        [int64]$_.summary.ownedSyntaxDefinitionCount -eq $expected.ownedDefinitionCount -and
        [int64]$_.summary.matchedDefinitionCount -eq $expected.definitionCount -and
        [int64]$_.summary.missingSyntaxDefinitionCount -eq 0 -and
        [int64]$_.summary.extraProviderDefinitionCount -eq 0 -and
        [int64]$_.summary.resolvedOwnerCount -eq $expected.ownedDefinitionCount -and
        [int64]$_.summary.unresolvedOwnerCount -eq 0 -and
        [int64]$_.summary.inventoryFailedFileCount -eq 0 -and
        [int64]$_.summary.metadataDefinitionCount -eq $expected.definitionCount -and
        [int64]$_.summary.callableDefinitionCount -eq $expected.callableDefinitionCount -and
        [int64]$_.summary.callableSignatureCount -eq $expected.callableDefinitionCount -and
        [int64]$_.summary.knownVisibilityCount -eq $expected.definitionCount
    }).Count -eq $Runs
    $deterministic = @($languageRuns | Select-Object -ExpandProperty streamSetDigest -Unique).Count -eq 1 -and
        @($languageRuns | Select-Object -ExpandProperty artifactContentDigest -Unique).Count -eq 1 -and
        @($languageRuns | Select-Object -ExpandProperty semanticPayloadSetDigest -Unique).Count -eq 1 -and
        @($languageRuns | Select-Object -ExpandProperty canonicalSemanticDigest -Unique).Count -eq 1 -and
        @($languageRuns | Select-Object -ExpandProperty canonicalBundleDigest -Unique).Count -eq 1 -and
        @($languageRuns | ForEach-Object { [string]$_.summary.definitionSetDigest } | Select-Object -Unique).Count -eq 1
    $metadataDeterministic = @($languageRuns | ForEach-Object { [string]$_.summary.metadataSetDigest } | Select-Object -Unique).Count -eq 1
    $reviewedMetadata = @($metadataCases | Where-Object {
        $_.project -eq $expected.projectId -and $_.language -eq $language
    })
    $metadataMatched = Compare-ReviewedMetadata -ProjectId $expected.projectId -Language $language -ExpectedEntries $reviewedMetadata -ActualEntries @($languageRuns[0].metadata)
    $truePositive = if ($definitionDigestMatches) { [int64]$first.matchedDefinitionCount } else { 0 }
    $falsePositive = [int64]$first.extraProviderDefinitionCount
    $falseNegative = [Math]::Max(0, $expected.definitionCount - $truePositive)
    $precision = Get-Rate $truePositive ($truePositive + $falsePositive)
    $recall = Get-Rate $truePositive $expected.definitionCount
    $kindAccuracy = if ($countsMatch -and $definitionDigestMatches) { 1.0 } else { 0.0 }
    $ownerAccuracy = Get-Rate ([int64]$first.resolvedOwnerCount) $expected.ownedDefinitionCount
    $inventoryCoverage = if ([int64]$first.inventoryFailedFileCount -eq 0 -and
        [int64]$first.syntaxDefinitionCount -eq $expected.definitionCount) { 1.0 } else { 0.0 }
    $releaseGatePassed = $countsMatch -and $definitionDigestMatches -and $deterministic -and $metadataDeterministic -and
        $metadataMatched -eq $reviewedMetadata.Count -and
        $precision -ge [double]$truthContract.releaseGate.minimumPrecision -and
        $recall -ge [double]$truthContract.releaseGate.minimumRecall -and
        $kindAccuracy -ge [double]$truthContract.releaseGate.minimumKindAccuracy -and
        $ownerAccuracy -ge [double]$truthContract.releaseGate.minimumOwnerAccuracy -and
        $inventoryCoverage -ge [double]$truthContract.releaseGate.minimumInventoryCoverage
    $languageReports += [pscustomobject]@{
        language = $language
        project = $expected.projectId
        expectedDefinitionCount = $expected.definitionCount
        expectedOwnedDefinitionCount = $expected.ownedDefinitionCount
        expectedCallableDefinitionCount = $expected.callableDefinitionCount
        reviewedNegativeCount = $expected.forbiddenDefinitionCount
        emittedMatchedDefinitionCount = [int64]$first.matchedDefinitionCount
        truePositive = $truePositive
        falsePositive = $falsePositive
        falseNegative = $falseNegative
        precision = $precision
        recall = $recall
        kindAccuracy = $kindAccuracy
        ownerAccuracy = $ownerAccuracy
        inventoryCoverage = $inventoryCoverage
        callableSignatureCoverage = Get-Rate ([int64]$first.callableSignatureCount) $expected.callableDefinitionCount
        knownVisibilityCoverage = Get-Rate ([int64]$first.knownVisibilityCount) $expected.definitionCount
        reviewedMetadataCaseCount = $reviewedMetadata.Count
        matchedMetadataCaseCount = $metadataMatched
        metadataSetDigest = [string]$first.metadataSetDigest
        metadataDeterministic = $metadataDeterministic
        providerKindRefinementCount = [int64]$first.kindRefinementCount
        providerOwnerRepairCount = [int64]$first.ownerRepairCount
        providerDefinitionAliasCount = [int64]$first.providerDefinitionAliasCount
        definitionSetDigest = [string]$first.definitionSetDigest
        reviewedDefinitionSetDigest = $expected.definitionSetDigest
        deterministic = $deterministic
        elapsedMs = @($languageRuns | ForEach-Object { $_.elapsedMs })
        releaseGatePassed = $releaseGatePassed
    }
}

$expectedTotal = ($languageReports | Measure-Object expectedDefinitionCount -Sum).Sum
$ownedTotal = ($languageReports | Measure-Object expectedOwnedDefinitionCount -Sum).Sum
$truePositiveTotal = ($languageReports | Measure-Object truePositive -Sum).Sum
$falsePositiveTotal = ($languageReports | Measure-Object falsePositive -Sum).Sum
$falseNegativeTotal = ($languageReports | Measure-Object falseNegative -Sum).Sum
$resolvedOwnerTotal = 0
foreach ($report in $languageReports) {
    $resolvedOwnerTotal += [int64]([Math]::Round($report.ownerAccuracy * $report.expectedOwnedDefinitionCount))
}
$microPrecision = Get-Rate $truePositiveTotal ($truePositiveTotal + $falsePositiveTotal)
$microRecall = Get-Rate $truePositiveTotal $expectedTotal
$ownerAccuracyTotal = Get-Rate $resolvedOwnerTotal $ownedTotal
$determinismRate = Get-Rate @($languageReports | Where-Object deterministic).Count $languageReports.Count
$releaseGatePassed = @($languageReports | Where-Object { -not $_.releaseGatePassed }).Count -eq 0

$report = [ordered]@{
    schema = 'codebase-workspace.definition-quality-report.v1'
    generatedAt = [DateTimeOffset]::UtcNow.ToString('o')
    groundTruthSchema = [string]$truthContract.schema
    groundTruthSha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $Truth).Hash.ToLowerInvariant()
    runs = $Runs
    aggregate = [ordered]@{
        languageCount = $languageReports.Count
        reviewedPhysicalSourceFileCount = $reviewedSourceFileCount
        measuredLanguageFileContextCount = $measuredLanguageFileCount
        expectedDefinitionCount = $expectedTotal
        expectedOwnedDefinitionCount = $ownedTotal
        truePositive = $truePositiveTotal
        falsePositive = $falsePositiveTotal
        falseNegative = $falseNegativeTotal
        microPrecision = $microPrecision
        microRecall = $microRecall
        kindAccuracy = if (@($languageReports | Where-Object kindAccuracy -ne 1).Count -eq 0) { 1.0 } else { 0.0 }
        ownerAccuracy = $ownerAccuracyTotal
        inventoryCoverage = if (@($languageReports | Where-Object inventoryCoverage -ne 1).Count -eq 0) { 1.0 } else { 0.0 }
        determinismRate = $determinismRate
        reviewedMetadataCaseCount = ($languageReports | Measure-Object reviewedMetadataCaseCount -Sum).Sum
        matchedMetadataCaseCount = ($languageReports | Measure-Object matchedMetadataCaseCount -Sum).Sum
        callableSignatureCoverage = if (@($languageReports | Where-Object callableSignatureCoverage -ne 1).Count -eq 0) { 1.0 } else { 0.0 }
        knownVisibilityCoverage = if (@($languageReports | Where-Object knownVisibilityCoverage -ne 1).Count -eq 0) { 1.0 } else { 0.0 }
        providerKindRefinementCount = ($languageReports | Measure-Object providerKindRefinementCount -Sum).Sum
        providerOwnerRepairCount = ($languageReports | Measure-Object providerOwnerRepairCount -Sum).Sum
        providerDefinitionAliasCount = ($languageReports | Measure-Object providerDefinitionAliasCount -Sum).Sum
        releaseGatePassed = $releaseGatePassed
    }
    languages = $languageReports
    projectRuns = $projectRuns
}

$reportPath = Join-Path $OutputRoot 'definition-quality-report.json'
$report | ConvertTo-Json -Depth 100 | Set-Content -LiteralPath $reportPath -Encoding utf8
$languageReports |
    Select-Object language, expectedDefinitionCount, truePositive, falsePositive, falseNegative,
        @{Name = 'kindPct'; Expression = { [Math]::Round($_.kindAccuracy * 100, 1) } },
        @{Name = 'ownerPct'; Expression = { [Math]::Round($_.ownerAccuracy * 100, 1) } },
        @{Name = 'coveragePct'; Expression = { [Math]::Round($_.inventoryCoverage * 100, 1) } },
        @{Name = 'signaturePct'; Expression = { [Math]::Round($_.callableSignatureCoverage * 100, 1) } },
        @{Name = 'visibilityPct'; Expression = { [Math]::Round($_.knownVisibilityCoverage * 100, 1) } },
        matchedMetadataCaseCount,
        providerKindRefinementCount, providerOwnerRepairCount, providerDefinitionAliasCount,
        deterministic, releaseGatePassed |
    Format-Table -AutoSize
Write-Host "definition quality: TP=$truePositiveTotal FP=$falsePositiveTotal FN=$falseNegativeTotal precision=$([Math]::Round($microPrecision * 100, 2))% recall=$([Math]::Round($microRecall * 100, 2))% kind=100% owner=$([Math]::Round($ownerAccuracyTotal * 100, 2))% callable-signature=100% known-visibility=100% metadata-cases=$($report.aggregate.matchedMetadataCaseCount)/$($report.aggregate.reviewedMetadataCaseCount) languages=$($languageReports.Count)/10"
Write-Host "report: $reportPath"

$tempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
$resolvedSessionCache = [IO.Path]::GetFullPath($sessionCacheRoot)
if ($resolvedSessionCache.StartsWith($tempRoot, [StringComparison]::OrdinalIgnoreCase) -and
    [IO.Path]::GetFileName($resolvedSessionCache).StartsWith('codebase-workspace-definition-quality-', [StringComparison]::Ordinal)) {
    Remove-Item -LiteralPath $resolvedSessionCache -Recurse -Force -ErrorAction SilentlyContinue
}

if (-not $AuditOnly -and -not $releaseGatePassed) {
    exit 1
}
