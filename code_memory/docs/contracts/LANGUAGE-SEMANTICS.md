# Language semantics contract

Status: active Language IR v2 and canonical Fact contract, 2026-08-09.

The static engine keeps only facts needed by the final hierarchical code map,
selection inspector, grounded AI input, and exact static TracePath queries.

## Canonical information

The current code graph may contain:

- source files and relevant type/callable/field/test/HTTP-route definitions;
- file containment and exact definition ownership;
- direct call and construction relations;
- internal import and explicit export/re-export relations;
- extends, implements, mixes-in, overrides, and declaration-bound type use;
- exact framework route exposure and handler binding;
- exact test-to-production relations;
- source evidence, file/scope coverage, capability receipts, typed gaps, and
  operational issues.

Raw ASTs, tokens, statements, local variables, arbitrary references, protocol
display strings, UI layout, AI names, and guessed domains are not product
facts. They may exist transiently while verifying a typed fact and are then
discarded.

## Authority flow

```text
provider batch + exact source inventory
  -> one validated Language IR v2 stream
  -> register definitions and stable identities
  -> resolve relations and evidence
  -> deterministic deduplication and relevance pruning
  -> canonical SQLite rows
```

There is no donor-to-legacy projection and no reverse conversion. The Language
IR JSONL artifact is job-scoped staging; the canonical SQLite bundle is the
only published engine output.

## Evidence and truth

- `confirmed` requires existing endpoints and at least one exact source
  evidence record.
- `static_candidate` also requires evidence and represents a bounded static
  uncertainty, never generic name similarity.
- Unknown/unmeasured behavior is a coverage or gap record, not an edge.
- Multiple call sites become multiple evidence records for one logical edge.
- Logical edge identity includes source, target, kind, qualifier, and semantic
  context.
- Human-readable diagnostic wording cannot change semantic identity or bundle
  digest.

The default `language-ir-migration-receipt.v7` progress record is bounded. It
contains snapshot/analyzer identities, deterministic digests, completion
counts, omission counts, and release-blocking audit totals. Detailed
per-language summaries and bounded source samples belong to
`language-ir-diagnostic-receipt.v1` and are written only when
`CODE_MEMORY_LANGUAGE_IR_DIAGNOSTICS=1`. Diagnostic output never participates
in snapshot or bundle identity.

## Context and language differences

All languages emit the same IR shape, but capabilities may honestly differ by
language and execution context. For example, C/C++ headers can belong to more
than one compile context, Rust features can change resolution, and Python
dynamic dispatch can remain unresolved. The engine records those contexts or
gaps instead of forcing uniform fake relations.

Framework facts follow the same rule. A statically proven route is retained
even if its handler is unresolved; `Handles` exists only for one exact handler.
A test becomes `TestCase` only from runner/annotation/registration evidence,
and `Tests` requires a provider-resolved call from that test body.

Ordered paths are not stored as architecture output. The desktop derives a
bounded TracePath only from confirmed execution-oriented facts. Type,
containment, import, candidate, virtual, or unknown relations cannot become
execution hops.
