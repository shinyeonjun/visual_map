# Code Memory engine

This directory contains the static/provider code-analysis engine used by
Codebase Workspace. It extracts source-backed facts; it does not decide the
final business areas, canvas layout, or user workflow.

## Supported languages

The executable contract fixes exactly ten languages:

`TypeScript, JavaScript, Python, Java, C#, C, C++, Go, Rust, Dart`

Dedicated SCIP indexers are used where available and native LSP clients cover
the remaining languages. Providers are static-analysis tools; application code
is not executed. Missing providers, unsupported project context, exclusions,
and partial coverage remain explicit in diagnostics and coverage records.

```powershell
cargo run --manifest-path rust\Cargo.toml -- list
cargo run --manifest-path rust\Cargo.toml -- doctor
cargo run --manifest-path rust\Cargo.toml -- index --root D:\path\to\repo --out D:\temp\language-index.json
```

## Outputs

- `codebase-workspace.language-ir.v2`: bounded, job-scoped provider-to-normalizer
  JSONL authority with exact source evidence, capability/gap receipts, and
  executed provider context.
- `codebase-workspace.canonical-fact.v1`: immutable fixed-schema SQLite Fact
  Bundle and completion manifest. `index` validates the Language IR bytes,
  resolves exact identities in two passes, applies deterministic relevance,
  checks graph invariants, fsyncs the content-addressed bundle, and only then
  publishes its manifest under the application cache outside the repository.
- typed Framework IR: backend static HTTP method/path/source facts are validated
  against the same census and plan, then published as canonical `HttpRoute`,
  file `Exposes`, and exact handler `Handles` records. Unresolved handlers never
  become guessed edges.
- `code-memory.language-index.v2`: documents, symbols, occurrences, relations,
  source ranges, provider provenance, coverage, and diagnostics.
- `code-memory.architecture-index.v4`: project/package/module/file hierarchy,
  verified framework entrypoints, resource boundaries, and module relations.
  Ordered execution paths are deliberately not precomputed by this artifact.
- `code-memory.collection-report.v1`: optional evidence from API contracts,
  build descriptors, migrations, explicit messaging APIs, deployment
  boundaries, and revision state.

The engine deliberately does not emit synthetic final “DOMAIN” groups. The
desktop Fact Graph ingests canonical source facts; a separate AI semantic
compiler may derive replaceable names and areas without changing those facts.

## Precision and safety

- `index` starts with `codebase-workspace.source-manifest.v1`: full SHA-256
  content identity, VCS/product ignore rules, explicit non-enumerated scopes,
  encoding/size/link status, and stable exclusion reasons.
- A deterministic `codebase-workspace.analysis-plan.v1` owns every included
  language candidate and is the sole provider-scheduling authority. The
  subordinate `codebase-workspace.provider-schedule.v1` receipt proves that
  every planned file is scheduled exactly once or has an explicit typed
  omission; provider-specific execution shards cannot change plan ownership.
- Scheduler-owned provider batches are converted directly under that plan,
  revalidate exact source coordinates and provider provenance, emit bounded
  `codebase-workspace.language-ir.v2` streams, and write deterministic
  `codebase-workspace.language-ir-migration-receipt.v6` plus
  `provider-execution-context-reconciliation.v3` receipts. The compatibility
  projection is no longer converted back into a second IR stream.
- A provider-independent syntax inventory measures explicit type/function/
  method/constructor/field definitions in every assigned source. The provider
  must match that denominator at an exact source location; final name, kind, and
  owner are checked against the source instead of protocol display strings.
- Provider results are source/config/provider-checksum keyed and deterministic.
- Java and C# providers that are known to materialize build/IDE state execute
  against a manifest-sealed writable copy under the local cache. Every index
  run rescans the selected repository before publishing and fails closed if
  its Source Manifest changed.
- Analysis is CPU/memory weighted, timeout bounded, and cancellation aware.
- Dependency/build/cache scopes are excluded by explicit policy and remain
  visible as non-enumerated coverage receipts rather than fake zero-file data.
- Unresolved targets and dynamic behavior are boundaries, not invented edges.
- Confirmed relations preserve file/range evidence.
- The engine emits extraction artifacts and an immutable canonical import
  bundle. Selecting/publishing the active product generation and serving graph
  queries still belong to the desktop Tauri layer.

```powershell
cargo run --manifest-path rust\Cargo.toml -- collect --root D:\path\to\repo
```

## Build and test

```powershell
cargo fmt --manifest-path rust\Cargo.toml -- --check
cargo test --locked --manifest-path rust\Cargo.toml
cargo build --locked --release --manifest-path rust\Cargo.toml
.\tests\gates\run-definition-ground-truth-gate.ps1 -Runs 2
.\tests\gates\run-semantic-ground-truth-gate.ps1 -Runs 2
.\tests\gates\run-large-source-semantic-gate.ps1 -Runs 2
.\tests\gates\run-import-ground-truth-gate.ps1 -Runs 2
.\tests\gates\run-type-relation-ground-truth-gate.ps1 -Runs 2
.\tests\gates\run-test-relation-ground-truth-gate.ps1 -Runs 2
.\tests\gates\run-execution-context-ground-truth-gate.ps1 -Runs 2
.\tests\gates\run-framework-flow-gate.ps1
.\tests\gates\run-import-ground-truth-gate.ps1 -Runs 2 -MinimumSourceBytes 1100000 `
  -OutputRoot .\build\large-source-import
```

The definition gate's pinned closed corpus currently measures 117 definitions
and 55 owned members across all ten languages with TP 117, FP 0, FN 0 and 100%
kind, owner, inventory-coverage, and cold/warm determinism. These are scoped
fixture results, not a claim of perfect analysis for arbitrary repositories.
The independent import fixture currently pins 45 reviewed source/config files
and 39 reviewed sites across all ten languages. The baseline passes with 15
internal, 7 known-external, 14 typed unresolved, and 3 genuinely ambiguous
outcomes. Python, Java, and C# use separate valid project roots to create the
three candidate-multiplicity cases; the other seven languages keep explicit
missing-context/unresolved cases instead of manufacturing impossible ambiguity.
Both the clean baseline and the import-specific
1.1 MB variant now run from pinned-file-only temporary fixtures and pass two-run
Source Manifest, Analysis Plan, IR stream, semantic payload, target, and
evidence determinism. Ambiguous sites fail closed as typed gaps and never become
an internal edge.

The independent type-relation corpus pins 17 reviewed source/config files and
90 relations across all ten languages: 11 extends, 7 implements, 1 mixes-in,
13 overrides, and 58 declaration-bound uses-type relations. It also checks 22
reviewed negatives, exact evidence, source immutability, direct/donor parity,
and two-run determinism. These are scoped fixture results, not an arbitrary
repository accuracy claim.

The independent test-relation corpus covers all ten languages with one exact
test-to-production call and one name-only negative per language. The gate pins
the reviewed source hashes, inspects the immutable canonical SQLite bundle,
requires 10 exact confirmed `Tests` edges, rejects all 10 name-only candidates,
preserves the rejected cases as typed static gaps for possible later AI review,
and verifies two-run determinism without mutating source.

The execution-context gate independently pins nine configured projects and
nine config-removed variants spanning all ten languages. Every configured unit
must be exact, every missing-context unit must remain partial/not-executed as
specified, config artifact SHA-256 values must match the source, and two runs
must preserve context, snapshot, stream, and authoritative-content digests.

Provider binaries are managed under `providers/` and packaged through the
parent repository's signed provider catalog. Framework declarations are under
`packs/framework/`. Authoritative engine contracts live in `docs/contracts/`.
