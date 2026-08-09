# Runtime architecture

Status: canonical-only code vertical slice, 2026-08-09.

Codebase Workspace turns a selected local repository into one evidence-backed
hierarchical map. The product supplies information quickly; it does not add
onboarding, impact-analysis, API-design, or refactoring “modes”. Users interpret
the same map for their own work.

## End-to-end ownership

```mermaid
flowchart LR
    A["Selected repository"] --> B["Source Census"]
    B --> B1["Detected-language provider activation"]
    B1 --> C["Analysis Plan"]
    C --> D["10 language providers"]
    D --> E["Language IR v2"]
    E --> F["Two-pass canonical linker"]
    F --> G["Immutable SQLite Fact bundle"]
    G --> H["Tauri typed read model"]
    G --> I["Bounded AI semantic compiler"]
    H --> J["Hierarchical canvas"]
    I --> J
    G --> K["Static TracePath query"]
    K --> J
```

The Fact Graph is authoritative. AI areas, names, summaries, and explanations
are replaceable derived revisions. An AI failure cannot invalidate or replace
the last verified static snapshot.

## Static engine

`code-memory-language index` owns one pipeline:

1. Source Census streams complete source bytes, hashes them, and records
   explicit excluded or non-enumerated scopes.
2. The desktop activates only the signed provider packs needed by that exact
   manifest. The validated preflight manifest is reused by `index`, so provider
   selection does not add a third source scan. Packs are atomically appended
   once to a catalog-digest-addressed app-data store; different repository
   language combinations reuse the same verified pack bytes instead of
   creating selection-specific copies.
3. Analysis Plan assigns every included language file to compiler/package/TU
   boundaries.
4. The resource-weighted scheduler runs provider shards without changing plan
   ownership.
5. Direct adapters reconcile provider output with exact source inventories and
   emit one validated Language IR v2 authority. File-local AST inventories run
   in a CPU- and memory-bounded pool and are merged back in repository-path
   order; record serialization reuses a bounded buffer. The adapter coordinator
   delegates definition reconciliation, relation classification, receipt data
   contracts, source inventory, and artifact writing to separate modules. These
   modules do not own scheduling or canonical publication, so extraction rules
   still have one direction and cannot form a second pipeline.
6. The linker reads the Language IR in two parsed passes: the first ingests
   receipts/evidence and registers definition identities, including valid
   forward evidence references; the second resolves evidence-backed
   relations. The SQLite staging store keeps a compact source-evidence index
   so identity and path checks do not deserialize the full evidence payload.
   It then deduplicates, prunes visualization-irrelevant details, and verifies
   graph invariants.
7. The store fsyncs an immutable content-addressed SQLite bundle and completion
   manifest, then emits the canonical receipt.

The removed `language-index`, `architecture-index`, and `collection-report`
formats have no runtime path. Provider DTOs and Language IR JSONL are bounded
job staging and are cleaned after publication.

### Sidecar identity and development startup

The code sidecar publishes a machine-readable CLI contract. The desktop adapter
and bundled manifest require contract v3, which includes `contract`, `list`,
`doctor`, `detect-languages`, and `index` and excludes the removed collector and
JSON-index commands. `npm run tauri dev` first performs a pinned, locked,
incremental release build, probes that contract, atomically stages the verified
executable under `src-tauri/engines`, and updates its development checksum.
Debug runtime resolution points directly at that staged directory; Cargo
`target/debug/engines` and `target/release/engines` copies are not runtime
authorities. Production still resolves only packaged resources verified during
the build.

### Static trust rules

- Similar names and nearby directories never resolve a relation.
- Confirmed and static-candidate relations require exact source evidence.
- Unknown or unmeasured behavior is a gap/coverage record, never an edge.
- Compile/project context participates in identity and cache keys.
- A source change during analysis rejects the mixed generation.
- Identical source/config/provider input produces identical semantic and bundle
  digests.

## Desktop publication and querying

`src-tauri/src/analysis.rs` starts one analysis operation per workspace. One
operation ID is passed to the static sidecar and every semantic child process.
Cancellation therefore stops the complete analysis, including parallel AI
partitions, while preserving the previously published revision.

The desktop validates the artifact schema, completion manifest, SQLite digest,
known tables, typed rows, references, and counts before copying it into
app-owned immutable storage and swapping the workspace pointer. It never writes
inside the selected repository.

`src-tauri/src/fact_graph.rs` serves fixed SQLite queries. Interactive map
overview, selection nodes, evidence, and bounded TracePath inputs are read by
key and limit; a click no longer materializes all node/edge/evidence tables or
caches two complete snapshots in memory. Semantic planning materializes the
structural node/edge/coverage tables needed for ownership and TracePath, then
fetches only evidence IDs referenced by the bounded final anchor set. The old
full-evidence snapshot API has no runtime path. A small verification-digest
cache avoids repeating immutable bundle verification and is invalidated by the
published pointer identity.

Source navigation resolves repository-relative evidence below the selected
root, rejects path escape/reparse traversal, and opens an exact file/line in a
supported local editor. Deleting a workspace removes only app-owned workspace,
Fact, and semantic data; the source repository is explicitly preserved.

## Static TracePath

TracePath is a query over canonical facts, not a second stored graph. It follows
only confirmed execution-oriented relations with exact direction:

- direct `Calls` and `Constructs`;
- route-to-handler order derived from exact `Handles` direction;
- bounded continuation through known callable/service nodes.

Containment, import, type, test, candidate, virtual/interface dispatch, and
unknown targets cannot become execution arrows. Partial paths remain visible
with gap/cycle/depth-limit status instead of being presented as complete.

## AI semantic boundary

AI receives a bounded, provider-neutral projection of the verified Fact Graph.
It may:

- group existing static regions into L0/L1 semantic areas;
- name and summarize those areas;
- assign only supplied Fact/Region IDs;
- cite supplied evidence and relation IDs;
- abstain and keep structural names when evidence is weak.

AI may not create static nodes, alter endpoints, reorder an execution path,
calculate source counts, change truth class, or hide coverage. Output is
schema-validated and referentially checked before an immutable semantic
revision is published. A trace can represent an area only when every region
that owns the trace belongs to that area's direct or descendant membership;
cross-area traces remain relationship information rather than area evidence.

Large semantic inputs use adaptive, disjoint local Map jobs. The planner derives
the partition shape from the complete prompt byte size, static region count,
and the per-request safety budget; it does not target a fixed job count. Each
local result is verified independently against its exact region scope. After
all partitions pass, one compact global coordinator receives only short region
aliases, structural summaries, verified local area hints, and selected boundary
counts. It may merge responsibilities across partition boundaries, but it never
receives or emits canonical fact, evidence, or trace IDs. Deterministic code maps
the aliases back to canonical regions, attaches only eligible citations and
traces, and runs the ordinary full-packet verifier before publication. This
avoids the former fan-in Reduce tree without degrading L0/L1 into partition-local
hierarchies.

Provider concurrency is also calculated at runtime from the current job count,
logical CPU count, and available memory. Optional environment values are safety
caps only; they cannot turn the adaptive scheduler into a larger fixed pool.
Scheduler telemetry records the inputs and selected worker count for every run.
The post-provider Language IR inventory uses the same principle: units with at
least 32 files may use up to eight file workers, but the cap is reduced from
available memory and the largest planned source file. Serial and parallel paths
must produce identical stream, semantic, and artifact digests.
All provider calls are ephemeral: they do not resume or create the user's
Codex/Claude chat session. The semantic cache key includes Fact digest,
prompt/schema version, provider/model, and reasoning effort so identical
approved input remains stable. When a provider
returns a schema-valid but unverifiable result, the broker sends that rejected
JSON and the exact verifier error through a bounded repair prompt. For
hierarchy/reference failures it also enumerates every safely detectable
violation of the same invariant from the rejected object, because the strict
verifier itself is fail-fast. If one repair exposes a later validation error,
the latest rejected object may receive one final targeted repair; semantic
analysis is never restarted and repair is capped at two provider calls. Every
complete corrected object must pass the same verifier. Only an execution
failure with no result retries the exact prompt that failed.

## UI boundary

The React/Fluent 2 workbench contains one project list, one hierarchical canvas,
and one inspector/conversation rail. It renders static facts and approved
semantic revisions; it does not infer relation truth in the browser.

Relation encodings remain distinct:

- confirmed: evidence-backed primary connector;
- structural: containment/composition encoding;
- static candidate: evidence-backed uncertain connector;
- unknown/unmeasured: no connector, shown through coverage/gap UI.

Model selection stores an exact `{ provider, model, effort }` tuple. Existing
workspaces without effort default to `high`. The broker resolves and pins one
installed compatible CLI runtime for the analysis boundary.

The canvas is sized from the published layout rather than a fixed viewport.
Top-level areas without saved reader positions are packed deterministically
using their projected width and content height; the column count expands with
the area count. Fit-to-screen may zoom out to 5% for very large repositories.
Below the detail threshold the same map becomes a responsibility overview:
area identity, child count, item count, and relationships remain visible while
member chains and relation labels are deferred until zoom-in. This is semantic
zoom, not a second product mode, and it does not change graph membership or
truth.

Analysis progress is deliberately stage-scoped. The UI shows the current
stage, the engine's exact label/work count, elapsed wall time, and cancellation;
it never presents one stage's ratio as an end-to-end completion percentage or
claims an unmeasured ETA. Reanalysis leaves the prior published map visible
with a compact status overlay. Long failures render a bounded summary with
collapsed technical detail so diagnostics remain available without replacing
the workbench.

## Surviving components

- `src/`: Fluent desktop shell and typed canvas/inspector.
- `src-tauri/src/analysis.rs`: workspace operation, progress, cancellation,
  canonical receipt import.
- `src-tauri/src/fact_graph.rs`: immutable bundle validation and query-backed
  read model.
- `src-tauri/src/static_query/`: bounded exact graph queries.
- `src-tauri/src/semantic/`: partitioned AI broker, verifier, revision store,
  and map projection.
- `src-tauri/src/source.rs`: safe evidence navigation.
- `crates/fact-model/`: provider/UI/AI-independent static contracts.
- `crates/semantic-model/`: provider-neutral semantic contracts.
- `crates/semantic-compiler/`: prompt, schema, canonicalization, and verifier.
- `code_memory/`: ten-language static code engine.
- `db_memory/`: separate metadata-only DB engine; not yet part of this code
  vertical slice.

## Non-negotiable invariants

1. A confirmed edge has existing endpoints and valid evidence.
2. Unknown is never converted into zero, confirmed, or a line.
3. Failure/cancellation never replaces the last published snapshot.
4. AI cannot mutate static identity, evidence, truth, coverage, or path order.
5. Large-project queries are bounded and disclose omissions.
6. The product exposes one map rather than purpose-specific modes.
7. The original repository is read-only from the product's perspective.
8. Only one canonical graph representation crosses the engine/desktop
   boundary.

## Remaining completion work

- measure and optimize cold analysis on representative large repositories;
- measure hierarchical Reduce quality, latency, and provider cost on S/M/L
  repositories and add immutable intermediate-result caching;
- finish real-repository holdouts for every supported language;
- integrate DB metadata through its own canonical typed adapter;
- connect grounded app-owned conversation after map correctness and latency are
  accepted.
