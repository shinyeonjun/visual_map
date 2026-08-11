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

## Provider identity boundary

Provider symbol strings are opaque external identities, not canonical Fact
IDs and not user-facing qualified names. Every supported language passes
definitions, parent symbols, occurrences, framework handler references, and
relation endpoints through the same boundary:

- ordinary contract-safe provider identities are retained byte-for-byte;
- identities containing control characters, exceeding the persisted size
  bound, or colliding with the reserved derived prefix are mapped to one
  domain-separated SHA-256 identity;
- the mapping is deterministic and is applied before any symbol join, so a
  definition and every reference to it remain the same endpoint;
- characters are never trimmed or deleted to make an ID pass, because that
  could merge two different provider identities;
- an empty/unjoinable provider identity omits only the affected record and
  contributes to the unit's typed omission/gap accounting. It cannot abort an
  otherwise valid multi-language repository.

Canonical Fact IDs remain strict and never accept provider-native control
characters directly.

## Source coordinate boundary

Canonical source positions always use zero-based UTF-8 byte columns and
half-open ranges. Provider coordinates are converted exactly once before they
can participate in symbol matching, relation reconciliation, evidence, or
storage:

- each raw SCIP document is decoded using its own declared `position_encoding`
  (`UTF-8`, `UTF-16`, or `UTF-32`), then rewritten to canonical UTF-8 columns;
- typed SCIP ranges take precedence over deprecated vector ranges and are
  folded into the same canonical representation;
- supported legacy SCIP producers that omit the field use a closed compatibility
  contract: TypeScript, JavaScript, and C# are UTF-16; C and C++ are UTF-8;
- native LSP clients do not advertise alternate position encodings, so the LSP
  default UTF-16 contract applies to Python, Java, Go, Rust, and Dart;
- tree-sitter/compiler inventory uses UTF-8 byte columns directly;
- UTF-8 BOM bytes are excluded from provider columns but retained in canonical
  absolute byte offsets; CRLF is one line terminator, not source content.

An unknown encoding, negative coordinate, out-of-range line/column, surrogate
split, or UTF-8 mid-codepoint column is rejected at the provider-unit boundary.
The engine never snaps a bad column to the nearest character because that would
turn the wrong source text into apparently confirmed evidence. A provider-unit
invalid-output result remains scoped; it cannot corrupt or abort facts from
other languages.

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
bounded TracePath from source-backed execution occurrences. Direct dispatch can
form an exact hop; resolved virtual/interface/dynamic dispatch remains visible
as a candidate hop and forces the path to `gap`. Type, containment, import,
unknown dispatch, calls without an execution occurrence, and deferred callbacks
cannot become immediate execution hops.
