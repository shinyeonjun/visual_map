# Architecture Index Contract

현재 provider 성능·정확도 검증 기록은
[10-language cold-first audit](COLD-FIRST-MULTILANGUAGE-AUDIT-2026-08-09.md)를 따른다.

`code-memory.architecture-index.v4` is the bounded, source-backed architecture
view emitted beside `code-memory.language-index.v2`. The language index retains
symbol and occurrence detail; the architecture index keeps the ownership tree,
verified boundaries, module relations, and quality ledgers used
to populate the desktop Fact Graph.

This is a transitional donor contract. It is not the canonical Fact Graph and
must not become a second product truth surface. The
`codebase-workspace.language-ir.v2` authority, typed Framework IR for backend
static HTTP routes, and two-pass canonical SQLite bundle now run before this
compatibility artifact. Backend `HTTP_ROUTE`/`HANDLES` product truth has moved
to canonical `HttpRoute`/`Exposes`/`Handles`; this donor remains for the other
framework/API/ORM/asset families until typed parity and desktop import are
complete.

```text
language-index.json
language-index.architecture.json
```

`index --architecture-out <path>` overrides the default sibling path.

## Authority boundary

This output contains extractor facts only. It does not generate final business
areas, business names, canvas coordinates, task modes, or AI explanations.
Those are replaceable derived artifacts in the desktop semantic layer.

## Nodes

| Kind | Meaning |
| --- | --- |
| `PROJECT` | Indexed repository root. |
| `PACKAGE` | Dependency/build manifest boundary. |
| `MODULE` | Compact source module/directory boundary. |
| `FILE` | Source file retained for coverage. |
| `ENDPOINT` | Verified HTTP/RPC entrypoint. |
| `COMPONENT` | Verified UI/component fact. |
| `SERVICE` | Verified service fact. |
| `JOB` | Verified scheduled job/server action. |
| `EVENT` | Verified event/async boundary. |
| `DYNAMIC_BOUNDARY` | Explicit runtime-dependent dispatch. |
| `EXTERNAL_LIBRARY` | Imported package outside the repository. |
| `DATA_RESOURCE` | Source-backed database/file boundary candidate. |

Every node has a stable `id`, `kind`, name/label, optional source path/range,
properties, and external flag. A consumer must key by `id`, never display text.

`FILE` and `MODULE` may carry `semantic=indexed|empty`. Empty means retained for
coverage with no provider symbol/occurrence facts; it is not safe to delete or
report as unanalysed. Modules are compact structural boundaries rather than an
automatic one-node-per-directory mirror.

Framework facts preserve `fact_kind`. `execution_root=true` is restricted to
externally triggered `HTTP_ROUTE`, `RPC_ENDPOINT`, `SCHEDULED_JOB`,
`SERVER_ACTION`, and async-provider `EVENT_HANDLER` facts. Components, services,
async calls, and UI/desktop/game event facts remain queryable but do not become
execution roots. Unknown kinds fail closed.

Diagnostics use a stable machine `code`; clients must not branch on localized
human messages. File-scoped gaps retain their evidence path.

For backend static HTTP registrations, these fields are compatibility-only.
New consumers must read typed `FactNodeDetails::HttpRoute { method, path }`
from the canonical bundle and exact `Exposes`/`Handles` edges. They must not
parse this artifact's label or infer a handler from a similar name.

## Edges

Tree-level:

- `CONTAINS`: repository/package/module/file ownership.

Summary-level:

- `ENTRYPOINT_TO`: verified framework entrypoint to source owner.
- `CALLS`: provider-resolved call relation.
- `IMPLEMENTS`: provider-resolved implementation relation.
- `IMPORTS`: provider/project-model resolved import relation.
- `USES_LIBRARY`: source module to external package.
- `DYNAMIC_CALL`: explicit dynamic-dispatch boundary.
- `READS`, `WRITES`: conservative source-level data/file relation candidate.

Every emitted edge retains evidence. An unresolved internal target is not
invented. External resolution remains an explicit library boundary. Static SQL
can target a table-specific `DATA_RESOURCE` with table, optional schema,
qualified name, source path, and range; a generic DB call stays generic.

The code engine does not claim that a table named in source exists. The desktop
may promote the candidate only after exact unique reconciliation with a
certified DB metadata object. Missing or ambiguous matches fail closed.

## Execution paths

This artifact does not emit a `flows` array. The removed field was an unordered
reachability set and could not satisfy the product's ordered, evidence-backed
`TracePath` contract. The desktop now derives representative and selected paths
from the canonical Fact Graph with bounded depth/expansion, explicit
complete/partial/gap/cycle/depth-limit state, and evidence. This architecture
artifact remains intentionally free of a second `flows` projection.

## Quality ledger

The output carries provider provenance, per-language summaries, file coverage,
analysis units, framework summaries, and diagnostics. UI coverage metrics must
use the appropriate source-unit weighting and keep `indexed`, `partial`,
`excluded`, `missing`, and `unsupported` distinct.

## Determinism and limits

- Stable IDs and canonical ordering must survive repeated unchanged analysis.
- Evidence ordering is canonicalized.
- Source/provider/config changes invalidate the corresponding cache key.
- Provider noise alone does not change success status; provider failure or
  invalid output does.
- The engine never executes the indexed application.

## Deliberate omissions

- no synthetic `DOMAIN` nodes or `DOMAIN_MEMBER` edges;
- no Leiden or other final semantic grouping;
- no guessed route/call targets;
- no runtime-only dispatch claims without evidence;
- no UI layout or task mode contract;
- no AI-generated meaning.

These omissions are required so the desktop can combine facts and AI without
confusing a heuristic projection with source truth.
