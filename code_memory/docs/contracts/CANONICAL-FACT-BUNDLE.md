# Canonical Fact Bundle Contract

Status: active code-language + backend HTTP framework path, 2026-08-08

This contract defines the deterministic boundary between provider-specific
Language IR and the desktop product's future published Fact Graph. It is a
static fact artifact, not an AI semantic map and not the active Tauri
generation store.

## Inputs and identity

One canonical run consumes exactly:

- one validated Source Manifest;
- the matching Analysis Plan;
- one closed Language IR v2 stream for every planned Analysis Unit;
- one validated typed Framework IR derived from the same census, plan, source
  bytes, and provider-backed framework donor facts;
- the executed provider-set digest;
- the executed provider-context-set digest.

`SnapshotId::from_execution_inputs` is the only snapshot formula. Language IR,
the linker receipt, the bundle manifest, and the artifact receipt must report
the same snapshot. Before reading facts, the linker hashes the JSONL bytes and
recounts records. A digest/count mismatch blocks publication.

The framework analyzer-set digest includes the selected framework pack JSON
bytes and the typed adapter version. It is joined into the executed provider
set before snapshot calculation. Changing a route rule without changing source
therefore cannot reuse the previous canonical snapshot.

## Exact two-pass linking

Pass 1 registers every provider definition identity as:

```text
(analysis_unit_id, provider_symbol_id) -> canonical_fact_node_id
```

Multiple provider identities may converge on one canonical node. Therefore the
receipt keeps provider identity count and canonical definition node count as
different denominators.

Pass 2 resolves relations using only:

1. exact local provider identity;
2. otherwise, a globally unique identical provider identity;
3. exact file identity, with local unit/language preferred;
4. an explicit Language IR package/module/namespace endpoint.

Display-name similarity, suffix matching, and path similarity never create a
target. Missing or ambiguous endpoints emit an `UnresolvedTarget` gap and no
edge.

## Typed backend HTTP linking

Framework-pack detection is only an analyzer-selection signal. It is not a
route fact. A backend HTTP route enters Framework IR only when all of the
following are source-backed:

- a static normalized uppercase HTTP method;
- a static absolute route path;
- a file owned by exactly one planned Analysis Unit;
- current source digest and exact source range evidence.

Canonical `HttpRoute` nodes carry typed `FactNodeDetails::HttpRoute { method,
path }`; consumers never recover these fields by parsing a display name. The
qualified identity must equal `{METHOD} {path}`.

The source file receives an `Exposes` edge to the route. A `Handles` edge is
created only when the handler reference resolves through the exact provider
identity rules above. Missing or ambiguous handlers do not hide the route and
do not create guessed edges: they produce a typed `UnresolvedTarget` gap. An
exact handler also receives the `Handler` role.

Framework accounting keeps two different denominators:

- raw donor candidate count, before duplicate removal;
- planned route record count, after exact duplicate removal.

The receipt enforces `planned = emitted + rejected`. `FrameworkBindings` has a
single owner: the canonical framework adapter emits exactly one unit receipt;
the language adapter does not also claim this capability.

## Merge and relevance

Logical node and edge identity comes from shared fact-model stable-ID helpers.
Duplicate logical edges merge deterministically:

- truth: `static_candidate < structural < confirmed`;
- resolution: stronger exact producer provenance wins;
- conflicting concrete dispatch becomes `unknown`;
- evidence is sorted and deduplicated.

The first visualization relevance gate retains:

- repository, file, and explicit structure nodes;
- top-level type/callable definitions;
- every exact relation endpoint;
- the complete ancestor chain of retained nodes.

Unused nested members and their otherwise-unreferenced evidence are pruned.
The receipt reports provider identity, canonical node, retained node, and
pruned node counts separately. Relevance is deterministic product selection,
not AI scoring.

## Required invariant gate

Publication is blocked unless all are zero:

- dangling edge endpoint;
- dangling parent;
- dangling node/edge/gap evidence reference;
- confirmed edge without evidence;
- duplicate logical edge.

An omitted provider unit, malformed/incomplete stream, unknown Analysis Unit,
missing relation evidence, source mutation, or identity collision also blocks
publication. An empty success is forbidden.

## SQLite bundle and manifest

The bundle is created outside the selected repository under the application
cache. Staging uses a unique `.sqlite.tmp` file with a fixed, strict schema.
Internal identity tables are removed before finalization. The writer then:

1. closes and `VACUUM`s the database;
2. fsyncs the staging bytes;
3. computes the full SQLite SHA-256;
4. atomically renames to `canonical-<bundleDigest>.sqlite`;
5. fsyncs the final payload;
6. writes and fsyncs `canonical-<bundleDigest>.manifest.json` last.

The manifest is the completion marker. A `.tmp` file is never product truth.

Final tables are:

- `analysis_unit_receipts`
- `evidence`
- `nodes`
- `edges`
- `file_coverage`
- `source_scope_coverage`
- `capability_receipts`
- `gaps`
- `issues`

The schema also includes indexed node-parent and edge source/target columns for
bounded import and adjacency reads.

## Two distinct digests

`semanticDigest` identifies typed machine meaning. Evidence summary and
diagnostic/gap remediation wording are excluded. Rewording a human message must
not create a new semantic map.

`bundleDigest` hashes every final SQLite byte. It protects integrity and is the
artifact content address. The same semantic meaning may have a different
bundle digest when operational wording differs.

## Executable gates

Every ground-truth invocation validates the canonical receipt, manifest,
artifact path, byte digest, accounting, and invariants. Definition and
CALLS/CONSTRUCTS gates additionally require two-run semantic and SQLite byte
digest equality across all ten supported languages. The same checks run with
one provider-visible source per language expanded beyond 1.1 MB.

Current closed-corpus receipts:

- Code Memory unit tests: 283/283;
- shared fact-model: 17/17;
- definitions: 117/117;
- CALLS/CONSTRUCTS: 35/35;
- imports/exports: 39/39;
- type relations: 90/90 plus 22 reviewed negatives;
- framework flow: 10/10; nine backend HTTP cases validate typed Framework IR,
  canonical `HttpRoute`, `Exposes`, and `Handles`, while the C GTK/GLib case
  remains an event donor-flow check;
- canonical invariant failures: 0.

These numbers are scoped regression evidence, not arbitrary-repository
accuracy.

## Deliberate remaining boundary

This bundle currently contains the canonical language path and the backend
static HTTP route/handler baseline. Middleware, RPC, GraphQL, frontend
route/action/API-client, ORM/static SQL, test, event/queue/cache/external/config
adapters and the independent database catalog still need typed canonical
integration. Tauri does not yet validate/import this bundle or atomically
publish an active generation. Clean-versus-incremental equivalence, retention,
crash injection, and frozen multi-unit/large-repository gates remain separate
work.
