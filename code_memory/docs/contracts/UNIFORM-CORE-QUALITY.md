# Uniform core quality contract

Status: Provider liveness gate active; closed executable ground-truth gate passing; certification expansion pending

## Goal

Every language that Codebase Workspace calls “supported” must provide the same minimum
quality of semantic evidence and the same failure behavior. A language-specific
parser may understand different syntax, but it must not silently produce a
weaker or more speculative graph.

## Uniform core

The following capabilities are mandatory for every active language provider:

1. project file coverage and language status;
2. source-located documents and symbols;
3. cross-file direct `CALLS` and `CONSTRUCTS` resolution;
4. exact caller and callee identifiers;
5. an exact callee source range for every emitted call/construct;
6. deterministic relation identity and duplicate suppression;
7. provider diagnostics when a required result cannot be produced;
8. no relation created from a name match alone;
9. one confirmed executable target at most per source call site and semantic context;
10. declarations and implementation bindings never promoted to `CALLS`.

Framework-specific route, middleware, dependency injection, ORM, and database
capabilities are layered on top of this core. They may not weaken the core or
change the meaning of failure states.

The common fixture floor does not mean every real repository emits the same
fact kinds or counts. Each analysis unit must separately report capability
support, execution state, precision, denominator, result counts, and a stable
gap reason. Only a completed capability with zero emitted relations means that
the relation was measured and not found. Partial, unsupported, failed, or
unrun capability output must never be interpreted as absence.

## Common result states

| State | Meaning | Allowed behavior |
| --- | --- | --- |
| `indexed` | The provider completed the requested language scope | Emit only evidence-backed facts |
| `indexed-partial` | Some files or provider work were excluded/limited | Preserve facts and expose partial coverage |
| `provider-failed` | The provider could not analyze the language | Keep a diagnostic; never fabricate relations |
| `missing-tool` | The required provider is unavailable | Report unsupported execution, not success |
| `unsupported` | The language/framework capability is outside the contract | Keep structural facts only when available |
| `stale` | The result does not match the current source revision | Do not answer a focused flow as current |

The same state must have the same meaning in the engine, Tauri adapter, snapshot,
and UI. A missing provider must not become an empty successful graph in one
language and an error in another.

The architecture projection carries the same language and framework summaries
as `languages` and `frameworks` arrays. The current payload is
`code-memory.architecture-index.v4`; the arrays contain provider status, file
coverage, adapter status, and emitted fact/relation counts. The UI may display
these summaries, but must keep the underlying evidence and gaps authoritative.

## Provider liveness gate

The strict gate must run every active language without `-AllowMissingProvider`.
For each language it verifies:

- indexed status;
- at least one document and one resolved `CALLS` relation;
- exact expected target and source range in the language fixture;
- non-empty relation endpoints and source path;
- no duplicate documents or duplicate logical relations;
- no error-level provider diagnostics;
- stable output shape suitable for the common adapter.

The fixture is intentionally semantic rather than textual. Each language uses
its own syntax to express the same behavior: a cross-file caller invokes a
callee and the result can be traced back to source.

This gate proves that every provider can start and produce one representative
positive result. It does **not** calculate precision or recall and must not be
cited as “ten-language accuracy 100%” or equivalent.

## Ground-truth accuracy gate

The separate ground-truth gate exhaustively labels the measured relation scope,
pins reviewed source hashes, and calculates TP, FP, FN, precision, recall,
coverage, evidence validity, and cold/warm determinism. The methodology and
current passing closed-corpus result and its superseded failing baseline are defined in
[Semantic ground-truth quality contract](SEMANTIC-QUALITY.md).

```powershell
.\tests\gates\run-semantic-ground-truth-gate.ps1 -Runs 2
```

All ten languages now pass the strict closed form. An aggregate average may not
hide a failing language, and the closed result must not be presented as
arbitrary-repository accuracy.

Passing the same fixed fixture repeatedly is not sufficient. The shared
anti-overfitting requirements are defined in
[Semantic relation root-cause diagnosis](SEMANTIC-RELATION-ROOT-CAUSE-2026-08-07.md):
positive construct families, negative look-alikes, rename/format metamorphic
tests, and frozen holdout plus real-repository samples are all release inputs.
All languages use the same call-site, target-resolution, dispatch, evidence,
and fail-closed invariants even when their syntax adapters differ.

The same gate also rejects a structural member whose parent is missing or does
not resolve to another definition. The current provider corpus has 43 emitted
methods/constructors/fields/properties and all 43 have valid owners.

The independent definition gate now supplies the exhaustive source denominator:
117 reviewed definitions, including 55 owned members and negative local/parameter
examples, across all ten languages. It compares a digest of path, canonical
kind, source name, and owner, so a matching count cannot hide a wrong definition.

```powershell
.\tests\gates\run-definition-ground-truth-gate.ps1 -Runs 2
```

The closed result is TP 117, FP 0, FN 0 with 100% kind accuracy, owner accuracy,
inventory coverage, and cold/warm determinism. This remains a pinned-corpus
claim, not arbitrary-repository accuracy.

The same strict corpus also has a large-source form. It expands one analyzed
source per language beyond 1.1 MB without changing executable semantics and
requires identical relation and definition precision, recall, kind/owner,
coverage, evidence, and cold/warm determinism:

```powershell
.\tests\gates\run-large-source-semantic-gate.ps1 -Runs 2
```

All ten languages pass. File size alone is not an exclusion reason; a real
provider resource failure remains a typed partial/failed receipt.

The packaged release gate verifies the signed catalog, archive and entrypoint
hashes, exact ten-language coverage, and then runs this contract against a
freshly extracted provider root:

```powershell
powershell -File scripts/run-provider-bundle-gate.ps1
```

Rust runs first so a cold toolchain must resolve a real call before cheaper
provider checks can hide first-run readiness regressions.

## What this contract does not claim

- It does not claim that dynamic dispatch, reflection, generated code, or every
  framework DSL can be resolved statically.
- It does not make a framework pack product-certified merely because its signal
  was detected.
- It does not confirm a database table from a string or symbol name.
- It does not make PHP, Ruby, Kotlin, or Swift active until providers and the
  same gate are implemented for them. The current bridge contains ten active
  languages and does not advertise a broader language target.

## Promotion rule

A language can move from engine-readable to active supported only when its
provider passes this contract and its framework/DB capabilities pass their own
conformance fixtures. If it fails, the release must either fix the provider or
remove the support claim; it must not lower the common gate for that language.
