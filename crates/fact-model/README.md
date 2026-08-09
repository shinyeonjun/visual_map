# Codebase Fact Model

This crate owns the versioned, provider-neutral contracts shared by the code
engine, database adapter, and desktop process.

It is deliberately split by responsibility:

- `identity`: deterministic IDs and SHA-256 digests
- `source`: canonical repository paths, zero-based half-open source spans,
  and source flags
- `evidence`: source, artifact, and database-catalog evidence
- `analysis`: language, provider, analysis-unit, and semantic-context types
- `source_manifest`: cryptographic file/scope census and deterministic manifest
- `analysis_plan`: complete file-to-unit ownership and config-set plan
- `coverage`: file/scope census, capability receipts, gaps, and operational
  analysis issues
- `language_ir`: the transient provider-to-normalizer record protocol
- `fact_graph`: canonical analysis-unit receipt, node, edge, role, and
  bundle-manifest rows
- `validation`: stable fail-closed contract validation errors

## Version boundary

`SourceManifestV1`, `AnalysisPlanV1`, `LanguageIrV2`, and `CanonicalFactV1` are
the active static/import-contract versions. The code engine emits one validated
Language IR stream and publishes one immutable canonical SQLite bundle. The
former `language-index`, `architecture-index`, and `collection-report`
compatibility outputs have been removed; no product or release gate consumes a
second graph representation.

## Invariants

- Unknown enum variants and unknown struct fields fail deserialization.
- Stable IDs are full SHA-256 domain-separated hashes of length-prefixed
  components; display names, line numbers, and snapshot timestamps are not ID
  inputs.
- Repository paths are normalized relative paths using forward slashes.
- Source positions are zero-based and ranges are half-open.
- A confirmed or static-candidate edge requires evidence.
- Unknown and unmeasured states are coverage/gap records, never graph edges.
- Excluded or unreadable repository subtrees remain explicit scope receipts;
  unenumerated descendants are never reported as indexed files.
- Every included language candidate is owned by at least one validated analysis
  unit, and unit file counts must equal the assignment ledger.
- A Language IR file must match the header language, every relation must match
  the header semantic context, and a capability may appear only once per unit.
- The Language IR v2 header carries the actual provider execution context. A
  not-executed provider may preserve syntax/project-model facts, but it may not
  claim SCIP/LSP/compiler evidence or provider/compiler resolution.
- A resolved call with missing dispatch metadata uses `unknown`; it is never
  guessed as `direct` or `not_applicable`.
- Provider unavailable and provider execution incomplete are stable coverage
  gaps, not empty successful analyses.
- Source-manifest, config-set, and analysis-plan digests are recomputed from
  canonical semantic fields during validation.
- Runtime timestamps are operational metadata and never semantic digest input.

This crate contains data contracts and deterministic validation only. It does
not run providers, read repositories, query databases, persist bundles, or call
AI.
