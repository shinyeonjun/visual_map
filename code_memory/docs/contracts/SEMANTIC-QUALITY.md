# Static semantic quality contract

Status: active verification policy, 2026-08-09.

Quality is measured against reviewed source facts, not against provider output
or whatever the engine happened to emit. A test that only snapshots current
output is useful for determinism but cannot establish correctness.

## Metrics

For every reviewed fact family record:

- true positives, false positives, and false negatives;
- precision, recall, and F1 when a reviewed denominator exists;
- valid evidence rate and exact endpoint rate;
- included/excluded/unmeasured source coverage;
- configured versus missing-context behavior;
- cold/warm and independent-run semantic/bundle determinism;
- runtime and peak-memory measurements for scale fixtures.

Counts must state their denominator. An area count cannot stand in for module
coverage, and a fixture-perfect score cannot be marketed as global 100%
accuracy.

## Required corpus shapes

Each language keeps positive, negative, ambiguous or missing-context cases for
the facts it supports. Cross-language tests cover:

- definitions, kind, owner, visibility, and declaration signature;
- direct calls and construction;
- import/export resolution;
- type hierarchy, override, and declaration-bound type use;
- framework route and exact handler binding;
- test-to-production relations;
- multibyte UTF-8/UTF-16 positions;
- large source files and multi-module/project contexts;
- source mutation, provider failure, timeout, and cancellation.

Real-repository holdouts must not be tuned into the training fixture set. A
new defect adds a minimal regression and, when possible, a different frozen
holdout shape to avoid the pesticide paradox.

## Blocking invariants

- False confirmed relation from name/path similarity: zero tolerance.
- Confirmed relation without valid evidence: zero tolerance.
- Mixed source generations or invalid digest publication: zero tolerance.
- Missing provider/context reported as complete: zero tolerance.
- Nondeterministic semantic or bundle identity for identical inputs: zero
  tolerance.

Recall gaps caused by genuinely dynamic behavior are acceptable only when the
coverage/gap ledger makes them visible. AI may explain such a gap or propose a
separately labelled semantic candidate, but it may not promote it to a static
confirmed fact.

## Verification layers

1. focused parser/resolver unit tests;
2. Language IR and canonical linker characterization tests;
3. the canonical ten-language provider gate;
4. independent bundle-byte determinism;
5. desktop import/query/TracePath tests;
6. measured real-repository and scale audits.

The release gate consumes the same canonical path as the desktop. Removed
`language-index`/`architecture-index` comparison scripts are not authoritative
and must not be reintroduced as a compatibility layer.
