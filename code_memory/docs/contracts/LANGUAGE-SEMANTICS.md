# Language semantic contract

This layer is responsible for language semantics only. Framework and ORM
relations are separate adapters and are not created here.

## Target ingestion boundary

`code-memory.language-index.v2` is the current transitional provider output,
not the final desktop data model. The target pipeline is:

```text
language provider -> codebase-workspace.language-ir.v2
-> static normalizer/linker -> canonical Fact Graph
```

The language IR preserves only project context, relevant definitions, resolved
relations, evidence, capability receipts, and file gaps. It does not persist a
raw AST, statements, expressions, tokens, or local variables. The normalizer
may map and verify facts, merge provider-identical declarations, and generate
stable IDs; it may not create a target from name or path similarity.

All language-specific output requirements and canonical mapping rules are
defined in [product requirements section 47](../../../../doc_gpt/ai-visual-codebase-workspace-product-requirements-2026-08-07.md#47-10언어-정적-분석-데이터-계약).
The implementation pipeline, transport, transition order, and keep/rewrite/delete
decisions are defined in [section 48](../../../../doc_gpt/ai-visual-codebase-workspace-product-requirements-2026-08-07.md#48-정적-원재료-파이프라인-구현-설계).

The current output is a donor during migration. A provider is not migrated
merely because it emits definitions and relations: it must also emit an
analysis-unit receipt, semantic-context fingerprint, per-capability support
and execution state, denominator/truncation, evidence, and stable gap codes.
Unsupported, timeout, malformed response, not-run, complete-empty, and partial
must remain distinguishable. An optional LSP response represented only as
`null` is insufficient for the target contract.

The shared target types live in `crates/fact-model` as the
`codebase-fact-model` path dependency. The code engine, DB core, and desktop all
compile against that crate. Source Census now drives the current index input,
and the Analysis Plan is built and validated on every run. It is the sole
owner of provider scheduling. The provider schedule may split an Analysis Unit
into provider-specific execution shards, but it cannot change canonical file
ownership; every planned file must be scheduled or recorded as an explicit
typed omission. Scheduler-owned provider batches are converted to one
validated, authoritative Language IR stream before the transitional
compatibility projection is assembled. The projection is not converted back
into a second IR stream.
Providers still hand off bounded `DocumentOutput`/`RelationOutput` DTO batches
rather than writing native IR records, and `language-index.v2` remains the file
output until canonical ingestion passes parity.

## Current provider-batch Language IR boundary

The direct adapter runs inside `index` before the current provider batches are
merged into the legacy donor and before the architecture output. It is a
fail-closed validation boundary, not a second product output:

- every emitted file belongs to the target Analysis Plan unit;
- `codebase-workspace.provider-schedule.v1` proves that planned files equal
  scheduled files plus explicit typed omissions, with no duplicate ownership;
- source size and SHA-256 are rechecked before provider positions become
  evidence;
- SCIP/compiler UTF-8 columns and LSP UTF-16 columns are converted to validated
  zero-based, UTF-8, half-open spans;
- the provider executable is hashed from the artifact that is actually used;
  a managed-catalog digest mismatch rejects the run;
- every assigned source is parsed by the registered language grammar to create
  an independent denominator of explicit product definitions; the AST inventory
  never resolves a reference or invents a provider symbol;
- definitions require an exact provider occurrence matched to that source
  denominator. Exact source position outranks protocol spellings such as
  `<constructor>`, `.ctor`, and `(value)`; display-name or path similarity never
  creates a definition or edge;
- the matched source token supplies the product display name, canonical kind,
  and type owner. A provider-only definition is rejected; exact-position,
  kind-compatible duplicate provider IDs are recorded as aliases and relation
  endpoints are redirected to the retained identity;
- the same source declaration supplies minimum definition metadata. Callable
  signatures contain the declaration header only, excluding decorators, body,
  and constructor initializers. Visibility comes from explicit syntax or the
  language's static default; provider absence does not force `unknown` when the
  source grammar proves the value;
- all twelve analysis capabilities have a closed support/execution/precision
  policy and explicit gap records;
- unavailable and incomplete providers remain typed gaps rather than empty
  success;
- a call whose resolved endpoints survive but whose donor dispatch metadata was
  lost uses `unknown`, not a guessed `direct` value;
- each unit stream is order/count validated and length-prefixed canonical JSON
  is SHA-256 hashed into `codebase-workspace.language-ir-migration-receipt.v6`.
- that receipt contains total and per-language definition counts, missing/extra
  counts, kind refinements, owner repairs, exact aliases, failed inventory files,
  a digest over `path + canonical kind + name + parent`, per-language metadata
  counts/digests/audit samples, and independent import site/internal/external/
  unresolved/ambiguous/invalid-evidence audit fields;
- receipt v6 also records per-language canonical type-relation counts and
  digests, exact source/target symbols and names, canonical evidence ranges,
  explicit hierarchy site counts, matched sites, unmatched sites, and syntax
  inventory failures. Raw provider `IMPLEMENTATION` is never treated as a
  canonical meaning by itself;
- each SCIP/LSP runner records the exact source scope, analysis root, repository
  config digests, generated/fallback mode, and known/missing semantic context;
  an unplanned root/config or contradictory project dimension rejects the run;
- the analysis root is the AnalysisPlan-owned provider process/workspace boundary.
  A config inherited from an ancestor package remains a hashed config artifact
  and never replaces that root merely because the config file lives higher in
  the repository;
- the Language IR v2 header embeds that actual execution context, and the v6
  receipt's context-set digest must equal the independent
  `provider-execution-context-reconciliation.v3` receipt;
- cache v4 preserves the original execution-context receipt instead of
  reconstructing one from the current plan.

The definition ground-truth gate pins 24 physical source files (25 language-file
contexts) and 117 definitions across
all ten languages. It passes with FP 0, FN 0, 55/55 reviewed owners, exact
per-language definition-set digests, callable declaration signature 63/63,
known visibility 117/117, 37/37 reviewed metadata cases, and cold/warm
definition/metadata determinism. The same result
passes when one provider-visible source per language is expanded beyond 1.1 MB.
The contract and report command are defined in
[semantic quality](SEMANTIC-QUALITY.md).

The type-relation ground-truth gate separately pins 17 source/config files and
the complete 90-relation set for the ten supported languages. It requires 11
`extends`, 7 `implements`, 1 `mixes_in`, 13 `overrides`, and 58
declaration-bound `uses_type` relations, plus 22 reviewed negatives, exact
source evidence, atomic stream-authority content digest, source immutability, and two-run
determinism. Local variables, constructor expressions, hierarchy duplicates,
and receiver/self types are not product type relations.

The execution-context ground-truth gate pins nine configured projects and nine
config-removed variants across all ten languages. It verifies exact configured
mode/dimensions/config SHA-256, typed missing dimensions, no false exact
promotion, and identical context/snapshot/stream/content digests across two
runs. Configured contexts are 10/10 exact; missing variants retain their
language-specific partial or not-executed modes.

The adapter still consumes materialized in-memory provider DTO batches, but it
now writes one validated job-scoped JSONL stream and never reconverts the
compatibility projection. All source capabilities share one syntax tree per
file. Providers do not yet emit native IR records directly. The authoritative
stream is now consumed by the production `index` path's exact two-pass
canonical normalizer/linker, which publishes a fixed-schema immutable SQLite
Fact Bundle plus a completion manifest outside the selected repository. The
header carries the executed-context fingerprint and the bundle revalidates the
stream content digest and record count before linking. Frozen real
project/TU/target variants and the full five-fixture matrix per language remain.
The actual-provider `core-positive`, explicit execution-context, canonical
invariant, and two-run semantic/SQLite-byte determinism gates pass for all ten
languages; dedicated `language-delta`, `negative-resolution`, and
real-repository holdouts remain.

All ten supported languages are release-blocking peers. A patch may migrate
one adapter at a time, but no subset is a reference-quality substitute for the
others and Batch C remains open until every language passes the same common
minimum plus its five real-provider fixtures. Similar quality means the same
identity, evidence, coverage, determinism, and negative-resolution bar; it does
not mean fabricating an equal number of relations for languages with different
static semantics.

## Relations produced

The current donor `code-memory.language-index.v2` output may contain:

| Relation | Meaning |
|---|---|
| `CALLS` | A function or method body calls the target symbol. |
| `REFERENCES` | A source occurrence refers to the target symbol. |
| `IMPORTS` | A source file imports or requires the target. |
| `IMPLEMENTATION` | The provider reports a type implementation/supertype relation. |
| `TYPE_DEFINITION` | The provider resolves a value to its type definition. |
| `USES_TYPE` | A C/C++ provider resolves a type occurrence to a typedef, struct, class, enum, or template symbol. |
| `DEFINITION` | A C/C++ provider resolves a declaration to its implementation. |
| `DEFINITION_OVERRIDE` | The provider reports an override/definition relation. |
| `SYMBOL_REFERENCE` | SCIP reports a symbol-level reference relationship. |

This table describes the transitional donor, not proof that every row has
already crossed the new Language IR boundary. Generic `REFERENCES` and
`SYMBOL_REFERENCE` rows are deliberately not persisted in Language IR or the
canonical Fact Graph. They may be consumed transiently by a typed adapter, but
only a visualization-relevant result such as `calls`, `uses_type`, `tests`,
`reads`, or `writes` can cross the boundary. Imports/exports are deliberately
not copied from provider `IMPORTS` rows. The direct and donor adapters build the
same project import index from Analysis Plan ownership plus compiler
`file_relations`, inventory explicit source sites, and resolve each site through
language-specific exact rules. The v3 receipt owns the independent denominator.
Internal targets become evidence-backed IR relations; known external sites are
boundary counts; unresolved/ambiguous/invalid sites remain typed gaps. The
separate 10-language `imports.v1` baseline gate pins 45 reviewed source/config
files and 39 import/export sites. Cold/warm runs exactly match 15 internal,
7 known-external, 14 unresolved, and 3 ambiguous outcomes with valid
UTF-8/UTF-16 evidence and identical semantic digests. The three real candidate
multiplicity cases use separate Python roots, Java modules, and C# projects.
The remaining seven languages retain explicit missing-context/unresolved cases
instead of inventing language-invalid ambiguity. The 1.1 MB variant matches all
39 reviewed sites on both runs. C# and Java providers now execute against a manifest-sealed
writable copy under the process cache, evidence is remapped to repository paths,
and a post-provider Source Census refuses mixed-snapshot publication. A fresh
pinned-file-only baseline and a clean 1.1 MB fixture both preserve identical
Source Manifest, Analysis Plan, IR stream, semantic payload, target, and
evidence digests across two runs. Ambiguous sites remain typed unresolved-target
gaps and never become graph relations.

The bridge does not invent an edge from a name match. A relation is emitted
only when the SCIP indexer or language server resolves it. C/C++ calls come
from clangd call hierarchy, type and inheritance edges come from clangd type
queries, and declaration/implementation edges come from clangd definition
queries. Lexical parsing is used only for file-level include/import boundaries.

For TypeScript and JavaScript, call classification uses the bundled TypeScript
compiler `Program`/`TypeChecker` call ranges before the SCIP occurrence target
is labeled `CALLS`. If the project model is unavailable, the bridge retains a
source-range fallback; it does not invent an unresolved target.

For TypeScript and JavaScript, the bridge also runs the bundled TypeScript
compiler API as a project model. Its file-level `file_relations` contain
`IMPORTS` edges resolved with the project's `tsconfig`/`jsconfig`, including
`extends`, project references, `baseUrl`, `paths`, and package exports. These
edges are separate from symbol-level SCIP relations because a top-level import
does not always have an enclosing function symbol. These relations and
`project_model_files` are now inputs to both direct and donor Language IR import
resolution. `project_model_files` lists
local files reached by that model, including Vue SFC files whose script blocks
were parsed. No edge is emitted for an unresolved import.

The model also partitions TypeScript and JavaScript into `units`. An explicit
config is used when it owns the file; files not covered by any config are put
into a generated, read-only synthetic unit. The bridge writes synthetic
configs only below its provider scratch directory and passes the real project
root to `scip-typescript` for package and local-file resolution. It never
writes a config, lockfile, or index file into the user's project.

## Required provider inputs

Exact results depend on the project metadata used by the language tool:

| Language family | Project metadata |
|---|---|
| TypeScript / JavaScript | `tsconfig.json`, including `allowJs`/`checkJs` for JavaScript |
| Python | Pyright configuration, type hints, stubs, and selected environment |
| Java | Maven/Gradle project and resolved JDK/classpath |
| C# | restored `.csproj`/`.sln` |
| C / C++ | `compile_commands.json` or equivalent compile flags |
| Go | `go.mod`/workspace and Go toolchain |
| Rust | Cargo workspace, features, build scripts, and rust-analyzer |
| Dart | `pubspec.yaml` and Dart Analysis Server |

## Acceptance fixture

Every language fixture must exercise:

1. a cross-file function or method call;
2. a type or interface implementation;
3. a generic/container type where the language supports it;
4. a source range that can be mapped back to the enclosing symbol.

The acceptance gate must check the exact target symbol and source range. An
empty provider result or unresolved required relation fails the gate instead
of becoming a guessed graph edge.

Dynamic runtime dispatch remains outside this static contract for now. The
adapter contract tests are synthetic boundary tests; they do not replace the
required real-provider `core-positive`, `language-delta`,
`negative-resolution`, `missing-context`, and `determinism` fixtures.

Diagnostics use `diagnostics[].code` for machine decisions. Human messages may
change without changing the meaning of a result; clients must not classify a
provider gap by matching message text.

Semantic collections are canonicalized before serialization. `timings` is the
only intentionally run-dependent field and is operational telemetry, not
semantic evidence. In the target pipeline, providers write bounded per-unit IR
records to a coordinator-backed job bundle. Definition identity is registered
before relation resolution, so the bridge does not need to retain an entire
large provider graph as one in-memory object.

CMake and Meson projects are supported through their generated
`compile_commands.json` (including common `build/`, `out/`, and preset build
directories). VCXPROJ files are detected and included in cache/module
invalidation, but they are not treated as a compiler context by themselves:
an exact C/C++ result still requires a compile database, `compile_flags.txt`,
or `.clangd`. The bridge never fabricates MSVC flags from XML.

Vue single-file components are covered at the script boundary. Imports in
`<script>` and `<script setup>` are resolved, and statically imported component
tags in `<template>` produce internal file relations. Dynamic components,
global registration, and framework auto-imports require Vue/Nuxt tooling and
are not guessed by the bridge.

The gate is run with:

```powershell
.\tests\gates\run-language-semantic-gate.ps1 -AllowMissingProvider
```

`-AllowMissingProvider` is only for a developer machine missing a provider;
release validation must run without it so every configured language is tested.
