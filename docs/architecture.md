# Runtime architecture

Status: legacy product/UI hard-cut complete; the code-only Base-map vertical
slice is connected end to end, 2026-08-08. Shared static contracts, Source
Census, plan-owned providers, Language IR, canonical normalization/SQLite
bundle, typed desktop import, isolated AI Base compilation, immutable semantic
store, bounded static TracePath retrieval, and L0/L1 map/selection APIs are
live. The surviving React shell now uses the Fluent 2 workbench and persists an
exact provider/model/reasoning-effort tuple. DB ingestion, area detail, chat, incremental analysis, and
measured real-model large-repository reconciliation quality remain. Inputs beyond the current global
reconciliation ceiling still require hierarchical multi-stage reconciliation.

## Product pipeline

```text
local project + optional DB metadata
        |
        v
code / DB extractors
        |
        v
canonical Fact Graph  ---- evidence + truth class + coverage
        |
        +----> AI semantic compiler (derived, replaceable)
        |
        v
one hierarchical canvas <----> one project conversation
```

The Fact Graph is the authority. AI-generated names, summaries, areas, and
explanations are derived artifacts and may not overwrite extractor facts.
Canvas and chat share one active project, one published snapshot, and one AI
provider/model/reasoning-effort setting.

## Fluent workbench and CLI settings

The desktop UI is a fixed light Fluent 2 workbench for the current release.
Official Fluent React controls and system icons own buttons, fields, dialogs,
tooltips, and provider configuration. Segoe UI Variable carries interface text;
Cascadia Mono carries paths, IDs, source, and exact CLI receipts. Neutral
Mica/Acrylic-style layers keep the project rail and inspector quiet while the
blueprint canvas remains the visual authority. Verified, structural, and
candidate relations retain separate colour and dash encodings; the design
system may not collapse those truth classes into decoration.

The workspace stores `{ provider, model, effort }`. Existing workspace files
without `effort` read as `high`. The Base semantic packet includes the effort,
so changing it changes the semantic input digest and cannot reuse a revision
created with a different inference budget. The current selectable Codex aliases
are `gpt-5.6-sol`, `gpt-5.6-terra`, `gpt-5.6-luna`, and `gpt-5.5`; the default is
Sol with `high`. Claude uses the CLI aliases reported by the installed client.
Only an installed provider can be saved.

Runtime hand-off is explicit:

```text
codex --model <model> --config model_reasoning_effort="<effort>"
claude --model <model> --effort <effort>
```

The broker still adds its isolation, schema, timeout, and no-session flags. The
receipt above describes only the user-controlled model and effort portion.

The desktop does not bundle, copy, or reinstall Codex or Claude. An app-lifetime
`ProviderRegistry` discovers installed provider candidates once and stores a
resolved absolute executable path plus its reported version. On Windows it
considers the visible standalone Codex path and the existing Codex Desktop
managed runtime, then selects the newest executable that is not older than the
shared `models_cache.json` client version. At an analysis boundary it probes
the pinned executable once; only a missing, changed, or newly incompatible
runtime causes re-discovery. Every local semantic partition, its retry, and the
global reconciliation share the same immutable runtime snapshot.

## Target AI semantic boundary

AI consumes only a bounded, provider-neutral projection of the last published
Fact Graph. It has four separate jobs:

1. base semantic compilation: L0/L1 areas, labels, responsibilities, and
   static-region assignments;
2. selection-time area detail: inferred role overlays and short narratives for
   existing relation bundles and ordered TracePaths;
3. typed-gap candidate relations: optional and only when the static layer has
   already emitted a bounded dynamic/reflection/registration gap with existing
   endpoint candidates;
4. grounded chat: answers and map actions that cite existing facts, edges,
   traces, areas, and evidence.

Numeric AI confidence is not a canonical output. A weak semantic signal keeps a
structural label with a typed fallback reason. AI never calculates LOC, counts,
layout, relation direction, or path order and never mutates Fact Graph truth.
The exact packet, response, verifier, persistence, and UI mapping contract is
the product requirements section 50. The provider-neutral base packet, prompt,
strict schema, verifier, and stable approved semantic IDs live in
`crates/semantic-model` and `crates/semantic-compiler`. The desktop runtime now
builds that packet from a published canonical snapshot, invokes the selected
Codex or Claude process without repository tools, verifies and stores the
revision, and exposes a bounded Base map. Area detail, gap candidates, and chat
remain deliberately disconnected.

## Surviving components

- `src/`: the new one-map desktop shell. It contains no legacy product modes or
  fabricated demo graph.
- `src-tauri/src/workspace.rs`: local v2 workspace identity and configuration.
- `src-tauri/src/fact_graph.rs`: read-only/query-only canonical-fact SQLite
  validator, immutable workspace copy, tamper-evident pointer, status/read
  model, and failed-refresh checkpoint/restore boundary.
- `src-tauri/src/analysis.rs`: one-job-per-workspace code analysis coordinator,
  bounded process execution, progress, canonical artifact receipt, and import.
- `src-tauri/src/semantic/`: bounded Fact-to-AI read model, isolated provider
  adapters, strict verification, immutable/cacheable revision store, and typed
  map/selection projection.
- `src-tauri/src/provider_assets.rs`: signed catalog and bounded ZIP activation
  for bundled language providers.
- `src-tauri/src/engine/`: bounded sidecar discovery and process execution.
- `src-tauri/src/source.rs`: repository-root source navigation boundary.
- `crates/fact-model/`: UI/AI-free shared Source Manifest v1, Analysis Plan v1,
  Language IR v1, and canonical-fact v1 contracts, deterministic IDs,
  evidence, coverage, gap/error codes, and fail-closed validators.
- `crates/semantic-model/`: provider-neutral Base semantic packet, proposal,
  approved revision, region/trace/bundle/area identities, and no confidence
  score or provider process.
- `crates/semantic-compiler/`: deterministic packet canonicalization and input
  digest, untrusted-source prompt policy, strict Codex JSON Schema, and
  fail-closed hierarchy/membership/reference/evidence verifier. Its real-model
  fixture runs in a fresh temporary directory without repository access.
- `code_memory/rust/src/static_pipeline/`: the live repository census and the
  deterministic ten-language unit/context planner. It reads no AI output.
- `code_memory/`: ten-language static/provider analysis and framework facts.
- `db_memory/`: certified metadata-only RDB analysis core and CLI.

The code sidecar's `index` command now emits the canonical immutable SQLite
bundle and completion manifest in addition to temporary compatibility outputs.
The desktop `analyze_workspace` command runs it, verifies every typed row and
digest, copies it into app-owned storage, then swaps the workspace pointer. The
DB sidecar still exposes its previous adapter contract and is not part of this
vertical slice. Transitional code outputs may be removed only after remaining
consumers and parity gates are migrated.

The shared contract foundation compiles as a direct path dependency of the
code engine, database core, and Tauri desktop. Source Census is now the code
index's live file-input authority. The Analysis Plan is built and validated on
every run and is the sole owner of provider scheduling. A subordinate provider
schedule may create execution shards for TypeScript project configs, C/C++
contexts, or bounded Dart batches, but every shard retains its canonical
Analysis Unit ID and must account for every planned file or an explicit typed
omission. Scheduler-owned provider batches are merged once into a validated,
job-scoped atomic JSONL Language IR authority. `language-index v2` is emitted
only as a compatibility projection of that same merge; it is not converted
back into a second stream. Artifact schema, snapshot, stream digest,
record/byte counts, content SHA-256, and repeated-run determinism are blocking.
Each runner also
records the source scope, root, config digests, fallback mode, and known/missing
semantic dimensions it actually used; that receipt is reconciled with the plan
before Language IR validation. Provider DTOs remain the runtime hand-off and
JSONL is linker staging rather than the final product bundle. Fact Graph SQLite
schema v2 has not been migrated.

## Target static fact ingestion

~~~text
AnalysisRequest
  -> Source Census / SourceManifest
  -> Analysis Unit + Semantic Context Planner
  -> fail-closed Provider Schedule (execution shards only)
  -> resource-weighted Provider Scheduler
  -> provider-independent source Definition Inventory
  -> per-unit Language IR v1
  -> deterministic Static Normalizer / Linker
       -> definition identity pass
       -> exact relation + evidence pass
       -> framework / ORM / asset reconciliation
       -> dedup / truth / context merge
       -> relevance + capability validation
  -> immutable canonical SQLite import bundle
  -> typed Tauri staging import
  -> invariant + digest validation
  -> atomic published-snapshot pointer swap
~~~

AI is not part of this pipeline. It consumes only the last published Fact
Graph and its coverage ledger. A failed, cancelled, truncated-without-receipt,
or invalid analysis may not replace the previous published snapshot.

### Source and context authority

- Source Census records included files and excluded, unsupported, unreadable,
  oversized, generated, vendor, dependency, and non-enumerated scopes. It uses
  cryptographic content digests for snapshot/evidence identity; a fast
  metadata hash may be used only for cache optimization.
- Analysis units follow compiler and package-manager boundaries rather than
  arbitrary folder size: tsconfig/jsconfig, Python execution environment,
  Maven/Gradle, solution/project/TFM, C/C++ compile command, go.mod/go.work,
  Cargo target/features/cfg, and Dart package configuration.
- A file may have more than one semantic context. Facts are merged only when
  their semantic identities agree; conflicting context-qualified relations
  remain separate.
- SCIP/compiler/LSP results are semantic authorities. CST parsing is a bounded
  fallback for syntax recovery, literal facts, and framework markers, never
  name-based target resolution.

### Language IR and canonical normalization

`codebase-workspace.language-ir.v1` is a transient, rebuildable provider boundary.
It carries unit receipts, capability receipts, relevant definitions, exact
relations, evidence, file coverage, diagnostics, and stable gap codes. It does
not persist raw ASTs, tokens, statements, locals, UI state, or AI semantics.

The current direct adapter verifies this boundary from scheduler-owned provider
batches and compares the resulting stream against the legacy donor projection.
It rechecks source bytes against the Source Manifest, converts SCIP UTF-8 and
LSP UTF-16 positions to exact UTF-8 half-open spans, verifies the actual provider
artifact digest, emits a closed twelve-capability receipt, and rejects guessed
targets. A separate ten-language syntax inventory supplies the definition
denominator without resolving references; provider definitions must match an
exact source declaration, and source name/kind/owner win over protocol display
spellings. The receipt carries per-language counts and a definition-set digest.
Type relations use a separate declaration-bound syntax inventory plus exact
provider-resolved local endpoints; the ten-language gate currently pins 90
reviewed relations and 22 negatives, including `extends`, `implements`,
`mixes_in`, `overrides`, and `uses_type`.
It reports `unknown` dispatch when the provider DTO lost that classification
instead of claiming a direct call. The provider decoders still materialize
bounded DTO batches, while the validated merged IR now feeds the deterministic
two-pass canonical linker and immutable SQLite bundle writer.

Normalization is two-pass at minimum: register definitions and identities
first, then resolve relation endpoints and evidence. Framework, ORM, build,
contract, messaging, deployment-boundary, revision, and DB reconciliation
adapters feed the same typed job. Unknown is a coverage state, not an edge.

The logical edge key is source, target, kind, qualifier, and semantic context.
Multiple ranges become multiple evidence records. A confirmed edge requires
existing endpoints and source-backed evidence. Similar names, nearby folders,
and possible virtual implementations do not create confirmed relations.

### Bundle and publishing boundary

The sidecar writes a job-scoped SQLite import bundle below app-owned temporary
storage. It is a transport artifact, never the product database and never a
file inside the analyzed repository. Workers send bounded IR records to one
coordinator writer. The completed bundle has an external typed manifest with
schema, source/config/provider digests, capability receipts, counts, and the
SHA-256 of the closed SQLite payload. Publishing that manifest is the complete
marker; there is no mutable `complete: true` field inside the payload it
authenticates.

Tauri opens the bundle read-only, reads only known tables through fixed typed
queries, and streams rows into a new product-database generation. It does not
execute bundle-provided SQL, triggers, or extensions. After foreign-key,
evidence, coverage, count, and digest validation, one transaction swaps the
published pointer. WAL may support readers during the generation write, with a
single-writer and explicit checkpoint/retention policy.

The desktop importer validates the manifest and closed SQLite payload, opens it
read-only with fixed queries, revalidates typed rows/references/counts, and
publishes an immutable bundle plus tamper-evident pointer. It does not load the
entire SQLite file as a byte buffer. Product query/read models are still bounded
and must be extended before claiming large-project completion.

### Transition order

1. Shared path crate for Source Manifest, Analysis Plan, Language IR, and the
   next canonical fact model:
   **implemented at the contract boundary** with golden serialization,
   field-derived stable-ID validation, streaming IR order/count validation,
   and invalid-input tests. SQLite storage migration is intentionally deferred
   until the staging importer is implemented.
2. Replace source census/digests and certify analysis-unit ownership:
   **implemented**. Census drives current file inputs; Analysis Plan is the sole
   scheduling authority; the provider schedule is fail-closed over planned,
   scheduled, and explicitly omitted files. The legacy module planner and its
   old/new comparison receipt have been removed.
3. Migrate each current provider and its exact project/TU/target variants to
   Language IR with explicit per-capability support, execution, precision,
   denominator, truncation, and reason: **provider-batch direct validation and
   execution-context receipts implemented**. Scheduler-owned provider batches
   now produce one job-scoped atomic JSONL authority; the compatibility
   projection comes from the same merge and is never converted into a second
   IR stream. Definition, direct-call/construct, import/explicit-export, and
   type-relation independent baselines pass. All capability inventories share
   one syntax tree per source file.
   Generic provider references are transient adapter input and intentionally
   are not a Language IR or canonical edge. Signature/visibility, relevance,
   exact context variants, and real-project holdouts remain. All ten languages are release-blocking peers; a shared
   core-positive fixture alone does not complete this step.
4. Add deterministic normalization and the canonical import bundle:
   **implemented for the current code pipeline**.
5. Fold typed framework and selected collector facts into the same job:
   **partially implemented** for static HTTP route/handler facts; remaining
   resource/boundary adapters and DB reconciliation are pending.
6. Add the Tauri analysis-job, staging-import, validation, cancellation, and
   atomic-publish API: **implemented for the code-only vertical slice**, with
   hard/idle timeout, single-workspace exclusion, progress, and rollback when
   semantic publication fails.
7. Prove selected node/edge/evidence/coverage parity, then delete
   `language-index v2`, `architecture-index v4`, the separate `collect`
   command, and `collection-report v1`.
8. Add incremental invalidation only after clean publication works; an
   incremental result must have the same digest as a clean result.

The authoritative detailed contract, keep/rewrite/delete matrix, and gates are
in [product requirements section 48](../../doc_gpt/ai-visual-codebase-workspace-product-requirements-2026-08-07.md#48-정적-원재료-파이프라인-구현-설계).

## Current integration status

Workspace creation, signed provider activation, code analysis, canonical Fact
publication, Base semantic compilation, real L0/L1 map retrieval, and selection
evidence now form one tested desktop path. The old Atlas adapters remain deleted.
The current map emits only evidence-backed, conservatively folded relations. A
leaf area may emit several nodes only when they come from one ordered static
`TracePath`; without such a path it still emits at most one representative
anchor, so unordered members never become a fake chain. Complete, partial, gap,
cycle, and depth-limit outcomes are preserved in the read model. DB facts, the
rest of Q1-Q8 retrieval, persisted layout, area-detail jobs, and app-owned chat
remain.

## Static TracePath query

`src-tauri/src/static_query/trace_path.rs` derives paths from the immutable
canonical snapshot; it does not create a second graph or persist guessed flow
rows. Base-map retrieval is bounded to 64 entrypoints, two paths per entrypoint,
64 paths total, depth 10, and 2,048 expansions per entrypoint. A selected fact
may retrieve up to eight paths at depth 16 and 8,192 expansions.

Representative entrypoints and paths are selected round-robin by their static
region owner before the final deterministic ordering. A route-heavy area cannot
consume the entire base-map budget. Evidence-scoped analysis gaps affect only
the path whose fact evidence matches; evidence-less unit/workspace gaps retain
their intentionally broader scope.

Only confirmed relations with exact execution orientation are hops. Direct
`Calls` and `Constructs` are accepted; route/handler order is derived by reading
`Handles` in reverse at query time while preserving its canonical direction.
Imports, containment, types, tests, candidates, virtual/interface dispatch, and
unknown targets never become arrows. A known direct branch can remain visible,
but an unresolved sibling marks the result as a gap instead of allowing a
misleading complete status. Missing facts make map projection fail closed rather
than bridge across them.

The static-scope cleanup also removed runtime telemetry/CI graph collection,
the code engine's duplicate generation/query store, unordered architecture
"flows", DB-only MCP/query vocabulary, and legacy table/impact/trace CLI
surfaces. DB impact, relationship trace, and diff algorithms remain internal
core primitives because the final Q1-Q8 queries need them.

## Non-negotiable invariants

1. A confirmed edge requires existing endpoints and at least one source
   evidence record.
2. Unknown or unmeasured data is not converted to zero, confirmed, or a line.
3. A failed analysis never replaces the last published snapshot.
4. AI may derive meaning but may not mutate fact identity, evidence, truth, or
   coverage.
5. Large-project views are bounded and disclose omitted/truncated information.
6. The app exposes one map and one conversation, not purpose-specific modes.
7. `unknown` is a coverage/state value, never a persisted FactEdge. The only
   non-confirmed FactEdge class is evidence-backed `static_candidate`; AI
   candidates live in the semantic layer.
8. A product execution path is an ordered, evidence-backed `TracePath`.
   Unordered reachability sets may not be labelled or rendered as flows.
