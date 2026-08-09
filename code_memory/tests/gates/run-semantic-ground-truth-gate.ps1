param(
    [string]$Bridge = (Join-Path $PSScriptRoot '..\..\rust\target\release\code-memory-language.exe'),
    [string]$ProvidersRoot,
    [string]$Truth = (Join-Path $PSScriptRoot '..\ground_truth\semantic-core.v2.json'),
    [string]$FixturesRoot = (Join-Path $PSScriptRoot '..\fixtures'),
    [string]$OutputRoot = (Join-Path $PSScriptRoot '..\..\build\semantic-ground-truth'),
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
    param([int]$Numerator, [int]$Denominator)
    if ($Denominator -eq 0) { return 1.0 }
    return [Math]::Round($Numerator / $Denominator, 6)
}

function Get-Sha256Text {
    param([string]$Value)
    $algorithm = [Security.Cryptography.SHA256]::Create()
    try {
        $bytes = [Text.Encoding]::UTF8.GetBytes($Value)
        return -join ($algorithm.ComputeHash($bytes) | ForEach-Object { $_.ToString('x2') })
    }
    finally {
        $algorithm.Dispose()
    }
}

function Get-ReviewedCoordinate {
    param(
        [string]$FixtureRoot,
        [pscustomobject]$Expected
    )

    $sourcePath = Join-Path $FixtureRoot $Expected.path
    $content = (Get-Content -Raw -LiteralPath $sourcePath).Replace("`r`n", "`n")
    $snippet = [string]$Expected.snippet
    $snippetIndex = $content.IndexOf($snippet, [StringComparison]::Ordinal)
    if ($snippetIndex -lt 0) {
        throw "Ground truth $($Expected.id): reviewed snippet is absent from $($Expected.path)"
    }
    if ($content.IndexOf($snippet, $snippetIndex + $snippet.Length, [StringComparison]::Ordinal) -ge 0) {
        throw "Ground truth $($Expected.id): reviewed snippet is not unique in $($Expected.path)"
    }

    $occurrenceProperty = $Expected.PSObject.Properties['tokenOccurrence']
    $occurrence = if ($null -eq $occurrenceProperty) { 1 } else { [int]$occurrenceProperty.Value }
    $tokenIndex = -1
    $searchStart = 0
    for ($index = 0; $index -lt $occurrence; $index++) {
        $tokenIndex = $snippet.IndexOf([string]$Expected.token, $searchStart, [StringComparison]::Ordinal)
        if ($tokenIndex -lt 0) {
            throw "Ground truth $($Expected.id): token occurrence $occurrence is absent from the reviewed snippet"
        }
        $searchStart = $tokenIndex + ([string]$Expected.token).Length
    }

    $absoluteIndex = $snippetIndex + $tokenIndex
    $prefix = $content.Substring(0, $absoluteIndex)
    $line = ([regex]::Matches($prefix, "`n")).Count
    $lastNewline = $prefix.LastIndexOf("`n", [StringComparison]::Ordinal)
    $column = if ($lastNewline -lt 0) { $prefix.Length } else { $prefix.Length - $lastNewline - 1 }
    return [pscustomobject]@{
        id = [string]$Expected.id
        kind = [string]$Expected.kind
        path = [string]$Expected.path
        line = $line
        column = $column
        token = [string]$Expected.token
        targetPattern = [string]$Expected.targetPattern
    }
}

function Test-RangeContains {
    param(
        [object]$Range,
        [int]$Line,
        [int]$Column
    )

    $values = @($Range | ForEach-Object { [int]$_ })
    if ($values.Count -eq 3) {
        return $Line -eq $values[0] -and $Column -ge $values[1] -and $Column -lt $values[2]
    }
    if ($values.Count -eq 4) {
        $afterStart = $Line -gt $values[0] -or ($Line -eq $values[0] -and $Column -ge $values[1])
        $beforeEnd = $Line -lt $values[2] -or ($Line -eq $values[2] -and $Column -lt $values[3])
        return $afterStart -and $beforeEnd
    }
    return $false
}

function Test-RelationEvidence {
    param(
        [string]$FixtureRoot,
        [pscustomobject]$Relation
    )

    if ([string]::IsNullOrWhiteSpace($Relation.path) -or [IO.Path]::IsPathRooted($Relation.path)) {
        return $false
    }
    $root = [IO.Path]::GetFullPath($FixtureRoot).TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
    $path = [IO.Path]::GetFullPath((Join-Path $FixtureRoot $Relation.path))
    if (-not $path.StartsWith($root, [StringComparison]::OrdinalIgnoreCase) -or -not (Test-Path -LiteralPath $path -PathType Leaf)) {
        return $false
    }

    $range = @($Relation.range | ForEach-Object { [int]$_ })
    if ($range.Count -notin @(3, 4)) { return $false }
    $lines = (Get-Content -Raw -LiteralPath $path).Replace("`r`n", "`n").Split("`n")
    if ($range[0] -lt 0 -or $range[0] -ge $lines.Count -or $range[1] -lt 0) { return $false }
    if ($range.Count -eq 3) {
        return $range[2] -gt $range[1] -and $range[2] -le $lines[$range[0]].Length
    }
    if ($range[2] -lt $range[0] -or $range[2] -ge $lines.Count -or $range[3] -lt 0) { return $false }
    if ($range[1] -gt $lines[$range[0]].Length -or $range[3] -gt $lines[$range[2]].Length) { return $false }
    return $range[2] -gt $range[0] -or $range[3] -gt $range[1]
}

function Get-SemanticDigest {
    param([pscustomobject]$Result)
    $projection = [ordered]@{
        languages = @($Result.languages | Sort-Object id)
        analysisUnits = @($Result.analysis_units | Sort-Object language, id | Select-Object id, language, root,
            files_found, files_indexed, files_excluded, files_missing, status, provider, reason)
        coverage = @($Result.coverage | Sort-Object language, path)
        documents = @($Result.documents | Sort-Object language, path)
        relations = @($Result.relations | Sort-Object kind, path, from, to, { @($_.range) -join ',' })
        fileRelations = @($Result.file_relations | Sort-Object kind, from, to)
        projectModelFiles = @($Result.project_model_files | Sort-Object)
        diagnostics = @($Result.diagnostics | Sort-Object language, code, path, line | Select-Object language,
            level, code, detail, path, line)
    }
    return Get-Sha256Text ($projection | ConvertTo-Json -Depth 100 -Compress)
}

function Invoke-Analysis {
    param(
        [pscustomobject]$Case,
        [string]$FixtureRoot,
        [string]$RunRoot,
        [string]$PacksRoot
    )

    New-Item -ItemType Directory -Force -Path $RunRoot | Out-Null
    $outputPath = Join-Path $RunRoot "$($Case.id).json"
    $arguments = @('index', '--root', $FixtureRoot, '--out', $outputPath, '--packs-root', $PacksRoot)
    if (-not [string]::IsNullOrWhiteSpace($ProvidersRoot)) {
        $arguments += @('--providers-root', (Resolve-Path $ProvidersRoot).Path)
    }
    $stopwatch = [Diagnostics.Stopwatch]::StartNew()
    $previousErrorActionPreference = $ErrorActionPreference
    try {
        # Windows PowerShell 5 turns native stderr into ErrorRecord objects. The
        # bridge intentionally writes progress events to stderr, so `Stop` would
        # abort a successful analysis before `$LASTEXITCODE` can be inspected.
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
        throw "$($Case.id): provider analysis failed with exit $exitCode`n$detail"
    }
    if (-not (Test-Path -LiteralPath $outputPath -PathType Leaf)) {
        throw "$($Case.id): analysis did not write $outputPath"
    }
    $receiptPrefix = '@codebase-workspace-language-ir '
    $receiptLine = @($bridgeOutput | ForEach-Object { $_.ToString() } | Where-Object {
            $_.StartsWith($receiptPrefix, [StringComparison]::Ordinal)
        }) | Select-Object -First 1
    if ([string]::IsNullOrWhiteSpace($receiptLine)) {
        throw "$($Case.id): analysis emitted no Language IR receipt"
    }
    $receipt = $receiptLine.Substring($receiptPrefix.Length) | ConvertFrom-Json
    if ($receipt.schema -ne 'codebase-workspace.language-ir-migration-receipt.v6') {
        throw "$($Case.id): unsupported Language IR receipt schema $($receipt.schema)"
    }
    $authority = Get-LanguageIrStreamAuthority -BridgeOutput $bridgeOutput -Receipt $receipt -Context $Case.id
    return [pscustomobject]@{
        elapsedMs = $stopwatch.ElapsedMilliseconds
        artifactContentDigest = [string]$authority.contentDigest
        canonicalSemanticDigest = [string]$authority.canonicalSemanticDigest
        canonicalBundleDigest = [string]$authority.canonicalBundleDigest
        result = Get-Content -Raw -LiteralPath $outputPath | ConvertFrom-Json
    }
}

if (-not (Test-Path -LiteralPath $Bridge -PathType Leaf)) {
    throw "Bridge not found: $Bridge"
}
if (-not (Test-Path -LiteralPath $Truth -PathType Leaf)) {
    throw "Ground truth not found: $Truth"
}

$truthContract = Get-Content -Raw -LiteralPath $Truth | ConvertFrom-Json
if ($truthContract.schema -ne 'codebase-workspace.semantic-ground-truth.v2') {
    throw "Unsupported ground-truth schema: $($truthContract.schema)"
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
        throw "Refusing to clean an unexpected ground-truth output directory: $candidate"
    }
    Remove-Item -LiteralPath $candidate -Recurse -Force
}
$sessionCacheRoot = Join-Path ([IO.Path]::GetTempPath()) ("codebase-workspace-semantic-quality-" + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force -Path $sessionCacheRoot | Out-Null
$env:CODE_MEMORY_CACHE_ROOT = $sessionCacheRoot

$reviewedByCase = @{}
foreach ($case in $truthContract.cases) {
    $fixtureRoot = (Resolve-Path (Join-Path $fixturesRoot $case.fixture)).Path
    foreach ($source in $case.sourceFiles) {
        $path = Join-Path $fixtureRoot $source.path
        if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
            throw "$($case.id): reviewed source is missing: $($source.path)"
        }
        $actualHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $path).Hash.ToLowerInvariant()
        if ($actualHash -ne $source.sha256) {
            throw "$($case.id): reviewed source changed without a new manual review: $($source.path)"
        }
    }
    $reviewedByCase[$case.id] = @($case.expectedRelations | ForEach-Object {
        Get-ReviewedCoordinate -FixtureRoot $fixtureRoot -Expected $_
    })
}

$runsByCase = @{}
for ($run = 1; $run -le $Runs; $run++) {
    $runRoot = Join-Path $OutputRoot "run-$run"
    foreach ($case in $truthContract.cases) {
        $fixtureRoot = (Resolve-Path (Join-Path $fixturesRoot $case.fixture)).Path
        $analysis = Invoke-Analysis -Case $case -FixtureRoot $fixtureRoot -RunRoot $runRoot -PacksRoot $packsRoot
        if (-not $runsByCase.ContainsKey($case.id)) { $runsByCase[$case.id] = @() }
        $runsByCase[$case.id] += [pscustomobject]@{
            elapsedMs = $analysis.elapsedMs
            digest = Get-SemanticDigest $analysis.result
            artifactContentDigest = $analysis.artifactContentDigest
            canonicalSemanticDigest = $analysis.canonicalSemanticDigest
            canonicalBundleDigest = $analysis.canonicalBundleDigest
            result = $analysis.result
        }
    }
}

$caseReports = @()
foreach ($case in $truthContract.cases) {
    $fixtureRoot = (Resolve-Path (Join-Path $fixturesRoot $case.fixture)).Path
    $caseRuns = @($runsByCase[$case.id])
    $result = $caseRuns[0].result
    $language = @($result.languages | Where-Object id -eq $case.id) | Select-Object -First 1
    $providerStatus = if ($null -eq $language) { 'missing-language-output' } else { [string]$language.status }
    $expectedPaths = @($case.sourceFiles | ForEach-Object { [string]$_.path })
    $ownedPaths = @($result.documents | Where-Object language -eq $case.id | ForEach-Object { [string]$_.path })
    $measuredPaths = @($expectedPaths + $ownedPaths | Sort-Object -Unique)
    $actual = @($result.relations | Where-Object {
        $_.kind -in @('CALLS', 'CONSTRUCTS') -and
        $_.to -match $case.projectTargetPattern -and
        $measuredPaths -contains $_.path
    } | ForEach-Object {
        [pscustomobject]@{
            matched = $false
            evidenceValid = Test-RelationEvidence -FixtureRoot $fixtureRoot -Relation $_
            relation = $_
        }
    })

    $falseNegatives = @()
    $truePositive = 0
    foreach ($expected in $reviewedByCase[$case.id]) {
        $match = @($actual | Where-Object {
            -not $_.matched -and
            $_.relation.kind -eq $expected.kind -and
            $_.relation.path -eq $expected.path -and
            $_.relation.to -match $expected.targetPattern -and
            (Test-RangeContains -Range $_.relation.range -Line $expected.line -Column $expected.column)
        }) | Select-Object -First 1
        if ($null -eq $match) {
            $falseNegatives += $expected
        }
        else {
            $match.matched = $true
            $truePositive++
        }
    }
    $falsePositives = @($actual | Where-Object { -not $_.matched } | ForEach-Object {
        [pscustomobject]@{
            kind = $_.relation.kind
            path = $_.relation.path
            range = @($_.relation.range)
            from = $_.relation.from
            to = $_.relation.to
            evidenceValid = $_.evidenceValid
        }
    })
    $falseNegativeCount = $falseNegatives.Count
    $falsePositiveCount = $falsePositives.Count
    $precision = Get-Rate $truePositive ($truePositive + $falsePositiveCount)
    $recall = Get-Rate $truePositive ($truePositive + $falseNegativeCount)
    $f1 = if (($precision + $recall) -eq 0) { 0.0 } else { [Math]::Round(2 * $precision * $recall / ($precision + $recall), 6) }

    $coverage = @($result.coverage | Where-Object language -eq $case.id)
    $indexedSources = 0
    $sourceFailures = @()
    foreach ($source in $case.sourceFiles) {
        $entry = @($coverage | Where-Object path -eq $source.path) | Select-Object -First 1
        if ($null -ne $entry -and $entry.status -eq 'indexed') {
            $indexedSources++
        }
        else {
            $sourceFailures += [pscustomobject]@{
                path = $source.path
                status = if ($null -eq $entry) { 'missing-coverage-entry' } else { $entry.status }
                reason = if ($null -eq $entry) { $null } else { $entry.reason }
            }
        }
    }
    $sourceCoverage = Get-Rate $indexedSources @($case.sourceFiles).Count
    $unexpectedCoverage = @($coverage | Where-Object { $expectedPaths -notcontains $_.path } | ForEach-Object {
        [pscustomobject]@{ path = $_.path; status = $_.status; reason = $_.reason }
    })
    $validEvidenceCount = @($actual | Where-Object evidenceValid).Count
    $evidenceValidity = Get-Rate $validEvidenceCount $actual.Count
    $languageDocuments = @($result.documents | Where-Object language -eq $case.id)
    $symbols = @($languageDocuments | ForEach-Object { @($_.symbols) })
    $definitionSymbols = @{}
    foreach ($document in $languageDocuments) {
        foreach ($occurrence in @($document.occurrences | Where-Object definition)) {
            if (-not [string]::IsNullOrWhiteSpace([string]$occurrence.symbol)) {
                $definitionSymbols[[string]$occurrence.symbol] = $true
            }
        }
    }
    $ownedMembers = @($symbols | Where-Object { $_.kind -in @('Method', 'Constructor', 'Field', 'Property') })
    $missingMemberOwners = @($ownedMembers | Where-Object {
        [string]::IsNullOrWhiteSpace([string]$_.enclosing_symbol)
    } | ForEach-Object {
        [pscustomobject]@{ symbol = $_.symbol; kind = $_.kind; reason = 'missing-parent' }
    })
    $danglingMemberOwners = @($ownedMembers | Where-Object {
        -not [string]::IsNullOrWhiteSpace([string]$_.enclosing_symbol) -and
        -not $definitionSymbols.ContainsKey([string]$_.enclosing_symbol)
    } | ForEach-Object {
        [pscustomobject]@{
            symbol = $_.symbol
            kind = $_.kind
            parent = $_.enclosing_symbol
            reason = 'parent-is-not-a-definition'
        }
    })
    $memberOwnershipFailures = @($missingMemberOwners + $danglingMemberOwners)
    $memberOwnershipValidity = Get-Rate ($ownedMembers.Count - $memberOwnershipFailures.Count) $ownedMembers.Count
    $digests = @($caseRuns | ForEach-Object digest | Sort-Object -Unique)
    $artifactDigests = @($caseRuns | ForEach-Object artifactContentDigest | Sort-Object -Unique)
    $canonicalSemanticDigests = @($caseRuns | ForEach-Object canonicalSemanticDigest | Sort-Object -Unique)
    $canonicalBundleDigests = @($caseRuns | ForEach-Object canonicalBundleDigest | Sort-Object -Unique)
    $deterministic = $Runs -ge 2 -and $digests.Count -eq 1 -and $artifactDigests.Count -eq 1 -and
        $canonicalSemanticDigests.Count -eq 1 -and $canonicalBundleDigests.Count -eq 1
    $elapsed = @($caseRuns | ForEach-Object elapsedMs)
    $casePassed = $providerStatus -eq 'indexed' -and
        $precision -ge [double]$truthContract.releaseGate.minimumPrecision -and
        $recall -ge [double]$truthContract.releaseGate.minimumRecall -and
        $sourceCoverage -ge [double]$truthContract.releaseGate.minimumSourceCoverage -and
        $evidenceValidity -ge [double]$truthContract.releaseGate.minimumEvidenceValidity -and
        $memberOwnershipFailures.Count -eq 0 -and
        (-not $truthContract.releaseGate.requireDeterminism -or $deterministic)
    $trustFloor = [Math]::Min([Math]::Min($precision, $recall), [Math]::Min($sourceCoverage, $evidenceValidity))
    $trustFloor = [Math]::Min($trustFloor, $memberOwnershipValidity)
    if ($truthContract.releaseGate.requireDeterminism -and -not $deterministic) { $trustFloor = 0.0 }

    $caseReports += [pscustomobject]@{
        language = $case.id
        providerStatus = $providerStatus
        reviewedSourceFileCount = @($case.sourceFiles).Count
        indexedSourceFileCount = $indexedSources
        sourceCoverage = $sourceCoverage
        expectedRelationCount = @($case.expectedRelations).Count
        emittedMeasuredRelationCount = $actual.Count
        truePositive = $truePositive
        falsePositive = $falsePositiveCount
        falseNegative = $falseNegativeCount
        precision = $precision
        recall = $recall
        f1 = $f1
        validEvidenceCount = $validEvidenceCount
        evidenceValidity = $evidenceValidity
        ownedMemberCount = $ownedMembers.Count
        validMemberOwnerCount = $ownedMembers.Count - $memberOwnershipFailures.Count
        memberOwnershipValidity = $memberOwnershipValidity
        memberOwnershipFailures = $memberOwnershipFailures
        deterministic = $deterministic
        semanticDigest = if ($digests.Count -eq 1) { $digests[0] } else { $null }
        canonicalSemanticDigest = if ($canonicalSemanticDigests.Count -eq 1) { $canonicalSemanticDigests[0] } else { $null }
        canonicalBundleDigest = if ($canonicalBundleDigests.Count -eq 1) { $canonicalBundleDigests[0] } else { $null }
        elapsedMs = $elapsed
        trustScore = [Math]::Round($trustFloor * 100, 2)
        releaseGatePassed = $casePassed
        sourceFailures = $sourceFailures
        unexpectedCoverage = $unexpectedCoverage
        falsePositives = $falsePositives
        falseNegatives = $falseNegatives
    }
}

$truePositiveTotal = ($caseReports | Measure-Object truePositive -Sum).Sum
$falsePositiveTotal = ($caseReports | Measure-Object falsePositive -Sum).Sum
$falseNegativeTotal = ($caseReports | Measure-Object falseNegative -Sum).Sum
$reviewedSourceTotal = ($caseReports | Measure-Object reviewedSourceFileCount -Sum).Sum
$indexedSourceTotal = ($caseReports | Measure-Object indexedSourceFileCount -Sum).Sum
$validEvidenceTotal = ($caseReports | Measure-Object validEvidenceCount -Sum).Sum
$emittedMeasuredTotal = ($caseReports | Measure-Object emittedMeasuredRelationCount -Sum).Sum
$ownedMemberTotal = ($caseReports | Measure-Object ownedMemberCount -Sum).Sum
$validMemberOwnerTotal = ($caseReports | Measure-Object validMemberOwnerCount -Sum).Sum
$microPrecision = Get-Rate $truePositiveTotal ($truePositiveTotal + $falsePositiveTotal)
$microRecall = Get-Rate $truePositiveTotal ($truePositiveTotal + $falseNegativeTotal)
$microF1 = if (($microPrecision + $microRecall) -eq 0) { 0.0 } else { [Math]::Round(2 * $microPrecision * $microRecall / ($microPrecision + $microRecall), 6) }
$macroPrecision = [Math]::Round(($caseReports | Measure-Object precision -Average).Average, 6)
$macroRecall = [Math]::Round(($caseReports | Measure-Object recall -Average).Average, 6)
$macroF1 = [Math]::Round(($caseReports | Measure-Object f1 -Average).Average, 6)
$sourceCoverageTotal = Get-Rate $indexedSourceTotal $reviewedSourceTotal
$evidenceValidityTotal = Get-Rate $validEvidenceTotal $emittedMeasuredTotal
$memberOwnershipValidityTotal = Get-Rate $validMemberOwnerTotal $ownedMemberTotal
$deterministicCount = @($caseReports | Where-Object deterministic).Count
$determinismRate = Get-Rate $deterministicCount $caseReports.Count
$aggregateTrustRate = [Math]::Min([Math]::Min($microPrecision, $microRecall), [Math]::Min($sourceCoverageTotal, $evidenceValidityTotal))
$aggregateTrustRate = [Math]::Min($aggregateTrustRate, $memberOwnershipValidityTotal)
$aggregateTrustRate = [Math]::Min($aggregateTrustRate, $determinismRate)
$minimumLanguageTrustScore = [Math]::Round(($caseReports | Measure-Object trustScore -Minimum).Minimum, 2)
$releaseGatePassed = @($caseReports | Where-Object { -not $_.releaseGatePassed }).Count -eq 0

$report = [ordered]@{
    schema = 'codebase-workspace.semantic-quality-report.v2'
    generatedAt = [DateTimeOffset]::Now.ToString('o')
    truthSchema = $truthContract.schema
    truthPath = (Resolve-Path $Truth).Path
    review = $truthContract.review
    releaseGate = $truthContract.releaseGate
    runs = $Runs
    cacheProtocol = 'run 1 uses a fresh isolated cache; later runs reuse that cache'
    scope = 'project-local CALLS/CONSTRUCTS accuracy plus emitted member-owner integrity; exhaustive definition recall and other relations are unscored'
    aggregate = [ordered]@{
        languageCount = $caseReports.Count
        reviewedSourceFileCount = $reviewedSourceTotal
        indexedSourceFileCount = $indexedSourceTotal
        expectedRelationCount = $truePositiveTotal + $falseNegativeTotal
        emittedMeasuredRelationCount = $emittedMeasuredTotal
        truePositive = $truePositiveTotal
        falsePositive = $falsePositiveTotal
        falseNegative = $falseNegativeTotal
        microPrecision = $microPrecision
        microRecall = $microRecall
        microF1 = $microF1
        macroPrecision = $macroPrecision
        macroRecall = $macroRecall
        macroF1 = $macroF1
        sourceCoverage = $sourceCoverageTotal
        evidenceValidity = $evidenceValidityTotal
        ownedMemberCount = $ownedMemberTotal
        validMemberOwnerCount = $validMemberOwnerTotal
        memberOwnershipValidity = $memberOwnershipValidityTotal
        deterministicLanguageCount = $deterministicCount
        determinismRate = $determinismRate
        microQualityScore = [Math]::Round($microF1 * 100, 2)
        aggregateTrustScore = [Math]::Round($aggregateTrustRate * 100, 2)
        minimumLanguageTrustScore = $minimumLanguageTrustScore
        trustScore = $minimumLanguageTrustScore
        releaseGatePassed = $releaseGatePassed
    }
    cases = $caseReports
}

$reportPath = Join-Path $OutputRoot 'semantic-quality-report.json'
$report | ConvertTo-Json -Depth 100 | Set-Content -LiteralPath $reportPath -Encoding utf8
$caseReports |
    Select-Object language, truePositive, falsePositive, falseNegative,
        @{Name = 'precisionPct'; Expression = { [Math]::Round($_.precision * 100, 1) } },
        @{Name = 'recallPct'; Expression = { [Math]::Round($_.recall * 100, 1) } },
        @{Name = 'coveragePct'; Expression = { [Math]::Round($_.sourceCoverage * 100, 1) } },
        @{Name = 'ownershipPct'; Expression = { [Math]::Round($_.memberOwnershipValidity * 100, 1) } },
        deterministic, trustScore, releaseGatePassed |
    Format-Table -AutoSize
Write-Host "semantic quality: TP=$truePositiveTotal FP=$falsePositiveTotal FN=$falseNegativeTotal precision=$([Math]::Round($microPrecision * 100, 2))% recall=$([Math]::Round($microRecall * 100, 2))% coverage=$([Math]::Round($sourceCoverageTotal * 100, 2))% ownership=$([Math]::Round($memberOwnershipValidityTotal * 100, 2))% microF1=$([Math]::Round($microF1 * 100, 2))% weakestLanguage=$minimumLanguageTrustScore%"
Write-Host "report: $reportPath"

$tempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
$resolvedSessionCache = [IO.Path]::GetFullPath($sessionCacheRoot)
if ($resolvedSessionCache.StartsWith($tempRoot, [StringComparison]::OrdinalIgnoreCase) -and
    [IO.Path]::GetFileName($resolvedSessionCache).StartsWith('codebase-workspace-semantic-quality-', [StringComparison]::Ordinal)) {
    Remove-Item -LiteralPath $resolvedSessionCache -Recurse -Force -ErrorAction SilentlyContinue
}

if (-not $AuditOnly -and -not $releaseGatePassed) {
    exit 1
}
