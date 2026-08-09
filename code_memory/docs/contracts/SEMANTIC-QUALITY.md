# Semantic ground-truth quality contract

Status: **measurement active; closed definition/metadata, executable-relation, import/export, and type-relation baseline gates passing**

The measured defects, their shared root causes, fixability by language, and the
anti-overfitting validation contract are recorded in
[Semantic relation root-cause diagnosis](SEMANTIC-RELATION-ROOT-CAUSE-2026-08-07.md).

## What is measured

The existing ten-language semantic gate proves provider availability and one
representative positive relation. It is a liveness/conformance gate, not an
accuracy percentage.

The ground-truth gate compares provider output with source code reviewed
occurrence by occurrence. Its first closed scope is:

- ten supported languages;
- 25 reviewed source files whose SHA-256 digests are pinned;
- all 35 project-local executable occurrences (`CALLS` and `CONSTRUCTS`) in those files;
- one expected occurrence matched to exactly one emitted relation of the reviewed kind;
- exact source path and a range covering the reviewed callee token;
- a resolved target matching the reviewed project symbol;
- cold-cache output compared with warm-cache output.

Standard-library/dependency calls, dynamic targets, and relation kinds other
than `CALLS`/`CONSTRUCTS` are not part of this first score. They must receive separate closed
ground-truth scopes instead of being silently included in one vague percentage.

PHP and Ruby are not part of the active product contract as of 2026-08-07.
Their providers, fixtures, framework packs, packaging entries, and release
gates were removed rather than lowering the common trust bar. This is a
product-quality and maintenance-scope decision, not a claim that every dynamic
language is unsupported: JavaScript and Python remain active and must pass the
same evidence and failure contracts as the other eight languages.

The reviewed truth is
[`tests/ground_truth/semantic-core.v2.json`](../../tests/ground_truth/semantic-core.v2.json).
Changing any reviewed source file invalidates its pinned digest and blocks the
measurement until the annotations are reviewed again.

## Metrics

| Metric | Formula | Meaning |
| --- | --- | --- |
| True positive | matched reviewed occurrence | right relation, right target, evidence covers the real call token |
| False positive | measured emitted relation without a reviewed match | duplicate, wrong target, or evidence attached to the wrong source occurrence |
| False negative | reviewed occurrence without an emitted match | relation the provider missed |
| Precision | `TP / (TP + FP)` | how many emitted measured relations are correct |
| Recall | `TP / (TP + FN)` | how many real measured relations were found |
| F1 | harmonic mean of precision and recall | compact scoped quality summary |
| Source coverage | indexed reviewed files / reviewed files | whether the provider actually measured the denominator |
| Evidence validity | structurally valid ranges / measured emissions | whether evidence points inside a real project file |
| Determinism | identical semantic digest for cold and warm cache | whether unchanged facts remain unchanged |

Small, fully reviewed fixtures require 100% precision, recall, coverage,
evidence validity, and determinism. This does not assert that an arbitrary
real-world repository is 100% understood; it requires the engine to be perfect
on the exact static constructs it claims to support in the closed fixture.

The aggregate micro F1 is diagnostic. It may not hide a weak language. The
release trust score is the **minimum per-language trust score**, because all
ten languages are release-blocking peers.

## Definition and ownership ground truth — 2026-08-08

Definitions have a separate denominator from executable relations. The reviewed
contract is [`tests/ground_truth/definitions.v1.json`](../../tests/ground_truth/definitions.v1.json)
and pins the exact SHA-256 of every measured source. Its closed scope is:

- 10 supported languages, 24 reviewed physical source files, and 25
  language-context file measurements (the shared C/C++ header is measured twice);
- 117 explicit product definitions: types, functions, methods, constructors,
  and fields;
- 55 type-owned members whose owner must resolve to another reviewed definition;
- 63 callable definitions whose source declaration header is the signature authority;
- 117 definitions whose visibility must be known from explicit syntax or the language's static default;
- 37 separately reviewed metadata cases covering the ten languages;
- representative negative names for local variables, parameters, receivers, and
  generic type parameters that must not become definitions;
- a per-language digest over `path + canonical kind + name + parent`, so equal
  counts cannot hide a wrong kind, wrong owner, or swapped definition;
- cold/warm stream, semantic-payload, definition-set, and metadata-set determinism.

The source AST inventory is the independent denominator; providers remain the
semantic identity authority. A definition is matched by exact source position
before protocol display spelling. The adapter then uses the source spelling,
canonical kind, and source owner in Language IR. A provider row without a source
declaration is rejected, except when two provider IDs point to the same exact
source definition with compatible kinds; that case is recorded as a provider
alias and all relation endpoints are redirected to the retained identity.

| Language | Reviewed definitions | TP | FP | FN | Kind accuracy | Owner accuracy |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| TypeScript | 14 | 14 | 0 | 0 | 100% | 100% |
| JavaScript | 9 | 9 | 0 | 0 | 100% | 100% |
| Python | 7 | 7 | 0 | 0 | 100% | 100% |
| Java | 12 | 12 | 0 | 0 | 100% | 100% |
| C# | 12 | 12 | 0 | 0 | 100% | 100% |
| C | 9 | 9 | 0 | 0 | 100% | 100% |
| C++ | 23 | 23 | 0 | 0 | 100% | 100% |
| Go | 10 | 10 | 0 | 0 | 100% | 100% |
| Rust | 10 | 10 | 0 | 0 | 100% | 100% |
| Dart | 11 | 11 | 0 | 0 | 100% | 100% |

Aggregate closed result: TP **117**, FP **0**, FN **0**, precision/recall,
kind accuracy, owner accuracy, inventory coverage, and 10-language determinism
are all **100%**. The adapter transparently performed 33 provider-kind
refinements, nine owner repairs, and one exact-position C++ provider alias
collapse. Those repairs are evidence-backed normalization, not AI inference.

The minimum product metadata result is callable declaration signature **63/63**,
known visibility **117/117**, and reviewed metadata cases **37/37**. Signatures
exclude decorators, bodies, and constructor initializers. Documentation,
annotations, local variables, and statements are not independent graph data.
The old raw-provider signature count 60/162 is a historical diagnostic over a
different denominator and must not be reported as current definition accuracy.

The 1.1 MB gate now runs both the 35-relation contract and this 117-definition
contract. Comment-only size expansion leaves every per-language definition-set
digest unchanged, and both cold/warm forms pass. This certifies the stated 1.1 MB
form only; larger tiers and injected provider resource failures remain separate
scale work.

## Current strict result after the root fix — 2026-08-07

The release engine was run twice with one isolated cache: run 1 cold and run 2
warm. The v2 truth also verifies relation kind, so a constructor emitted as
`CALLS` no longer receives credit.

| Language | TP | FP | FN | Precision | Recall | Source coverage | Deterministic | Trust score |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| TypeScript | 4 | 0 | 0 | 100% | 100% | 100% | yes | 100 |
| JavaScript | 3 | 0 | 0 | 100% | 100% | 100% | yes | 100 |
| Python | 4 | 0 | 0 | 100% | 100% | 100% | yes | 100 |
| Java | 4 | 0 | 0 | 100% | 100% | 100% | yes | 100 |
| C# | 4 | 0 | 0 | 100% | 100% | 100% | yes | 100 |
| C | 2 | 0 | 0 | 100% | 100% | 100% | yes | 100 |
| C++ | 4 | 0 | 0 | 100% | 100% | 100% | yes | 100 |
| Go | 3 | 0 | 0 | 100% | 100% | 100% | yes | 100 |
| Rust | 3 | 0 | 0 | 100% | 100% | 100% | yes | 100 |
| Dart | 4 | 0 | 0 | 100% | 100% | 100% | yes | 100 |

Aggregate scoped result:

- reviewed executable relations: **35**;
- emitted measured executable relations: **35**;
- TP **35**, FP **0**, FN **0**;
- micro precision/recall/F1: **100% / 100% / 100%**;
- source coverage: **25/25 = 100%**;
- evidence structural validity: **100%**;
- cold/warm determinism: **10/10 = 100%**;
- weakest-language release trust score: **100/100**;
- release gate: **passed**.

This is a 100% result for the pinned closed corpus, not a claim that arbitrary
runtime dispatch, reflection, macros, or framework behavior is fully understood.

## Superseded baseline before the root fix (historical)

The release engine was run twice with an isolated cache: run 1 cold, run 2 warm.
The result was:

| Language | TP | FP | FN | Precision | Recall | Source coverage | Deterministic | Trust score |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| TypeScript | 4 | 0 | 0 | 100% | 100% | 100% | yes | 100 |
| JavaScript | 3 | 0 | 0 | 100% | 100% | 100% | yes | 100 |
| Python | 4 | 0 | 0 | 100% | 100% | 100% | yes | 100 |
| Java | 4 | 0 | 0 | 100% | 100% | 100% | yes | 100 |
| C# | 3 | 0 | 1 | 100% | 75% | 100% | yes | 75 |
| C | 1 | 0 | 1 | 100% | 50% | 50% | yes | 50 |
| C++ | 3 | 2 | 1 | 60% | 75% | 100% | yes | 60 |
| Go | 3 | 1 | 0 | 75% | 100% | 100% | yes | 75 |
| Rust | 3 | 2 | 0 | 60% | 100% | 100% | yes | 60 |
| Dart | 4 | 0 | 0 | 100% | 100% | 100% | yes | 100 |

Aggregate scoped result:

- reviewed calls: **35**;
- emitted measured calls: **37**;
- TP **32**, FP **5**, FN **3**;
- micro precision **86.49%**;
- micro recall **91.43%**;
- micro F1 **88.89%**;
- source coverage **24/25 = 96.00%**;
- evidence structural validity **100%**;
- cold/warm determinism **10/10 = 100%**;
- weakest-language release trust score **50/100**;
- release gate: **failed**.

These numbers are valid only for the stated project-local call scope. They are
not an overall percentage for definitions, imports, frameworks, DB access, or
arbitrary repositories.

## Historical findings now covered by regressions

The following findings describe the failing baseline above. They are retained
as regression rationale; they are not the current engine result.

### Critical — the old liveness result was not an accuracy result

The earlier gate accepted any relation whose target looked like the expected
callee. It did not enumerate all real calls and all emitted calls. The old gate
remains useful as a provider smoke test but cannot be cited as semantic
accuracy.

### Resolved — C coverage and call recall were incomplete

`main.c -> add` is correct. `main.c -> box_id` is missing, and the shared
`types.h` C coverage entry is `missing/not-returned-by-provider`. The C score is
therefore recall 50% and source coverage 50%.

### Resolved — C++ constructor evidence was attached to the wrong token

The real constructor call begins at `BoxValue<int>`, but the emitted relation
points at the local variable `box`. Another emitted call points at the
constructor declaration. Both are false positives under the exact-evidence
contract, and the real constructor occurrence remains a false negative.

### Resolved — Go and Rust emitted duplicate logical call targets

Go emits the same `ID()` source occurrence once for `(User).ID` and once for
`ID`. Rust emits the same `id()` occurrence for both trait and implementation
targets and additionally classifies an implementation binding as a call. A
single source occurrence cannot become multiple confirmed product edges without
a typed dispatch/implementation distinction.

### Resolved — C# missed the generic constructor call

`new Box<string>(...)` is absent while the other three reviewed calls are
correct, producing 75% recall.

### Resolved — legitimate large source files were excluded before analysis

The old Census, compatibility snapshot, and provider filter each used an
independent **1,000,000-byte** ceiling. The final hidden ceiling was
scip-typescript's own 1 MB default. The active path now:

- measures every eligible source with a bounded 64 KiB read buffer;
- calculates the full SHA-256 and text-line metrics without a file-sized Census
  allocation;
- uses the real content digest for snapshot/cache invalidation;
- never substitutes an empty source for a large source;
- sends the file to the provider instead of classifying size as exclusion;
- derives scip-typescript's file ceiling from the exact scheduled source set;
- records provider time/workspace exhaustion as a typed partial resource gap.

The large-source gate expands one provider-visible source in every active
language beyond **1.1 MB** using comment-only payloads, then reruns the same 35
reviewed relations twice. All ten languages pass with TP 35, FP 0, FN 0, source
coverage 100%, evidence validity 100%, determinism 10/10, and weakest-language
trust 100.

### Resolved — LSP definition ownership was flattened or pointed at synthetic containers

The LSP decoder previously flattened hierarchical `DocumentSymbol.children`.
Python methods therefore had no class owner; flat Go receiver methods remained
top-level; and Rust methods pointed at non-visual `impl ...` pseudo-symbols that
the Language IR adapter later discarded. Workspace-symbol fallback could also
duplicate a document symbol and take the slot of the richer hierarchical row.

The active path now preserves the exact hierarchical parent before flattening,
deterministically merges duplicate document/workspace symbols, maps Go receiver
syntax and Rust impl declarations to one uniquely present provider type, and
retains no owner when that match is ambiguous. Language IR also normalizes
type-owned provider variables to fields, file-namespace methods to functions,
and evidence-supported constructor shapes to constructors.

The strict relation gate also has a structural invariant: every emitted
`Method`, `Constructor`, `Field`, or `Property` must name a parent that is itself
a definition in the same language result. The current normal and 1.1 MB forms
both pass **43/43 emitted owned members** across ten languages. The independent
definition gate above now supplies the previously missing exhaustive source
denominator and validates all 53 reviewed owned definitions.

## Running the gate

Audit mode writes the full TP/FP/FN report but returns success so a baseline can
be inspected while fixes are in progress:

```powershell
.\tests\gates\run-semantic-ground-truth-gate.ps1 -AuditOnly -Runs 2
```

Strict mode exits non-zero until every language passes the closed fixture:

```powershell
.\tests\gates\run-semantic-ground-truth-gate.ps1 -Runs 2
```

The inspectable report is written to
`code_memory/build/semantic-ground-truth/semantic-quality-report.json`.

The independent definition/ownership gate is run with:

```powershell
.\tests\gates\run-definition-ground-truth-gate.ps1 -Runs 2
```

Its inspectable report is written to
`code_memory/build/definition-ground-truth/definition-quality-report.json`.

The separate large-source form is run with:

```powershell
.\tests\gates\run-large-source-semantic-gate.ps1 -Runs 2
```

Its summary is written to
`code_memory/build/large-source-semantic/large-source-semantic-report.json`.

The independent execution-context gate is run with:

```powershell
.\tests\gates\run-execution-context-ground-truth-gate.ps1 -Runs 2
```

It pins nine configured projects covering all ten languages plus nine
config-removed variants. Configured units are exact 10/10; missing-context
variants have false exact 0 and retain their reviewed partial/not-executed mode,
typed missing dimensions, config artifact hashes, and two-run identity digests.

## Required expansion before product certification

Internal import/package resolution now has a 39-site baseline, and inheritance,
implementation, override, mixin, and declaration-bound type use have a 90-relation
plus 22-negative baseline. These are closed fixture baselines, not arbitrary-repository
certification. Each following scope still needs its own manually reviewed positive
and negative denominator, source hashes, per-language results, and cold/warm digest:

1. frozen unseen build/runtime/classpath execution-context variants beyond the
   certified nine-positive/nine-missing ten-language matrix;
2. frozen unseen syntax holdouts and selected real open-source repositories;
3. framework/API entrypoints;
4. DB read/write and ORM mappings;
5. queue, cache, and external-service boundaries;
6. negative-resolution fixtures with similar names and unreachable targets;
7. additional large-source tiers and real provider resource-limit injection
    beyond the certified 1.1 MB case;
8. clean, cached, and canonical incremental digest parity.

No single combined percentage may be published until its measured scopes and
denominators are shown beside it.

## Root-fix rule

A patch does not pass merely because the current 35 reviewed calls become
green. Every semantic relation fix must establish a language-neutral invariant,
add a negative counterexample, survive rename/format metamorphic variants, and
pass a frozen holdout or reviewed real-repository sample. Production code may
not special-case a fixture path, reviewed symbol name, or expected token.
Definition fixes follow the same rule: a source-independent invariant, reviewed
negative declarations, source-set digest, large-source form, and cold/warm
determinism are required; increasing one fixture count is not a root fix.

The final product analyzes only facts needed by the visualization contract, but
project-local call facts remain necessary inputs for cross-boundary aggregation
and drill-down. Standard-library calls, local variables, primitive field access,
and unproven runtime targets are not promoted into the visualization Fact Graph.
