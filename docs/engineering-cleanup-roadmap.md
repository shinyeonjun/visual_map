# Engineering cleanup roadmap

Status: canonical hard cut and final local certification completed, 2026-08-09.

This is not a feature backlog. It records the safe cleanup sequence for the
code-analysis vertical slice and separates completed structural work from the
remaining measured completion gates.

## Completed structural work

### One canonical engine path

- Source Census and Analysis Plan own all provider input.
- Provider batches emit one validated Language IR v2 authority.
- Test and framework facts enter the same IR/linker job.
- The desktop consumes only the immutable canonical SQLite artifact receipt.
- `language-index`, `architecture-index`, collector report, temporary desktop
  compatibility JSON, and their JSON-output gates are removed.
- The signed provider release gate now runs ten-language canonical publication
  plus independent bundle-byte determinism.
- The code sidecar exposes CLI contract v3. Development startup rebuilds,
  probes, atomically stages, and checksum-pins that exact executable before the
  app starts. Debug runtime no longer prefers stale Cargo `target` resource
  copies, and uncompressed pre-canonical cache migration has been removed.

### Coordinator decomposition

- `index_project()` is a small stage coordinator.
- `provider_planning.rs` owns project-model preparation, language inventory,
  schedule, cache, and writable provider workspaces.
- Language IR unit emission delegates source inventory, definition
  reconciliation, relation classification, receipt construction, and artifact
  publication to focused helpers.
- No extraction rule was changed merely to make the functions smaller.
- File-local source inventory now lives in `adapter/source_inventory.rs`, runs
  with bounded parallelism for large units, and rejoins in deterministic path
  order. The serial/parallel byte-identity gate is part of the engine suite.

### Physical Language IR ownership split

- `adapter.rs` remains the unit-level coordinator and aggregation boundary.
- `adapter/definitions.rs` owns definition draft reconciliation and its focused
  regression tests.
- `adapter/relations.rs` owns provider-relation filtering, endpoint retention,
  capability mapping, and deterministic relation ordering.
- `adapter/receipts.rs` owns the stable migration, diagnostic, stream, and
  audit receipt data contracts.
- `adapter/source_inventory.rs` and `adapter/artifact_writer.rs` retain source
  enumeration and atomic stream publication respectively.
- The split changed no schema, extraction rule, record order, semantic digest,
  or canonical bundle behavior; the Language IR/canonical regression suite is
  the gate for every physical move.

### Query-backed desktop read model

- Map overview and selection use fixed typed SQLite queries.
- Evidence and trace inputs are fetched by key with explicit limits.
- The previous full `nodes + edges + evidence + coverage + receipts + gaps`
  snapshot materialization/cache is removed.
- Immutable bundle verification uses a small digest cache, not a graph cache.
- Semantic planning keeps full evidence on disk and key-queries only evidence
  referenced by its bounded selected anchors.

### Detected-language provider activation

- A provider-free Source Census emits the exact supported-language set and a
  reusable validated Source Manifest before signed provider activation.
- Only core plus intersecting language packs are verified and extracted.
- The index reuses that manifest instead of performing an additional source
  scan, and the existing final census still rejects mixed source generations.
- The app-data provider store is keyed by signed catalog digest rather than by
  the selected language combination. Each non-core pack is extracted into a
  private staging directory, verified, and atomically appended exactly once;
  later projects reuse it without duplicating multi-gigabyte runtime trees.
- Catalog and per-pack receipts are separate. Existing pack directories are
  never merged or overwritten, and only requested packs have their entrypoints
  reverified for the current analysis.

### Canonical publication hot path

- Language IR verification remains an independent raw-byte gate, while the
  parsed linker work is reduced from three full JSONL passes to two.
- Receipt/structure ingestion and definition identity registration share the
  first parsed pass; relations remain a separate second pass so endpoints are
  never resolved before all definitions exist.
- Valid definition-before-evidence streams are deferred and resolved after the
  first pass instead of silently tightening the shared IR ordering contract.
- Unique evidence takes a SQLite insert fast path. A temporary compact
  source-evidence identity table serves existence/path checks without reading
  and deserializing the full evidence JSON; it is dropped before immutable
  publication.
- Duplicate evidence still runs full identity-collision validation and summary
  merging, so the optimization does not weaken the fact contract.

### Lifecycle controls

- One shared operation ID covers static analysis and every parallel semantic
  child process.
- The UI can cancel the complete operation and keeps the prior snapshot.
- Evidence opens at an exact repository-relative source location.
- Workspace deletion requires confirmation and never deletes the source folder.
- UI tests explicitly clean every render to prevent cross-test state leakage.

### Bounded product receipts

- The default Language IR receipt contains only identities, digests, counts,
  omissions, and release blockers.
- Per-language audit tables and source samples are retained in an opt-in
  diagnostic receipt and do not affect semantic or bundle identity.
- Normal engine progress no longer scales with repository audit sample size.

### Large-map workbench behavior

- The map world expands from actual area positions and estimated rendered
  bounds instead of clipping everything outside a fixed `1440×1080` stage.
- Default top-level placement uses actual projected width/content height and a
  deterministic area-count-dependent column count, so wide or tall areas do
  not overlap the next column or row.
- Fit-to-screen supports large repositories down to 5% scale. At low scale,
  semantic zoom keeps responsibility names, counts, and relationship lines but
  omits member-heavy detail until the reader zooms in.
- Connector geometry is memoized from one measured rectangle set and unchanged
  `ResizeObserver` samples do not trigger another render.
- Analysis feedback reports current stage, exact work count, elapsed time, and
  cancellation rather than a misleading whole-analysis percentage. Existing
  maps remain visible during reanalysis.
- Long engine/AI diagnostics are collapsed behind a short error summary.
- Regression fixtures cover forty unplaced areas and stored positions beyond
  the former fixed world boundary. Large-repository interaction latency and
  peak-memory measurements remain the separate P2 gate below.

## Final local certification

- Code Memory: 323 passed, 0 failed.
- Fact/Semantic contracts: 18 + 3 + 27 passed; three authenticated Codex
  evaluations remain intentionally ignored in local automation.
- Tauri: 79 passed, 0 failed, 4 external-environment tests ignored.
- Frontend: 12 passed; typecheck, ESLint, Prettier, and production build passed.
- Canonical provider gate: 9 project runs, 10 language contracts, 0 skipped.
- Independent cache runs produced identical semantic digests and identical
  canonical SQLite bytes in debug and release profiles.
- Code Memory, Tauri, fact-model, semantic-model, and semantic-compiler passed
  pinned Rust 1.96.1 Clippy with `--locked --all-targets -D warnings` after all
  code and structural patches. The only first-pass warning was an equivalent
  manual clamp in Source Census worker selection; it was normalized, followed
  by another 322-test Code Memory run and a clean five-package Clippy rerun.
- Locked optimized Code Memory build passed. The signed local provider gate ran
  9 project fixtures across all 10 supported languages with 0 skipped and
  produced identical semantic digests and canonical SQLite bytes across two
  independent caches.
- Product version, bundled-engine notices, dependency inventory (945 entries),
  and all five Rust formatting checks passed. Verification fixtures were
  removed without leaving repository artifacts.

External CI is the final repository certification after this checkpoint is
published. It must not replace any of the local gates above.

## Remaining cleanup

### P2. Measured large-repository read model

Record cold/warm map overview latency, selection latency, trace latency, and
peak memory on representative small, medium, and large repositories. Add query
indexes or pagination only from measured evidence.

Completion: explicit product budgets are documented from measurements rather
than invented targets; truncation remains visible.

### P3. Ten-language real-repository holdouts

Run frozen, unseen repositories for all supported languages, including
multi-module/TU/target configurations. Compare selected facts to human-reviewed
source and record recall gaps instead of tuning only the existing fixtures.

Completion: every language has precision/evidence/coverage measurements and a
documented dynamic or missing-context boundary.

## Do not remove

- shared fact schema and stable identities;
- Source Census, Analysis Plan, and actual execution-context receipts;
- exact source evidence, truth class, typed gaps, and coverage;
- two-pass canonical linker and immutable SQLite publication;
- atomic pointer swap and previous revision preservation;
- AI referential/schema verification and abstention;
- deterministic semantic and bundle digests.

DB ingestion, grounded conversation, collaboration, and new relation families
are separate product work. They do not reopen a second static graph or justify
restoring a compatibility output.
