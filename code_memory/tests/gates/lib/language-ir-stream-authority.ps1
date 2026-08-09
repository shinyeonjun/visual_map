function Get-TaggedJsonReceipt {
    param(
        [object[]]$BridgeOutput,
        [string]$Prefix,
        [string]$Context
    )

    $line = @($BridgeOutput | ForEach-Object { $_.ToString() } | Where-Object {
            $_.StartsWith($Prefix, [StringComparison]::Ordinal)
        }) | Select-Object -First 1
    if ([string]::IsNullOrWhiteSpace($line)) {
        throw "$Context`: required receipt is missing: $Prefix"
    }
    return $line.Substring($Prefix.Length) | ConvertFrom-Json
}

function Assert-Sha256Digest {
    param(
        [string]$Value,
        [string]$Context
    )
    if ($Value -notmatch '^[0-9a-f]{64}$') {
        throw "$Context`: invalid SHA-256 digest"
    }
}

function Get-CanonicalFactBundleAuthority {
    param(
        [object[]]$BridgeOutput,
        [pscustomobject]$Receipt,
        [pscustomobject]$LanguageIrAuthority,
        [string]$Context
    )

    $linker = Get-TaggedJsonReceipt -BridgeOutput $BridgeOutput `
        -Prefix '@codebase-workspace-canonical-linker ' -Context $Context
    $manifest = Get-TaggedJsonReceipt -BridgeOutput $BridgeOutput `
        -Prefix '@codebase-workspace-canonical-fact-manifest ' -Context $Context
    $artifact = Get-TaggedJsonReceipt -BridgeOutput $BridgeOutput `
        -Prefix '@codebase-workspace-canonical-fact-bundle ' -Context $Context

    if ($linker.schema -ne 'codebase-workspace.canonical-linker-receipt.v2' -or
        $manifest.schema -ne 'codebase-workspace.canonical-fact.v1' -or
        $artifact.schema -ne 'codebase-workspace.canonical-fact-bundle-artifact.v1') {
        throw "$Context`: unsupported canonical bundle receipt schema"
    }
    foreach ($value in @($linker.snapshotId, $manifest.snapshotId, $artifact.snapshotId)) {
        if ([string]$value -cne [string]$Receipt.snapshotId) {
            throw "$Context`: canonical snapshot does not match Language IR"
        }
    }
    if ([string]$linker.languageIrContentDigest -cne [string]$LanguageIrAuthority.contentDigest -or
        [int64]$linker.languageIrRecordCount -ne [int64]$LanguageIrAuthority.recordCount) {
        throw "$Context`: canonical linker did not consume the authoritative Language IR artifact"
    }
    if ([int64]$linker.providerDefinitionIdentityCount -ne [int64]$Receipt.definitionCount) {
        throw "$Context`: canonical linker definition accounting differs from Language IR"
    }
    if ([int64]$linker.canonicalDefinitionNodeCount -gt [int64]$linker.providerDefinitionIdentityCount -or
        [int64]$linker.retainedDefinitionNodeCount -gt [int64]$linker.canonicalDefinitionNodeCount -or
        [int64]$linker.prunedDefinitionNodeCount -ne
        ([int64]$linker.canonicalDefinitionNodeCount - [int64]$linker.retainedDefinitionNodeCount)) {
        throw "$Context`: canonical definition relevance accounting is inconsistent"
    }
    if (([int64]$linker.resolvedRelationCount + [int64]$linker.unresolvedRelationCount) -ne
        [int64]$Receipt.relationCount) {
        throw "$Context`: canonical relation accounting differs from Language IR"
    }
    if ([int64]$linker.danglingEndpointCount -ne 0 -or
        [int64]$linker.confirmedWithoutEvidenceCount -ne 0 -or
        [int64]$linker.duplicateLogicalEdgeCount -ne 0) {
        throw "$Context`: canonical graph invariants failed"
    }
    Assert-Sha256Digest -Value ([string]$linker.semanticDigest) -Context "$Context canonical semantic digest"
    Assert-Sha256Digest -Value ([string]$manifest.bundleDigest) -Context "$Context canonical bundle digest"
    if ([string]$linker.semanticDigest -cne [string]$manifest.semanticDigest -or
        [string]$manifest.semanticDigest -cne [string]$artifact.semanticDigest -or
        [string]$manifest.bundleDigest -cne [string]$artifact.bundleDigest) {
        throw "$Context`: canonical receipt, manifest, and artifact identities disagree"
    }
    if ([int64]$manifest.analysisUnitReceiptCount -ne [int64]$Receipt.emittedUnitCount) {
        throw "$Context`: canonical analysis-unit accounting differs from Language IR"
    }
    if (-not (Test-Path -LiteralPath ([string]$artifact.bundlePath) -PathType Leaf) -or
        -not (Test-Path -LiteralPath ([string]$artifact.manifestPath) -PathType Leaf)) {
        throw "$Context`: canonical immutable bundle or completion manifest is missing"
    }
    $actualBundleDigest = (Get-FileHash -Algorithm SHA256 -LiteralPath ([string]$artifact.bundlePath)).Hash.ToLowerInvariant()
    if ($actualBundleDigest -cne [string]$artifact.bundleDigest) {
        throw "$Context`: canonical immutable bundle digest does not match its bytes"
    }
    $publishedManifest = Get-Content -Raw -LiteralPath ([string]$artifact.manifestPath) | ConvertFrom-Json
    foreach ($property in @('snapshotId', 'semanticDigest', 'bundleDigest', 'nodeCount', 'edgeCount', 'evidenceCount')) {
        if ([string]$publishedManifest.$property -cne [string]$manifest.$property) {
            throw "$Context`: published canonical manifest differs at $property"
        }
    }

    return [pscustomobject]@{
        semanticDigest = [string]$artifact.semanticDigest
        bundleDigest = [string]$artifact.bundleDigest
        bundlePath = [string]$artifact.bundlePath
        manifestPath = [string]$artifact.manifestPath
    }
}

function Get-LanguageIrStreamAuthority {
    param(
        [object[]]$BridgeOutput,
        [pscustomobject]$Receipt,
        [string]$Context
    )

    $prefix = '@codebase-workspace-language-ir-stream-authority '
    $authorityLine = @($BridgeOutput | ForEach-Object { $_.ToString() } | Where-Object {
            $_.StartsWith($prefix, [StringComparison]::Ordinal)
        }) | Select-Object -First 1
    if ([string]::IsNullOrWhiteSpace($authorityLine)) {
        throw "$Context`: Language IR stream authority receipt is missing"
    }

    $authority = $authorityLine.Substring($prefix.Length) | ConvertFrom-Json
    if ($authority.schema -ne 'codebase-workspace.language-ir-stream-authority.v2') {
        throw "$Context`: unsupported Language IR stream authority schema $($authority.schema)"
    }
    if ($authority.complete -ne $true) {
        throw "$Context`: Language IR stream artifact is incomplete"
    }
    if ([string]$authority.snapshotId -cne [string]$Receipt.snapshotId) {
        throw "$Context`: Language IR stream snapshot does not match its receipt"
    }
    if ([string]$authority.streamSetDigest -cne [string]$Receipt.streamSetDigest) {
        throw "$Context`: Language IR stream digest does not match its receipt"
    }
    if ([int64]$authority.recordCount -ne [int64]$Receipt.recordCount) {
        throw "$Context`: Language IR stream record count does not match its receipt"
    }
    if ([string]$authority.contentDigest -notmatch '^[0-9a-f]{64}$') {
        throw "$Context`: Language IR stream content digest is invalid"
    }
    if ([int64]$authority.recordCount -gt 0 -and [int64]$authority.byteCount -le 0) {
        throw "$Context`: non-empty Language IR stream has no serialized bytes"
    }
    if ([int64]$authority.recordCount -eq 0 -and [int64]$authority.byteCount -ne 0) {
        throw "$Context`: empty Language IR stream reported serialized bytes"
    }

    $canonical = Get-CanonicalFactBundleAuthority -BridgeOutput $BridgeOutput -Receipt $Receipt `
        -LanguageIrAuthority $authority -Context $Context
    $authority | Add-Member -NotePropertyName canonicalSemanticDigest -NotePropertyValue $canonical.semanticDigest
    $authority | Add-Member -NotePropertyName canonicalBundleDigest -NotePropertyValue $canonical.bundleDigest
    return $authority
}
