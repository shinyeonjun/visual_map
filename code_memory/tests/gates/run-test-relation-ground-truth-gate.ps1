param(
    [string]$Bridge = (Join-Path $PSScriptRoot '..\..\rust\target\release\code-memory-language.exe'),
    [string]$ProvidersRoot = '',
    [string]$Truth = (Join-Path $PSScriptRoot '..\ground_truth\test-relations.v1.json'),
    [string]$FixturesRoot = (Join-Path $PSScriptRoot '..\fixtures\test-relations'),
    [string]$OutputRoot = (Join-Path $PSScriptRoot '..\..\build\test-relation-ground-truth'),
    [string]$Python = '',
    [ValidateRange(2, 5)]
    [int]$Runs = 2
)

. (Join-Path $PSScriptRoot 'lib\language-ir-stream-authority.ps1')

$ErrorActionPreference = 'Stop'
if (Get-Variable PSNativeCommandUseErrorActionPreference -ErrorAction SilentlyContinue) {
    $PSNativeCommandUseErrorActionPreference = $false
}

$bundleInspectionProgram = @'
import json
import sqlite3
import sys

connection = sqlite3.connect(sys.argv[1])
nodes = {
    row[0]: json.loads(row[1])
    for row in connection.execute("SELECT id, payload_json FROM nodes")
}
evidence = {
    row[0]: json.loads(row[1])
    for row in connection.execute("SELECT id, payload_json FROM evidence")
}

def parent_path(node):
    current = node
    seen = set()
    while current and current.get("id") not in seen:
        seen.add(current.get("id"))
        parent = nodes.get(current.get("parentId"))
        if not parent:
            return None
        if parent.get("kind") == "file":
            return parent.get("qualifiedName")
        current = parent
    return None

test_cases = []
for node in nodes.values():
    if node.get("kind") == "test_case":
        test_cases.append({
            "id": node["id"],
            "name": node.get("displayName"),
            "path": parent_path(node),
            "language": node.get("language"),
            "testFlag": node.get("flags", {}).get("test"),
        })
test_cases.sort(key=lambda item: (item.get("path") or "", item.get("name") or "", item["id"]))

test_edges = []
for row in connection.execute("SELECT payload_json FROM edges WHERE kind = 'tests'"):
    edge = json.loads(row[0])
    source = nodes[edge["sourceId"]]
    target = nodes[edge["targetId"]]
    edge_evidence = []
    for evidence_id in edge.get("evidenceIds", []):
        item = evidence.get(evidence_id)
        if not item:
            continue
        span = item.get("location", {}).get("span", {})
        start = span.get("start", {})
        edge_evidence.append({
            "kind": item.get("kind"),
            "path": span.get("path"),
            "line": start.get("line"),
            "column": start.get("utf8Column"),
        })
    edge_evidence.sort(key=lambda item: (
        item.get("kind") or "",
        item.get("path") or "",
        -1 if item.get("line") is None else item["line"],
        -1 if item.get("column") is None else item["column"],
    ))
    test_edges.append({
        "sourceName": source.get("displayName"),
        "sourcePath": parent_path(source),
        "sourceTestFlag": source.get("flags", {}).get("test"),
        "targetName": target.get("displayName"),
        "targetPath": parent_path(target),
        "targetTestFlag": target.get("flags", {}).get("test"),
        "targetExternalFlag": target.get("flags", {}).get("external"),
        "family": edge.get("family"),
        "kind": edge.get("kind"),
        "truth": edge.get("truth"),
        "resolution": edge.get("resolution"),
        "evidence": edge_evidence,
    })
test_edges.sort(key=lambda item: (
    item.get("sourcePath") or "",
    item.get("sourceName") or "",
    item.get("targetPath") or "",
    item.get("targetName") or "",
))

test_gaps = []
for row in connection.execute("SELECT payload_json FROM gaps"):
    gap = json.loads(row[0])
    if gap.get("capability") == "test_relations":
        scope = gap.get("scope", {})
        test_gaps.append({
            "code": gap.get("code"),
            "path": scope.get("path"),
            "message": gap.get("message"),
            "evidenceCount": len(gap.get("evidenceIds", [])),
        })
test_gaps.sort(key=lambda item: (item.get("path") or "", item.get("message") or ""))

print(json.dumps({
    "testCases": test_cases,
    "testEdges": test_edges,
    "testGaps": test_gaps,
}, separators=(",", ":")))
'@

function Get-ReviewedFileState {
    param(
        [string]$Root,
        [object[]]$SourceFiles
    )

    $state = [ordered]@{}
    foreach ($source in @($SourceFiles | Sort-Object path)) {
        $path = Join-Path $Root ([string]$source.path -replace '/', '\')
        if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
            throw "Reviewed source is missing: $path"
        }
        $state[[string]$source.path] = (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant()
    }
    return $state
}

function Assert-ReviewedFileState {
    param(
        [string]$ProjectId,
        [System.Collections.IDictionary]$State,
        [object[]]$SourceFiles
    )

    if ($State.Count -ne @($SourceFiles).Count) {
        throw "${ProjectId}: reviewed file accounting changed"
    }
    foreach ($source in $SourceFiles) {
        if (-not $State.Contains([string]$source.path)) {
            throw "${ProjectId}: reviewed source is missing: $($source.path)"
        }
        if ([string]$State[[string]$source.path] -cne [string]$source.sha256) {
            throw "${ProjectId}: source hash changed for $($source.path)"
        }
    }
}

function Copy-ReviewedProject {
    param(
        [pscustomobject]$Project,
        [string]$FixtureRoot,
        [string]$ProjectRoot,
        [string]$ApprovedOutputRoot
    )

    $fullProjectRoot = [IO.Path]::GetFullPath($ProjectRoot)
    $approvedPrefix = [IO.Path]::GetFullPath($ApprovedOutputRoot).TrimEnd('\') + '\'
    if (-not $fullProjectRoot.StartsWith($approvedPrefix, [StringComparison]::OrdinalIgnoreCase)) {
        throw "$($Project.id): refusing to replace project outside the approved output root"
    }
    if (Test-Path -LiteralPath $fullProjectRoot) {
        Remove-Item -LiteralPath $fullProjectRoot -Recurse -Force
    }
    New-Item -ItemType Directory -Force -Path $fullProjectRoot | Out-Null
    foreach ($source in $Project.sourceFiles) {
        $sourcePath = Join-Path $FixtureRoot ([string]$source.path -replace '/', '\')
        $targetPath = Join-Path $fullProjectRoot ([string]$source.path -replace '/', '\')
        New-Item -ItemType Directory -Force -Path (Split-Path $targetPath) | Out-Null
        Copy-Item -LiteralPath $sourcePath -Destination $targetPath
    }
    if ($Project.language -eq 'dart') {
        $dartTool = Join-Path $fullProjectRoot '.dart_tool'
        New-Item -ItemType Directory -Force -Path $dartTool | Out-Null
        [IO.File]::WriteAllText(
            (Join-Path $dartTool 'package_config.json'),
            '{"configVersion":2,"packages":[]}',
            [Text.UTF8Encoding]::new($false)
        )
    }
    return $fullProjectRoot
}

function Get-BundleInspection {
    param(
        [string]$BundlePath,
        [string]$Context
    )

    $previousErrorActionPreference = $ErrorActionPreference
    try {
        $ErrorActionPreference = 'Continue'
        $inspectionOutput = @($bundleInspectionProgram | & $Python - $BundlePath 2>&1)
        $inspectionExitCode = $LASTEXITCODE
    }
    finally {
        $ErrorActionPreference = $previousErrorActionPreference
    }
    if ($inspectionExitCode -ne 0) {
        $detail = ($inspectionOutput | ForEach-Object { $_.ToString() }) -join "`n"
        throw "${Context}: canonical bundle inspection failed`n$detail"
    }
    return (($inspectionOutput | ForEach-Object { $_.ToString() }) -join '') | ConvertFrom-Json
}

function Test-EvidenceMatch {
    param(
        [object[]]$Evidence,
        [string]$Kind,
        [string]$Path,
        [pscustomobject]$Position
    )

    return $null -ne ($Evidence | Where-Object {
            $_.kind -ceq $Kind -and
            $_.path -ceq $Path -and
            [int64]$_.line -eq [int64]$Position.line -and
            [int64]$_.column -eq [int64]$Position.column
        } | Select-Object -First 1)
}

function Assert-TestResult {
    param(
        [pscustomobject]$Project,
        [pscustomobject]$TestReceipt,
        [pscustomobject]$CanonicalReceipt,
        [pscustomobject]$Inspection
    )

    $context = $Project.id
    $expected = $Project.expected
    if ($TestReceipt.schema -ne 'codebase-workspace.test-ir.v1') {
        throw "${context}: unsupported Test IR schema $($TestReceipt.schema)"
    }
    if ($CanonicalReceipt.schema -ne 'codebase-workspace.canonical-linker-receipt.v2') {
        throw "${context}: unsupported canonical linker schema $($CanonicalReceipt.schema)"
    }
    if ([string]$TestReceipt.snapshotId -cne [string]$CanonicalReceipt.snapshotId -or
        [string]$TestReceipt.contentDigest -cne [string]$CanonicalReceipt.testIrContentDigest) {
        throw "${context}: Test IR was not consumed as the canonical test authority"
    }
    $testCounts = @{
        detectedTestCaseCount = 2
        linkedTestCaseCount = 1
        emittedRelationCount = 1
        inventoryFailedFileCount = 0
        gapCount = 1
    }
    foreach ($entry in $testCounts.GetEnumerator()) {
        if ([int64]$TestReceipt.PSObject.Properties[$entry.Key].Value -ne [int64]$entry.Value) {
            throw "${context}: $($entry.Key) expected $($entry.Value), got $($TestReceipt.PSObject.Properties[$entry.Key].Value)"
        }
    }
    $canonicalCounts = @{
        testCaseNodeCount = 2
        testsEdgeCount = 1
        unlinkedTestCaseCount = 1
        danglingEndpointCount = 0
        confirmedWithoutEvidenceCount = 0
        duplicateLogicalEdgeCount = 0
    }
    foreach ($entry in $canonicalCounts.GetEnumerator()) {
        if ([int64]$CanonicalReceipt.PSObject.Properties[$entry.Key].Value -ne [int64]$entry.Value) {
            throw "${context}: canonical $($entry.Key) expected $($entry.Value), got $($CanonicalReceipt.PSObject.Properties[$entry.Key].Value)"
        }
    }

    $cases = @($Inspection.testCases)
    if ($cases.Count -ne 2) {
        throw "${context}: canonical bundle expected two TestCase nodes, got $($cases.Count)"
    }
    foreach ($name in @([string]$expected.positiveTestName, [string]$expected.unlinkedTestName)) {
        $matches = @($cases | Where-Object {
                $_.name -ceq $name -and $_.path -ceq [string]$expected.testPath -and $_.testFlag -eq $true
            })
        if ($matches.Count -ne 1) {
            throw "${context}: canonical TestCase '$name' is missing or ambiguous"
        }
    }

    $edges = @($Inspection.testEdges)
    if ($edges.Count -ne 1) {
        throw "${context}: canonical bundle expected one Tests edge, got $($edges.Count)"
    }
    $edge = $edges[0]
    if ($edge.sourceName -cne [string]$expected.positiveTestName -or
        $edge.sourcePath -cne [string]$expected.testPath -or
        $edge.targetName -cne [string]$expected.targetName -or
        $edge.targetPath -cne [string]$expected.targetPath -or
        $edge.sourceTestFlag -ne $true -or
        $edge.targetTestFlag -ne $false -or
        $edge.targetExternalFlag -ne $false -or
        $edge.family -cne 'verification' -or
        $edge.kind -cne 'tests' -or
        $edge.truth -cne 'confirmed' -or
        $edge.resolution -cne 'provider') {
        throw "${context}: canonical Tests edge does not match the reviewed production target"
    }
    if ($edge.sourceName -ceq [string]$expected.unlinkedTestName) {
        throw "${context}: name-only negative test was promoted to a confirmed edge"
    }
    $edgeEvidence = @($edge.evidence)
    if (-not (Test-EvidenceMatch $edgeEvidence 'framework_registration' ([string]$expected.testPath) $expected.marker)) {
        throw "${context}: exact test registration evidence is missing"
    }
    if (-not (Test-EvidenceMatch $edgeEvidence 'call_site' ([string]$expected.testPath) $expected.call)) {
        throw "${context}: exact production call evidence is missing"
    }

    $gaps = @($Inspection.testGaps)
    if ($gaps.Count -ne 1 -or
        $gaps[0].code -cne 'unresolved_target' -or
        $gaps[0].path -cne [string]$expected.testPath -or
        [int64]$gaps[0].evidenceCount -lt 1 -or
        -not ([string]$gaps[0].message).Contains([string]$expected.unlinkedTestName, [StringComparison]::Ordinal)) {
        throw "${context}: unlinked test was not preserved as one evidence-backed static gap"
    }
}

if (-not (Test-Path -LiteralPath $Bridge -PathType Leaf)) {
    throw "Bridge not found: $Bridge"
}
if (-not (Test-Path -LiteralPath $Truth -PathType Leaf)) {
    throw "Test-relation ground truth not found: $Truth"
}
if ([string]::IsNullOrWhiteSpace($Python)) {
    $pythonCommand = Get-Command python -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($null -eq $pythonCommand) {
        throw 'Python is required only to inspect the immutable SQLite test artifact for this gate'
    }
    $Python = $pythonCommand.Source
}
$Bridge = (Resolve-Path $Bridge).Path
$Python = (Resolve-Path $Python).Path
$FixturesRoot = (Resolve-Path $FixturesRoot).Path
$packsRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$bundledProviders = Join-Path $packsRoot 'providers'
if ([string]::IsNullOrWhiteSpace($ProvidersRoot) -and (Test-Path -LiteralPath $bundledProviders)) {
    $ProvidersRoot = $bundledProviders
}
if (-not [string]::IsNullOrWhiteSpace($ProvidersRoot)) {
    $ProvidersRoot = (Resolve-Path $ProvidersRoot).Path
}
$OutputRoot = [IO.Path]::GetFullPath($OutputRoot)
New-Item -ItemType Directory -Force -Path $OutputRoot | Out-Null

$contract = Get-Content -Raw -LiteralPath $Truth | ConvertFrom-Json
if ($contract.schema -ne 'codebase-workspace.test-relation-ground-truth.v1') {
    throw "Unsupported test-relation ground-truth schema: $($contract.schema)"
}
$requiredLanguages = @('c', 'cpp', 'csharp', 'dart', 'go', 'java', 'javascript', 'python', 'rust', 'typescript')
$actualLanguages = @($contract.projects | ForEach-Object { [string]$_.language } | Sort-Object -Unique)
if (@($contract.projects).Count -ne 10 -or $actualLanguages.Count -ne 10 -or
    (Compare-Object $requiredLanguages $actualLanguages)) {
    throw 'Test-relation ground truth must cover the exact ten-language code contract'
}

$sourceStates = @{}
$projectRoots = @{}
foreach ($project in $contract.projects) {
    $fixtureRoot = (Resolve-Path (Join-Path $FixturesRoot $project.fixture)).Path
    $sourceState = Get-ReviewedFileState $fixtureRoot $project.sourceFiles
    Assert-ReviewedFileState $project.id $sourceState $project.sourceFiles
    $sourceStates[$project.id] = $sourceState
    $projectRoot = Join-Path $OutputRoot "projects\$($project.id)"
    $projectRoots[$project.id] = Copy-ReviewedProject $project $fixtureRoot $projectRoot $OutputRoot
}

$runResults = @()
for ($run = 1; $run -le $Runs; $run++) {
    $runRoot = Join-Path $OutputRoot "run-$run"
    New-Item -ItemType Directory -Force -Path $runRoot | Out-Null
    foreach ($project in $contract.projects) {
        $projectRoot = [string]$projectRoots[$project.id]
        $outputPath = Join-Path $runRoot "$($project.id).json"
        $arguments = @('index', '--root', $projectRoot, '--out', $outputPath, '--packs-root', $packsRoot)
        if (-not [string]::IsNullOrWhiteSpace($ProvidersRoot)) {
            $arguments += @('--providers-root', $ProvidersRoot)
        }
        $stopwatch = [Diagnostics.Stopwatch]::StartNew()
        $previousErrorActionPreference = $ErrorActionPreference
        try {
            $ErrorActionPreference = 'Continue'
            $bridgeOutput = @(& $Bridge @arguments 2>&1)
            $bridgeExitCode = $LASTEXITCODE
        }
        finally {
            $ErrorActionPreference = $previousErrorActionPreference
        }
        $stopwatch.Stop()
        if ($bridgeExitCode -ne 0 -or -not (Test-Path -LiteralPath $outputPath -PathType Leaf)) {
            $detail = ($bridgeOutput | Select-Object -Last 80 | ForEach-Object { $_.ToString() }) -join "`n"
            throw "$($project.id): provider analysis failed with exit $bridgeExitCode`n$detail"
        }

        $index = Get-Content -Raw -LiteralPath $outputPath | ConvertFrom-Json
        $language = @($index.languages | Where-Object id -eq $project.language) | Select-Object -First 1
        if ($null -eq $language -or -not ([string]$language.status).StartsWith('indexed', [StringComparison]::Ordinal)) {
            throw "$($project.id): reviewed language was not indexed"
        }
        $languageReceipt = Get-TaggedJsonReceipt -BridgeOutput $bridgeOutput `
            -Prefix '@codebase-workspace-language-ir ' -Context $project.id
        if ($languageReceipt.schema -ne 'codebase-workspace.language-ir-migration-receipt.v6' -or
            $languageReceipt.detailsTruncated -eq $true) {
            throw "$($project.id): Language IR receipt is unsupported or truncated"
        }
        $authority = Get-LanguageIrStreamAuthority -BridgeOutput $bridgeOutput `
            -Receipt $languageReceipt -Context $project.id
        $testReceipt = Get-TaggedJsonReceipt -BridgeOutput $bridgeOutput `
            -Prefix '@codebase-workspace-test-ir ' -Context $project.id
        $canonicalReceipt = Get-TaggedJsonReceipt -BridgeOutput $bridgeOutput `
            -Prefix '@codebase-workspace-canonical-linker ' -Context $project.id
        $artifact = Get-TaggedJsonReceipt -BridgeOutput $bridgeOutput `
            -Prefix '@codebase-workspace-canonical-fact-bundle ' -Context $project.id
        $inspection = Get-BundleInspection ([string]$artifact.bundlePath) $project.id
        Assert-TestResult $project $testReceipt $canonicalReceipt $inspection

        $currentState = Get-ReviewedFileState $projectRoot $project.sourceFiles
        Assert-ReviewedFileState $project.id $currentState $project.sourceFiles
        $beforeJson = $sourceStates[$project.id] | ConvertTo-Json -Compress
        $afterJson = $currentState | ConvertTo-Json -Compress
        if ($beforeJson -cne $afterJson) {
            throw "$($project.id): provider analysis mutated reviewed source"
        }

        $runResults += [pscustomobject]@{
            run = $run
            project = [string]$project.id
            language = [string]$project.language
            elapsedMs = $stopwatch.ElapsedMilliseconds
            snapshotId = [string]$testReceipt.snapshotId
            sourceManifestDigest = [string]$languageReceipt.sourceManifestDigest
            analysisPlanDigest = [string]$languageReceipt.analysisPlanDigest
            languageIrContentDigest = [string]$authority.contentDigest
            testIrContentDigest = [string]$testReceipt.contentDigest
            canonicalSemanticDigest = [string]$artifact.semanticDigest
            canonicalBundleDigest = [string]$artifact.bundleDigest
        }
        Write-Host "PASS run=$run $($project.id): cases=2 linked=1 static-gap=1"
    }
}

foreach ($project in $contract.projects) {
    $results = @($runResults | Where-Object project -eq $project.id | Sort-Object run)
    $baseline = $results[0]
    foreach ($candidate in $results | Select-Object -Skip 1) {
        foreach ($field in @(
                'snapshotId',
                'sourceManifestDigest',
                'analysisPlanDigest',
                'languageIrContentDigest',
                'testIrContentDigest',
                'canonicalSemanticDigest',
                'canonicalBundleDigest'
            )) {
            if ($candidate.$field -cne $baseline.$field) {
                throw "$($project.id): non-deterministic $field between run 1 and run $($candidate.run)"
            }
        }
    }
}

$report = [ordered]@{
    schema = 'codebase-workspace.test-relation-quality-report.v1'
    reviewedProjectCount = 10
    reviewedLanguageCount = 10
    reviewedPositiveCount = 10
    reviewedNegativeCount = 10
    truePositive = 10
    falsePositive = 0
    falseNegative = 0
    trueNegative = 10
    precision = 1.0
    recall = 1.0
    f1 = 1.0
    exactEvidence = 1.0
    deterministicRuns = $Runs
    sourceMutationCount = 0
    confirmedTestsEdgeCount = 10
    staticGapCount = 10
    aiCandidateCount = 0
    runs = $runResults
}
$reportPath = Join-Path $OutputRoot 'test-relation-quality-report.json'
$report | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $reportPath -Encoding utf8

Write-Host "Test relation ground-truth gate passed: 10/10 languages, 10/10 exact confirmed relations, 10/10 name-only negatives rejected, $Runs deterministic runs."
Write-Host "Static gaps reserved for later AI review: 10; AI-confirmed facts: 0."
Write-Host "Report: $reportPath"
