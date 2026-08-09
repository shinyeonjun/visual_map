# Code Memory engine

Code Memory is the deterministic static-code engine used by Codebase
Workspace. It extracts source-backed facts; it does not invent business areas,
choose a canvas layout, or call AI.

## Supported languages

The product contract contains exactly ten languages:

`TypeScript` · `JavaScript` · `Python` · `Java` · `C#` · `C` · `C++` · `Go` · `Rust` · `Dart`

SCIP/compiler/LSP providers supply semantic identities and resolved targets.
Provider-independent syntax inventories verify definitions, import sites, type
sites, tests, and framework evidence against the selected source bytes. A
missing provider or incomplete compile context is a typed gap, never an empty
successful analysis.

```powershell
cargo run --manifest-path rust\Cargo.toml -- list
cargo run --manifest-path rust\Cargo.toml -- doctor --providers-root providers
cargo run --manifest-path rust\Cargo.toml -- index `
  --root D:\path\to\repo `
  --providers-root providers `
  --packs-root .
```

`index` does not accept an output path. It publishes one immutable,
content-addressed canonical Fact artifact below the local engine cache and
prints a final receipt prefixed with:

```text
@codebase-workspace-canonical-fact-bundle
```

The desktop validates that receipt, the completion manifest, every known
SQLite row type, counts, references, and the bundle digest before atomically
publishing a workspace snapshot.

## One authoritative pipeline

```text
Source Census
  -> Analysis Plan
  -> provider schedule and execution
  -> Language IR v2
  -> two-pass canonical linker
  -> immutable SQLite Fact Graph + manifest
```

- Source Census hashes complete source bytes and records explicit exclusion or
  non-enumerated-scope reasons.
- Analysis Plan owns every included file. Provider shards may split work but
  cannot change file ownership.
- Language IR is transient, validated staging. It carries exact evidence,
  execution context, coverage, gaps, and issues.
- The normal Language IR receipt is bounded to identity, digests, completion
  counts, and release blockers. Per-language audit tables and samples are
  emitted only when `CODE_MEMORY_LANGUAGE_IR_DIAGNOSTICS=1`.
- The linker registers identities first, then resolves relations and evidence.
  Similar names and nearby folders never resolve a target.
- The published SQLite bundle is the only product output. The removed
  `language-index`, `architecture-index`, and `collection-report` formats have
  no runtime or release-gate consumer.

Framework HTTP routes and exact handlers enter the same Language IR and linker
job. A route may survive without a handler; an ambiguous handler never becomes
an edge. Ordered execution views are queried later from confirmed canonical
facts rather than precomputed as guessed flows.

## Precision and failure rules

- Every confirmed relation has existing endpoints and source evidence.
- Provider ranges are normalized to repository-relative UTF-8 half-open spans
  and rechecked against the current Source Manifest.
- Generic references are transient inputs, not product edges.
- Direct calls, construction, imports/exports, type relations, framework
  routes/handlers, and tests use separate typed relations.
- Dynamic dispatch or missing context remains a typed gap.
- A source change during analysis rejects the mixed generation.
- Identical input and analyzer assets must produce identical semantic and
  SQLite bundle digests.
- Application code is never executed by the engine.

## Internal layout

```text
rust/src/
  static_pipeline/          census, plan, Language IR, linker, store
  providers/                SCIP/compiler/LSP execution and decoding
  frameworks/               typed static framework evidence
  provider_planning.rs      provider jobs derived from Analysis Plan
  index.rs                  small pipeline coordinator
  publication.rs            canonical artifact publication
```

`index_project()` coordinates stages only. Language IR emission delegates
source inventory, definition reconciliation, relation classification,
receipts, and artifact publication to focused helpers. Provider DTOs are
internal transport objects and are not a second public graph.

## Build and verification

Fast crate verification:

```powershell
cargo fmt --manifest-path rust\Cargo.toml -- --check
cargo test --locked --manifest-path rust\Cargo.toml
```

Canonical ten-language provider verification:

```powershell
.\tests\gates\run-uniform-core-quality-gate.ps1 `
  -Bridge .\rust\target\release\code-memory-language.exe `
  -ProvidersRoot .\providers
```

That gate verifies the closed ten-language catalog and runs all language
fixtures through the real canonical publication path. C and C++ share a clangd
project run but remain separate catalog obligations. Every run must publish a
non-empty bundle with nodes, relations, evidence, coverage, capability
receipts, and stable identity digests.

Deterministic bundle bytes are checked separately:

```powershell
..\scripts\smoke-code-determinism.ps1 `
  -EnginePath .\rust\target\release\code-memory-language.exe `
  -ProvidersRoot .\providers `
  -PacksRoot .
```

Language-specific ground truth lives in Rust characterization and contract
tests so it validates the same Language IR/linker code used by the product.
Fixture-perfect counts are closed-corpus regression evidence, not a claim of
100% accuracy on arbitrary repositories.

Provider binaries are managed below `providers/` and packaged through the
parent repository's signed provider catalog. Framework declarations live below
`packs/framework/`. Current contracts are indexed in [docs](docs/README.md).
