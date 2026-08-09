# Code Memory documentation

This directory documents the current canonical-only static-code engine.

## Active contracts

- [Language providers](contracts/LANGUAGE-PROVIDERS.md): supported provider,
  provenance, execution, and failure rules.
- [Language semantics](contracts/LANGUAGE-SEMANTICS.md): facts that may enter
  Language IR and canonical linking.
- [Uniform core quality](contracts/UNIFORM-CORE-QUALITY.md): the common
  ten-language minimum and release gate.
- [Semantic quality](contracts/SEMANTIC-QUALITY.md): precision, recall,
  evidence, determinism, and holdout policy.
- [Canonical Fact bundle](contracts/CANONICAL-FACT-BUNDLE.md): immutable SQLite
  transport and publication invariants.
- [Framework packs](contracts/FRAMEWORK-PACKS.md): typed static framework
  evidence.
- [Installation layout](contracts/INSTALLATION-LAYOUT.md): managed offline
  provider assets.

`code-memory-language index` publishes one canonical Fact bundle. The old
`language-index.v2`, `architecture-index.v4`, separate collector report, and
their JSON-output PowerShell gates were removed on 2026-08-09. Provider DTOs
and atomic Language IR JSONL files are internal job-scoped staging, not product
outputs.

## Runtime ownership

```text
Source Census -> Analysis Plan -> provider schedule -> Language IR v2
  -> two-pass canonical linker -> immutable SQLite Fact bundle
```

The desktop validates and publishes the bundle, serves typed SQLite queries,
and supplies a bounded projection to the separate AI semantic compiler. The
static engine neither calls AI nor creates final business areas.

## Verification ownership

- Rust unit/characterization tests own language-specific ground truth and
  fail-closed cases.
- `tests/gates/run-uniform-core-quality-gate.ps1` owns real provider liveness
  through canonical publication for all ten languages.
- `scripts/smoke-code-determinism.ps1` owns independent identical-run bundle
  byte determinism.
- `tests/gates/run-framework-pack-gate.ps1` and
  `run-framework-semantic-gate.ps1` own framework catalog integrity.

Dated audit and research documents remain evidence of how defects were found;
they do not override these active contracts. Important reusable incidents are
summarized in [troubleshooting](troubleshooting/IMPORTANT-TROUBLESHOOTING.md).

Build output, provider workspaces, job artifacts, logs, and caches belong in
ignored build or local application-data directories, never in source fixtures.
