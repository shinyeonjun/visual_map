param(
    [string]$Bridge = (Join-Path $PSScriptRoot '..\..\rust\target\release\code-memory-language.exe'),
    [string]$ProvidersRoot,
    [string]$Truth = (Join-Path $PSScriptRoot '..\ground_truth\imports.v1.json'),
    [string]$FixturesRoot = (Join-Path $PSScriptRoot '..\fixtures'),
    [string]$OutputRoot = (Join-Path $PSScriptRoot '..\..\build\import-ground-truth'),
    [ValidateRange(1, 5)]
    [int]$Runs = 2,
    [ValidateRange(0, 20000000)]
    [int]$MinimumSourceBytes = 0,
    [switch]$AuditOnly
)

. (Join-Path $PSScriptRoot 'lib\language-ir-stream-authority.ps1')

$ErrorActionPreference = 'Stop'
if (Get-Variable PSNativeCommandUseErrorActionPreference -ErrorAction SilentlyContinue) {
    $PSNativeCommandUseErrorActionPreference = $false
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

function Assert-PinnedFiles {
    param(
        [string]$Root,
        [object[]]$SourceFiles
    )
    foreach ($source in $SourceFiles) {
        $path = Join-Path $Root ([string]$source.path)
        if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
            throw "Reviewed import source is missing: $($source.path)"
        }
        $actual = (Get-FileHash -Algorithm SHA256 -LiteralPath $path).Hash.ToLowerInvariant()
        if ($actual -ne [string]$source.sha256) {
            throw "Reviewed import source changed without manual review: $($source.path) ($actual)"
        }
    }
}

function Get-ExpectedTokenCoordinates {
    param(
        [string]$FixtureRoot,
        [pscustomobject]$Expected
    )
    $sourcePath = Join-Path $FixtureRoot ([string]$Expected.path)
    $content = [IO.File]::ReadAllText($sourcePath).Replace("`r`n", "`n")
    $lines = $content.Split("`n")
    $lineNumber = [int]$Expected.line
    if ($lineNumber -lt 0 -or $lineNumber -ge $lines.Count) {
        throw "$($Expected.id): reviewed line $lineNumber is outside $($Expected.path)"
    }
    $line = $lines[$lineNumber]
    $token = [string]$Expected.token
    $characterStart = $line.IndexOf($token, [StringComparison]::Ordinal)
    if ($characterStart -lt 0) {
        throw "$($Expected.id): token '$token' is absent from $($Expected.path):$lineNumber"
    }
    if ($line.IndexOf($token, $characterStart + $token.Length, [StringComparison]::Ordinal) -ge 0) {
        throw "$($Expected.id): token '$token' is not unique on $($Expected.path):$lineNumber"
    }
    $prefix = $line.Substring(0, $characterStart)
    $throughToken = $line.Substring(0, $characterStart + $token.Length)
    return [pscustomobject]@{
        line = $lineNumber
        utf8Start = [Text.Encoding]::UTF8.GetByteCount($prefix)
        utf8End = [Text.Encoding]::UTF8.GetByteCount($throughToken)
        utf16Start = $prefix.Length
        utf16End = $throughToken.Length
    }
}

function Test-RangeContainsToken {
    param(
        [object]$Range,
        [int]$Line,
        [int]$Start,
        [int]$End
    )
    $values = @($Range | ForEach-Object { [int]$_ })
    if ($values.Count -eq 3) {
        return $values[0] -eq $Line -and $values[1] -le $Start -and $values[2] -ge $End
    }
    if ($values.Count -ne 4) { return $false }
    $startsBefore = $values[0] -lt $Line -or ($values[0] -eq $Line -and $values[1] -le $Start)
    $endsAfter = $values[2] -gt $Line -or ($values[2] -eq $Line -and $values[3] -ge $End)
    return $startsBefore -and $endsAfter
}

function Get-OptionalProperty {
    param(
        [pscustomobject]$Object,
        [string]$Name
    )
    $property = $Object.PSObject.Properties[$Name]
    if ($null -eq $property) { return $null }
    return $property.Value
}

function Get-SiteKey {
    param([pscustomobject]$Site)
    $range = @($Site.utf8Range)
    $line = if ($range.Count -gt 0) { [int]$range[0] } else { -1 }
    return "$($Site.language)`t$($Site.path)`t$($Site.capability)`t$($Site.specifier)`t$line"
}

function Get-ExpectedKey {
    param(
        [string]$Language,
        [pscustomobject]$Site
    )
    return "$Language`t$($Site.path)`t$($Site.capability)`t$($Site.specifier)`t$([int]$Site.line)"
}

function Assert-SiteMatches {
    param(
        [string]$FixtureRoot,
        [string]$Language,
        [pscustomobject]$Expected,
        [pscustomobject]$Actual
    )
    $coordinates = Get-ExpectedTokenCoordinates -FixtureRoot $FixtureRoot -Expected $Expected
    if (-not (Test-RangeContainsToken -Range $Actual.utf8Range -Line $coordinates.line `
            -Start $coordinates.utf8Start -End $coordinates.utf8End)) {
        throw "$($Expected.id): UTF-8 evidence does not cover the reviewed token"
    }
    if (-not (Test-RangeContainsToken -Range $Actual.utf16Range -Line $coordinates.line `
            -Start $coordinates.utf16Start -End $coordinates.utf16End)) {
        throw "$($Expected.id): UTF-16 evidence does not cover the reviewed token"
    }
    if ([string]$Actual.outcome -ne [string]$Expected.outcome) {
        throw "$($Expected.id): expected outcome $($Expected.outcome), got $($Actual.outcome)"
    }
    $expectedTarget = Get-OptionalProperty -Object $Expected -Name 'target'
    $expectedTargetSuffix = Get-OptionalProperty -Object $Expected -Name 'targetSuffix'
    $actualTarget = Get-OptionalProperty -Object $Actual -Name 'target'
    if ($null -ne $expectedTarget -and [string]$actualTarget -ne [string]$expectedTarget) {
        throw "$($Expected.id): expected target $expectedTarget, got $actualTarget"
    }
    if ($null -ne $expectedTargetSuffix -and
        -not ([string]$actualTarget).EndsWith([string]$expectedTargetSuffix, [StringComparison]::Ordinal)) {
        throw "$($Expected.id): target '$actualTarget' does not end with '$expectedTargetSuffix'"
    }
    foreach ($propertyName in @('resolutionMethod', 'gapCode', 'candidateCount')) {
        $expectedValue = Get-OptionalProperty -Object $Expected -Name $propertyName
        $actualValue = Get-OptionalProperty -Object $Actual -Name $propertyName
        if ($null -ne $expectedValue -and [string]$actualValue -ne [string]$expectedValue) {
            throw "$($Expected.id): expected $propertyName=$expectedValue, got $actualValue"
        }
        if ($null -eq $expectedValue -and $null -ne $actualValue) {
            throw "$($Expected.id): unexpected $propertyName=$actualValue"
        }
    }
    if ([string]$Actual.language -ne $Language) {
        throw "$($Expected.id): receipt language is $($Actual.language), expected $Language"
    }
}

function Get-ExpectedLanguageSummary {
    param([pscustomobject]$Language)
    $sites = @($Language.sites)
    return [pscustomobject]@{
        language = [string]$Language.language
        eligibleSiteCount = $sites.Count
        importSiteCount = @($sites | Where-Object capability -eq 'imports').Count
        exportSiteCount = @($sites | Where-Object capability -eq 'exports').Count
        internalRelationCount = @($sites | Where-Object outcome -eq 'internal').Count
        knownExternalCount = @($sites | Where-Object outcome -eq 'known_external').Count
        unresolvedCount = @($sites | Where-Object outcome -eq 'unresolved').Count
        ambiguousCount = @($sites | Where-Object outcome -eq 'ambiguous').Count
        invalidEvidenceCount = @($sites | Where-Object outcome -eq 'invalid_evidence').Count
    }
}

function Assert-LanguageSummary {
    param(
        [pscustomobject]$Expected,
        [pscustomobject]$Actual
    )
    foreach ($property in @(
        'eligibleSiteCount', 'importSiteCount', 'exportSiteCount', 'internalRelationCount',
        'knownExternalCount', 'unresolvedCount', 'ambiguousCount', 'invalidEvidenceCount'
    )) {
        if ([int64]$Actual.$property -ne [int64]$Expected.$property) {
            throw "$($Expected.language): expected $property=$($Expected.$property), got $($Actual.$property)"
        }
    }
    if ([int64]$Actual.inventoryFailedFileCount -ne 0 -or
        [int64]$Actual.metadataUnavailableFileCount -ne 0) {
        throw "$($Expected.language): import denominator is unknown because inventory or metadata files failed"
    }
}

function Copy-LargeFixture {
    param(
        [string]$SourceRoot,
        [object[]]$SourceFiles,
        [object[]]$Languages,
        [int]$MinimumBytes,
        [string]$DestinationRoot
    )
    New-Item -ItemType Directory -Force -Path $DestinationRoot | Out-Null
    foreach ($source in $SourceFiles) {
        $sourcePath = Join-Path $SourceRoot ([string]$source.path)
        $destination = Join-Path $DestinationRoot ([string]$source.path)
        $parent = Split-Path -Parent $destination
        New-Item -ItemType Directory -Force -Path $parent | Out-Null
        Copy-Item -LiteralPath $sourcePath -Destination $destination -Force
    }
    $largeFiles = @()
    foreach ($language in $Languages) {
        $path = [string]@($language.sites)[0].path
        $target = Join-Path $DestinationRoot $path
        $current = (Get-Item -LiteralPath $target).Length
        if ($current -lt $MinimumBytes) {
            $prefix = if ([string]$language.language -eq 'python') { "#" } else { "//" }
            $line = "$prefix import-ground-truth large-source padding`n"
            $needed = $MinimumBytes - $current
            $repeat = [Math]::Ceiling($needed / [Text.Encoding]::UTF8.GetByteCount($line)) + 1
            $padding = "`n" + ($line * $repeat)
            [IO.File]::AppendAllText($target, $padding, [Text.UTF8Encoding]::new($false))
        }
        $largeFiles += [pscustomobject]@{
            language = [string]$language.language
            path = $path
            byteSize = (Get-Item -LiteralPath $target).Length
        }
    }
    return $largeFiles
}

function Invoke-ImportAnalysis {
    param(
        [string]$FixtureRoot,
        [string]$RunRoot,
        [string]$PacksRoot
    )
    New-Item -ItemType Directory -Force -Path $RunRoot | Out-Null
    $outputPath = Join-Path $RunRoot 'index.json'
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
        throw "Import analysis failed with exit $exitCode`n$detail"
    }
    $lines = @($bridgeOutput | ForEach-Object { $_.ToString() })
    $receiptLine = $lines |
        Where-Object { $_.StartsWith('@codebase-workspace-language-ir ', [StringComparison]::Ordinal) } |
        Select-Object -First 1
    if ([string]::IsNullOrWhiteSpace($receiptLine)) {
        throw 'Import analysis emitted no Language IR receipt'
    }
    $receipt = $receiptLine.Substring('@codebase-workspace-language-ir '.Length) | ConvertFrom-Json
    $authority = Get-LanguageIrStreamAuthority -BridgeOutput $lines -Receipt $receipt -Context 'import analysis'
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
    throw "Import ground truth not found: $Truth"
}

$truthContract = Get-Content -Raw -LiteralPath $Truth | ConvertFrom-Json
if ($truthContract.schema -ne 'codebase-workspace.import-ground-truth.v1') {
    throw "Unsupported import ground-truth schema: $($truthContract.schema)"
}
if (@($truthContract.languages).Count -ne 10) {
    throw "Import ground truth must cover exactly 10 languages; got $(@($truthContract.languages).Count)"
}
$languageNames = @($truthContract.languages | ForEach-Object { [string]$_.language } | Sort-Object -Unique)
if ($languageNames.Count -ne 10) {
    throw 'Import ground truth contains duplicate language entries'
}
foreach ($language in $truthContract.languages) {
    if (@($language.sites | Where-Object outcome -eq 'internal').Count -eq 0) {
        throw "$($language.language): at least one reviewed internal import is required"
    }
    if (@($language.forbiddenSpecifiers).Count -eq 0) {
        throw "$($language.language): at least one reviewed negative import is required"
    }
}
$declaredAmbiguityLanguages = @(
    $truthContract.edgeCasePolicy.ambiguityLanguages | ForEach-Object { [string]$_ } |
        Sort-Object -Unique
)
$actualAmbiguityLanguages = @(
    $truthContract.languages | Where-Object {
        @($_.sites | Where-Object outcome -eq 'ambiguous').Count -gt 0
    } | ForEach-Object { [string]$_.language } | Sort-Object -Unique
)
if (($declaredAmbiguityLanguages -join "`n") -ne ($actualAmbiguityLanguages -join "`n")) {
    throw "Reviewed ambiguity languages do not match edgeCasePolicy: declared=$($declaredAmbiguityLanguages -join ',') actual=$($actualAmbiguityLanguages -join ',')"
}
foreach ($language in $truthContract.languages) {
    foreach ($site in @($language.sites | Where-Object outcome -eq 'ambiguous')) {
        if ([int64]$site.candidateCount -lt 2 -or [string]$site.gapCode -ne 'unresolved_target') {
            throw "$($site.id): ambiguous sites require candidateCount >= 2 and unresolved_target"
        }
    }
}
$declaredUnresolvedLanguages = @(
    $truthContract.edgeCasePolicy.unresolvedLanguages | ForEach-Object { [string]$_ } |
        Sort-Object -Unique
)
$actualUnresolvedLanguages = @(
    $truthContract.languages | Where-Object {
        @($_.sites | Where-Object outcome -eq 'unresolved').Count -gt 0
    } | ForEach-Object { [string]$_.language } | Sort-Object -Unique
)
if (($declaredUnresolvedLanguages -join "`n") -ne ($actualUnresolvedLanguages -join "`n")) {
    throw "Reviewed unresolved languages do not match edgeCasePolicy: declared=$($declaredUnresolvedLanguages -join ',') actual=$($actualUnresolvedLanguages -join ',')"
}

$fixturesRoot = (Resolve-Path $FixturesRoot).Path
$sourceFixtureRoot = (Resolve-Path (Join-Path $fixturesRoot ([string]$truthContract.fixture))).Path
Assert-PinnedFiles -Root $sourceFixtureRoot -SourceFiles @($truthContract.sourceFiles)
$packsRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$bundledProviders = Join-Path $packsRoot 'providers'
if ([string]::IsNullOrWhiteSpace($ProvidersRoot) -and (Test-Path -LiteralPath $bundledProviders)) {
    $ProvidersRoot = $bundledProviders
}

$temporaryFixtureRoot = Join-Path ([IO.Path]::GetTempPath()) (
    'codebase-workspace-import-fixture-' + [Guid]::NewGuid().ToString('N')
)
$copiedFiles = @(Copy-LargeFixture -SourceRoot $sourceFixtureRoot `
    -SourceFiles @($truthContract.sourceFiles) -Languages @($truthContract.languages) `
    -MinimumBytes $MinimumSourceBytes -DestinationRoot $temporaryFixtureRoot)
$largeFiles = if ($MinimumSourceBytes -gt 0) { $copiedFiles } else { @() }
$fixtureRoot = $temporaryFixtureRoot

New-Item -ItemType Directory -Force -Path $OutputRoot | Out-Null
$resolvedOutputRoot = [IO.Path]::GetFullPath($OutputRoot).TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
Get-ChildItem -LiteralPath $OutputRoot -Directory -Filter 'run-*' | ForEach-Object {
    $candidate = [IO.Path]::GetFullPath($_.FullName)
    if (-not $candidate.StartsWith($resolvedOutputRoot, [StringComparison]::OrdinalIgnoreCase) -or
        $_.Name -notmatch '^run-[0-9]+$') {
        throw "Refusing to clean unexpected import output directory: $candidate"
    }
    Remove-Item -LiteralPath $candidate -Recurse -Force
}

$sessionCacheRoot = Join-Path ([IO.Path]::GetTempPath()) (
    'codebase-workspace-import-quality-' + [Guid]::NewGuid().ToString('N')
)
New-Item -ItemType Directory -Force -Path $sessionCacheRoot | Out-Null
$hadPreviousCacheRoot = Test-Path Env:CODE_MEMORY_CACHE_ROOT
$previousCacheRoot = $env:CODE_MEMORY_CACHE_ROOT
$env:CODE_MEMORY_CACHE_ROOT = $sessionCacheRoot
$expectedByKey = @{}
$expectedSummaries = @{}
foreach ($language in $truthContract.languages) {
    $name = [string]$language.language
    $expectedSummaries[$name] = Get-ExpectedLanguageSummary -Language $language
    foreach ($site in $language.sites) {
        $key = Get-ExpectedKey -Language $name -Site $site
        if ($expectedByKey.ContainsKey($key)) { throw "Duplicate reviewed import key: $key" }
        $expectedByKey[$key] = [pscustomobject]@{ language = $name; site = $site }
    }
}

$runReports = @()
$languageRunDigests = @{}
try {
    for ($run = 1; $run -le $Runs; $run++) {
        Assert-PinnedFiles -Root $sourceFixtureRoot -SourceFiles @($truthContract.sourceFiles)
        $analysis = Invoke-ImportAnalysis -FixtureRoot $fixtureRoot `
            -RunRoot (Join-Path $OutputRoot "run-$run") -PacksRoot $packsRoot
        $receipt = $analysis.receipt
        if ($receipt.schema -ne 'codebase-workspace.language-ir-migration-receipt.v6') {
            throw "Run $run used unsupported Language IR receipt schema $($receipt.schema)"
        }
        if ($receipt.detailsTruncated) {
            throw "Run $run truncated import audit details; the ground-truth denominator is incomplete"
        }
        $actualEntries = @($receipt.importAuditSample)
        if ($actualEntries.Count -ne $expectedByKey.Count) {
            throw "Run $run expected $($expectedByKey.Count) import sites, got $($actualEntries.Count)"
        }
        $actualByKey = @{}
        foreach ($actual in $actualEntries) {
            $key = Get-SiteKey -Site $actual
            if ($actualByKey.ContainsKey($key)) { throw "Run $run emitted duplicate import key: $key" }
            $actualByKey[$key] = $actual
        }
        foreach ($key in $expectedByKey.Keys) {
            if (-not $actualByKey.ContainsKey($key)) { throw "Run $run missed reviewed import site: $key" }
            $expected = $expectedByKey[$key]
            Assert-SiteMatches -FixtureRoot $fixtureRoot -Language $expected.language `
                -Expected $expected.site -Actual $actualByKey[$key]
        }
        foreach ($language in $truthContract.languages) {
            foreach ($forbidden in @($language.forbiddenSpecifiers)) {
                if ($actualEntries | Where-Object {
                    $_.language -eq $language.language -and $_.specifier -eq $forbidden
                }) {
                    throw "Run $run emitted forbidden import specifier $($language.language)/$forbidden"
                }
            }
            $actualSummary = @($receipt.importLanguageSummaries | Where-Object language -eq $language.language)
            if ($actualSummary.Count -ne 1) {
                throw "Run $run expected one import summary for $($language.language), got $($actualSummary.Count)"
            }
            Assert-LanguageSummary -Expected $expectedSummaries[[string]$language.language] `
                -Actual $actualSummary[0]
            if (-not $languageRunDigests.ContainsKey([string]$language.language)) {
                $languageRunDigests[[string]$language.language] = @()
            }
            $languageRunDigests[[string]$language.language] += [string]$actualSummary[0].siteSetDigest
        }
        $runReports += [pscustomobject]@{
            run = $run
            elapsedMs = $analysis.elapsedMs
            sourceManifestDigest = [string]$receipt.sourceManifestDigest
            analysisPlanDigest = [string]$receipt.analysisPlanDigest
            streamSetDigest = [string]$receipt.streamSetDigest
            artifactContentDigest = [string]$analysis.authority.contentDigest
            semanticPayloadSetDigest = [string]$receipt.semanticPayloadSetDigest
            importSiteCount = $actualEntries.Count
        }
        Assert-PinnedFiles -Root $sourceFixtureRoot -SourceFiles @($truthContract.sourceFiles)
    }
}
finally {
    if ($hadPreviousCacheRoot) {
        $env:CODE_MEMORY_CACHE_ROOT = $previousCacheRoot
    }
    else {
        Remove-Item Env:CODE_MEMORY_CACHE_ROOT -ErrorAction SilentlyContinue
    }
    if ($null -ne $temporaryFixtureRoot -and (Test-Path -LiteralPath $temporaryFixtureRoot)) {
        Remove-Item -LiteralPath $temporaryFixtureRoot -Recurse -Force
    }
}

foreach ($field in @('sourceManifestDigest', 'analysisPlanDigest', 'streamSetDigest', 'artifactContentDigest', 'semanticPayloadSetDigest')) {
    if (@($runReports.$field | Sort-Object -Unique).Count -ne 1) {
        throw "Import analysis is non-deterministic across runs for $field"
    }
}
foreach ($language in $languageNames) {
    if (@($languageRunDigests[$language] | Sort-Object -Unique).Count -ne 1) {
        throw "$language import site digest is non-deterministic across runs"
    }
}

$languageReports = @()
foreach ($language in $languageNames) {
    $expected = $expectedSummaries[$language]
    $languageReports += [pscustomobject]@{
        language = $language
        eligibleSiteCount = $expected.eligibleSiteCount
        internalRelationCount = $expected.internalRelationCount
        knownExternalCount = $expected.knownExternalCount
        unresolvedCount = $expected.unresolvedCount
        ambiguousCount = $expected.ambiguousCount
        negativeSpecifierCount = @(
            ($truthContract.languages | Where-Object language -eq $language).forbiddenSpecifiers
        ).Count
        deterministic = $true
        releaseGatePassed = $true
    }
}
$aggregate = [pscustomobject]@{
    languageCount = $languageReports.Count
    reviewedSourceFileCount = @($truthContract.sourceFiles).Count
    expectedSiteCount = $expectedByKey.Count
    internalRelationCount = @($expectedByKey.Values | Where-Object { $_.site.outcome -eq 'internal' }).Count
    knownExternalCount = @($expectedByKey.Values | Where-Object { $_.site.outcome -eq 'known_external' }).Count
    unresolvedCount = @($expectedByKey.Values | Where-Object { $_.site.outcome -eq 'unresolved' }).Count
    ambiguousCount = @($expectedByKey.Values | Where-Object { $_.site.outcome -eq 'ambiguous' }).Count
    deterministicLanguageCount = $languageReports.Count
    releaseGatePassed = $true
}
$report = [ordered]@{
    schema = 'codebase-workspace.import-quality-report.v1'
    generatedAt = [DateTimeOffset]::Now.ToString('o')
    truthSchema = [string]$truthContract.schema
    truthSha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $Truth).Hash.ToLowerInvariant()
    fixtureFileSetDigest = Get-Sha256Text ((@($truthContract.sourceFiles) |
        Sort-Object path | ForEach-Object { "$($_.path)`t$($_.sha256)" }) -join "`n")
    runs = $Runs
    minimumSourceBytes = $MinimumSourceBytes
    aggregate = $aggregate
    languages = $languageReports
    largeFiles = $largeFiles
    runReports = $runReports
}
$reportPath = Join-Path $OutputRoot 'import-quality-report.json'
[IO.File]::WriteAllText(
    $reportPath,
    ($report | ConvertTo-Json -Depth 100),
    [Text.UTF8Encoding]::new($false)
)

$languageReports | Format-Table language,eligibleSiteCount,internalRelationCount,knownExternalCount,
    unresolvedCount,ambiguousCount,negativeSpecifierCount,deterministic -AutoSize
Write-Host ((
        "import quality: languages={0}/10 sites={1} internal={2} external={3} unresolved={4} " +
        "ambiguous={5} deterministic=100%"
    ) -f $aggregate.languageCount, $aggregate.expectedSiteCount, $aggregate.internalRelationCount,
    $aggregate.knownExternalCount, $aggregate.unresolvedCount, $aggregate.ambiguousCount)
Write-Host "report: $reportPath"

if (-not $AuditOnly -and -not $aggregate.releaseGatePassed) {
    throw 'Import quality release gate failed'
}
