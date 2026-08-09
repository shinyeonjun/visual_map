# Code Memory documentation

- `contracts/`: current provider and semantic output contracts
- Installation behavior is defined in `contracts/INSTALLATION-LAYOUT.md`.
- `../rust/src/`: Rust bridge code, split into `static_pipeline/`, `providers/`,
  `architecture/`, `frameworks/`, and small pipeline modules. `main.rs` is only
  the CLI and orchestration entry point.
- `../tests/gates/`: executable semantic, framework, and external-project gates
- Build output, provider workspaces, and test caches belong under ignored
  `../artifacts/` or the local Codebase Workspace cache, never in fixtures.
- Framework support is defined in `contracts/FRAMEWORK-PACKS.md` and the
  manifests under `../packs/framework/`.
- The current `language-index.v2`/`architecture-index.v4` shapes are
  transitional donor outputs. The target Language IR and canonical mapping are
  defined in [product requirements section 47](../../../doc_gpt/ai-visual-codebase-workspace-product-requirements-2026-08-07.md#47-10언어-정적-분석-데이터-계약).
- `index` now uses Analysis Plan as the sole provider-scheduling authority. A
  fail-closed provider schedule accounts for every planned file and keeps
  provider-specific execution shards subordinate to canonical Analysis Unit
  ownership. It merges scheduler-owned batches once into an atomic JSONL
  Language IR authority and a compatibility projection, validates the actual
  artifact content digest instead of reconverting the projection, and
  reconciles the root/config/source context actually used by each provider.
  Known writable Java/C# providers execute in a manifest-sealed cache copy,
  and the coordinator refuses to publish if a post-provider Source Census
  differs from the original manifest.
  Provider DTO batches and legacy output files remain transitional. The JSONL
  authority now feeds an exact two-pass linker and immutable SQLite canonical
  code Fact Bundle in the real `index` path. Backend static HTTP route/handler
  facts additionally flow through typed Framework IR into canonical
  `HttpRoute`/`Exposes`/`Handles`. Remaining framework families, DB integration,
  and Tauri import/publish remain. See `contracts/LANGUAGE-SEMANTICS.md` and
  [`contracts/CANONICAL-FACT-BUNDLE.md`](contracts/CANONICAL-FACT-BUNDLE.md).
- Runtime ownership and the canonical store boundary are documented in
  [runtime architecture](../../docs/architecture.md).
- Manually reviewed precision, recall, source coverage, evidence validity, and
  cold/warm determinism are defined in
  [`contracts/SEMANTIC-QUALITY.md`](contracts/SEMANTIC-QUALITY.md). The existing
  10/10 provider gate is only a liveness check. Separate strict gates now pass
  for the pinned 117 definitions/55 owners, 63 callable declaration signatures,
  117 known visibility values, 37 reviewed metadata cases, 35 executable relations, 39
  import sites including 3 real ambiguity cases, and 90 type relations plus 22
  reviewed negatives. Release provider packaging invokes all five gates and
  every ground-truth run validates the canonical bundle bytes and invariants;
  none of these scoped results may be described as
  arbitrary-repository accuracy.
- The five-language relation failures, shared root causes, visualization-only
  analysis boundary, and anti-overfitting validation plan are defined in
  [`contracts/SEMANTIC-RELATION-ROOT-CAUSE-2026-08-07.md`](contracts/SEMANTIC-RELATION-ROOT-CAUSE-2026-08-07.md).
- Important recurring incidents, root causes, verification numbers, and
  recovery order are maintained in
  [`troubleshooting/IMPORTANT-TROUBLESHOOTING.md`](troubleshooting/IMPORTANT-TROUBLESHOOTING.md).
- The current end-to-end static-analysis completion audit, including the exact
  completed/partial/unimplemented boundary and next critical path, is maintained
  in [`contracts/STATIC-ANALYSIS-PROGRESS-2026-08-08.md`](contracts/STATIC-ANALYSIS-PROGRESS-2026-08-08.md).
- The evidence-level speed/provider comparison across all ten supported
  languages, including rejected candidates and Windows packaging blockers, is
  maintained in
  [`contracts/PROVIDER-SHADOW-EVALUATION-2026-08-08.md`](contracts/PROVIDER-SHADOW-EVALUATION-2026-08-08.md).
