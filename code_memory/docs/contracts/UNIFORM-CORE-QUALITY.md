# Uniform core quality contract

Status: Active for the current 12-language release gate

## Goal

Every language that Visual Map calls “supported” must provide the same minimum
quality of semantic evidence and the same failure behavior. A language-specific
parser may understand different syntax, but it must not silently produce a
weaker or more speculative graph.

## Uniform core

The following capabilities are mandatory for every active language provider:

1. project file coverage and language status;
2. source-located documents and symbols;
3. cross-file direct `CALLS` resolution;
4. exact caller and callee identifiers;
5. a source range for every emitted call;
6. deterministic relation identity and duplicate suppression;
7. provider diagnostics when a required result cannot be produced;
8. no relation created from a name match alone.

Framework-specific route, middleware, dependency injection, ORM, and database
capabilities are layered on top of this core. They may not weaken the core or
change the meaning of failure states.

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
`code-memory.architecture-index.v3`; the arrays contain provider status, file
coverage, adapter status, and emitted fact/relation counts. The UI may display
these summaries, but must keep the underlying evidence and gaps authoritative.

## Release gate

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

The packaged release gate verifies the signed catalog, archive and entrypoint
hashes, exact twelve-language coverage, and then runs this contract against a
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
- It does not make Kotlin or Swift active until providers and the same gate are
  implemented for them. The current bridge contains twelve active languages;
  the product's broader fourteen-language target is a future expansion gate.

## Promotion rule

A language can move from engine-readable to active supported only when its
provider passes this contract and its framework/DB capabilities pass their own
conformance fixtures. If it fails, the release must either fix the provider or
remove the support claim; it must not lower the common gate for that language.
