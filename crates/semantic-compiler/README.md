# Codebase Semantic Compiler

This crate implements the first AI task only: base semantic compilation for an
L0/L1 codebase map. It owns:

- deterministic packet canonicalization and input digests;
- a provider-neutral prompt policy that treats repository text as untrusted
  data;
- a strict JSON Schema suitable for Codex `--output-schema`;
- fail-closed parsing, ID/membership/hierarchy/evidence verification;
- deterministic semantic area and revision IDs after verification.
- deterministic relation-aware partition planning for inputs that are unsafe
  as one provider request;
- compact global reconciliation of independently verified local results.

The complete Base packet remains the only publication authority. A large input
is divided into disjoint, complete local Base packets; every local result is
verified and may be cached, then one compact reconciliation prompt produces a
global proposal that is checked again against the original complete packet.
Local partitions are never published as a complete map. Source excerpts are
sent only to their owning local partition and are not repeated in global
reconciliation.

The default planner executes a packet directly only when it has at most four
regions and a rendered prompt of at most 96 KiB. Otherwise it grows
relation-cohesive partitions bounded to 12 regions and 96 KiB of serialized
input. The global prompt carries the full region directory, verified local
results, and a deterministic top-boundary summary, with a 512 KiB fail-closed
safety ceiling. These byte budgets are implementation safety limits, not
claims about provider token capacity.

It does not execute an AI provider, read a repository, persist revisions,
render the canvas, infer candidate relations, or implement chat.

## Prompt evaluation

Normal tests validate deterministic packet construction, prompt-injection
isolation, strict parsing, complete region assignment, evidence locality,
fallback honesty, and stable approved IDs without calling a model.

An ignored real-model evaluation runs Codex in a fresh temporary directory with
an output schema and no repository access:

```text
cargo test --test codex_prompt_eval -- --ignored --nocapture
```

It is intentionally opt-in because it needs an authenticated Codex CLI and
consumes provider capacity.
