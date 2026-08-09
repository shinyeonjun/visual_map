param(
    [string]$Bridge = (Join-Path $PSScriptRoot '..\..\rust\target\release\code-memory-language.exe'),
    [string]$ProvidersRoot,
    [string]$Truth = (Join-Path $PSScriptRoot '..\ground_truth\semantic-core.v2.json'),
    [string]$DefinitionTruth = (Join-Path $PSScriptRoot '..\ground_truth\definitions.v1.json'),
    [string]$OutputRoot = (Join-Path $PSScriptRoot '..\..\build\large-source-semantic'),
    [ValidateRange(1, 5)]
    [int]$Runs = 2,
    [ValidateRange(1000001, 16777216)]
    [int]$MinimumBytes = 1100000
)

$ErrorActionPreference = 'Stop'

function Assert-ChildPath {
    param([string]$Parent, [string]$Child)
    $parentPath = [IO.Path]::GetFullPath($Parent).TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
    $childPath = [IO.Path]::GetFullPath($Child)
    if (-not $childPath.StartsWith($parentPath, [StringComparison]::OrdinalIgnoreCase)) {
        throw "Generated path escapes the large-source output root: $childPath"
    }
    return $childPath
}

function Get-RelativeRepositoryPath {
    param([string]$RepositoryRoot, [string]$Target)
    $base = [IO.Path]::GetFullPath($RepositoryRoot).TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
    $baseUri = [Uri]$base
    $targetUri = [Uri][IO.Path]::GetFullPath($Target)
    return [Uri]::UnescapeDataString($baseUri.MakeRelativeUri($targetUri).ToString()).TrimEnd('/')
}

function Add-LargeCommentPayload {
    param(
        [string]$Path,
        [string]$Language,
        [int]$TargetBytes
    )
    $prefix = if ($Language -eq 'python') { '# ' } else { '// ' }
    $utf8 = [Text.UTF8Encoding]::new($false)
    $writer = [IO.StreamWriter]::new($Path, $true, $utf8, 65536)
    try {
        $line = $prefix + 'large-source semantic gate keeps provider-visible code deterministic ' + ('x' * 896)
        while (([IO.FileInfo]$Path).Length -lt $TargetBytes) {
            $writer.WriteLine($line)
            $writer.Flush()
        }
    }
    finally {
        $writer.Dispose()
    }
}

$codeMemoryRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$repositoryRoot = Split-Path $codeMemoryRoot -Parent
$sourceFixturesRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\fixtures')).Path
$semanticGate = (Resolve-Path (Join-Path $PSScriptRoot 'run-semantic-ground-truth-gate.ps1')).Path
$definitionGate = (Resolve-Path (Join-Path $PSScriptRoot 'run-definition-ground-truth-gate.ps1')).Path
$truthPath = (Resolve-Path $Truth).Path
$definitionTruthPath = (Resolve-Path $DefinitionTruth).Path
$outputPath = [IO.Path]::GetFullPath($OutputRoot)
New-Item -ItemType Directory -Force -Path $outputPath | Out-Null
$generatedFixturesRoot = Assert-ChildPath $outputPath (Join-Path $outputPath 'fixtures')
$semanticOutputRoot = Assert-ChildPath $outputPath (Join-Path $outputPath 'semantic')
$definitionFixturesRoot = Assert-ChildPath $outputPath (Join-Path $outputPath 'definition-fixtures')
$definitionOutputRoot = Assert-ChildPath $outputPath (Join-Path $outputPath 'definitions')
foreach ($generatedPath in @($generatedFixturesRoot, $semanticOutputRoot, $definitionFixturesRoot, $definitionOutputRoot)) {
    if (Test-Path -LiteralPath $generatedPath) {
        Remove-Item -LiteralPath $generatedPath -Recurse -Force
    }
    New-Item -ItemType Directory -Force -Path $generatedPath | Out-Null
}

$contract = Get-Content -Raw -LiteralPath $truthPath | ConvertFrom-Json
if ($contract.schema -ne 'codebase-workspace.semantic-ground-truth.v2') {
    throw "Unsupported semantic truth schema: $($contract.schema)"
}
$largeFiles = @()
foreach ($case in $contract.cases) {
    $sourceFixture = (Resolve-Path (Join-Path $sourceFixturesRoot $case.fixture)).Path
    $targetFixture = Assert-ChildPath $generatedFixturesRoot (Join-Path $generatedFixturesRoot $case.id)
    New-Item -ItemType Directory -Force -Path $targetFixture | Out-Null
    Get-ChildItem -LiteralPath $sourceFixture -Force |
        Copy-Item -Destination $targetFixture -Recurse -Force

    $largeSource = $case.sourceFiles | Select-Object -First 1
    $largeSourcePath = Assert-ChildPath $targetFixture (Join-Path $targetFixture $largeSource.path)
    Add-LargeCommentPayload -Path $largeSourcePath -Language $case.id -TargetBytes $MinimumBytes
    $largeLength = ([IO.FileInfo]$largeSourcePath).Length
    if ($largeLength -lt $MinimumBytes) {
        throw "$($case.id): large-source fixture did not reach $MinimumBytes bytes"
    }

    foreach ($source in $case.sourceFiles) {
        $path = Assert-ChildPath $targetFixture (Join-Path $targetFixture $source.path)
        $source.sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $path).Hash.ToLowerInvariant()
    }
    $case.fixture = $case.id
    if ($case.id -in @('typescript', 'javascript')) {
        $relativeFixture = Get-RelativeRepositoryPath $repositoryRoot $targetFixture
        $case.projectTargetPattern = [regex]::Escape($relativeFixture + '/')
    }
    $largeFiles += [pscustomobject]@{
        language = $case.id
        path = [string]$largeSource.path
        byteSize = $largeLength
        sha256 = [string]$largeSource.sha256
    }
}
$contract.review.reviewedAt = (Get-Date).ToString('yyyy-MM-dd')
$contract.review.scope = 'Original reviewed CALLS/CONSTRUCTS corpus with one provider-visible source per language expanded beyond one megabyte using comment-only payloads'
$generatedTruth = Assert-ChildPath $outputPath (Join-Path $outputPath 'semantic-core.large.v2.json')
$json = $contract | ConvertTo-Json -Depth 100
[IO.File]::WriteAllText($generatedTruth, $json, [Text.UTF8Encoding]::new($false))

$gateParameters = @{
    Bridge = $Bridge
    Truth = $generatedTruth
    FixturesRoot = $generatedFixturesRoot
    OutputRoot = $semanticOutputRoot
    Runs = $Runs
}
if (-not [string]::IsNullOrWhiteSpace($ProvidersRoot)) {
    $gateParameters.ProvidersRoot = $ProvidersRoot
}
& $semanticGate @gateParameters
if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
}

$semanticReportPath = Join-Path $semanticOutputRoot 'semantic-quality-report.json'
$semanticReport = Get-Content -Raw -LiteralPath $semanticReportPath | ConvertFrom-Json

$definitionContract = Get-Content -Raw -LiteralPath $definitionTruthPath | ConvertFrom-Json
if ($definitionContract.schema -ne 'codebase-workspace.definition-ground-truth.v1') {
    throw "Unsupported definition truth schema: $($definitionContract.schema)"
}
foreach ($project in $definitionContract.projects) {
    $sourceFixture = (Resolve-Path (Join-Path $sourceFixturesRoot $project.fixture)).Path
    $targetFixture = Assert-ChildPath $definitionFixturesRoot (Join-Path $definitionFixturesRoot $project.id)
    New-Item -ItemType Directory -Force -Path $targetFixture | Out-Null
    Get-ChildItem -LiteralPath $sourceFixture -Force |
        Copy-Item -Destination $targetFixture -Recurse -Force

    foreach ($language in $project.languages) {
        $largeSource = $largeFiles | Where-Object language -eq $language.language | Select-Object -First 1
        if ($null -eq $largeSource) {
            throw "$($project.id): no large-source target is registered for $($language.language)"
        }
        $largeSourcePath = Assert-ChildPath $targetFixture (Join-Path $targetFixture $largeSource.path)
        Add-LargeCommentPayload -Path $largeSourcePath -Language $language.language -TargetBytes $MinimumBytes
    }
    foreach ($source in $project.sourceFiles) {
        $path = Assert-ChildPath $targetFixture (Join-Path $targetFixture $source.path)
        $source.sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $path).Hash.ToLowerInvariant()
    }
    $project.fixture = $project.id
}
$definitionContract.review.reviewedAt = (Get-Date).ToString('yyyy-MM-dd')
$definitionContract.review.scope = 'Original reviewed definition corpus with one provider-visible source per language expanded beyond one megabyte using comment-only payloads'
$generatedDefinitionTruth = Assert-ChildPath $outputPath (Join-Path $outputPath 'definitions.large.v1.json')
[IO.File]::WriteAllText(
    $generatedDefinitionTruth,
    ($definitionContract | ConvertTo-Json -Depth 100),
    [Text.UTF8Encoding]::new($false)
)
$definitionGateParameters = @{
    Bridge = $Bridge
    Truth = $generatedDefinitionTruth
    FixturesRoot = $definitionFixturesRoot
    OutputRoot = $definitionOutputRoot
    Runs = $Runs
}
if (-not [string]::IsNullOrWhiteSpace($ProvidersRoot)) {
    $definitionGateParameters.ProvidersRoot = $ProvidersRoot
}
& $definitionGate @definitionGateParameters
if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
}
$definitionReportPath = Join-Path $definitionOutputRoot 'definition-quality-report.json'
$definitionReport = Get-Content -Raw -LiteralPath $definitionReportPath | ConvertFrom-Json

$report = [ordered]@{
    schema = 'codebase-workspace.large-source-semantic-report.v1'
    generatedAt = (Get-Date).ToString('o')
    minimumBytes = $MinimumBytes
    runs = $Runs
    largeFiles = $largeFiles
    semanticAggregate = $semanticReport.aggregate
    definitionAggregate = $definitionReport.aggregate
    releaseGatePassed = [bool]$semanticReport.aggregate.releaseGatePassed -and
        [bool]$definitionReport.aggregate.releaseGatePassed
}
$reportPath = Join-Path $outputPath 'large-source-semantic-report.json'
[IO.File]::WriteAllText(
    $reportPath,
    ($report | ConvertTo-Json -Depth 100),
    [Text.UTF8Encoding]::new($false)
)
Write-Host "large-source semantic report: $reportPath"
if (-not $report.releaseGatePassed) {
    exit 1
}
