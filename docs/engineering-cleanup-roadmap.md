# Engineering cleanup roadmap

Status: canonical hard cut and local certification completed, 2026-08-09.

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

### Coordinator decomposition

- `index_project()` is a small stage coordinator.
- `provider_planning.rs` owns project-model preparation, language inventory,
  schedule, cache, and writable provider workspaces.
- Language IR unit emission delegates source inventory, definition
  reconciliation, relation classification, receipt construction, and artifact
  publication to focused helpers.
- No extraction rule was changed merely to make the functions smaller.

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

## Local certification result

- Code Memory: 317 passed, 0 failed.
- Fact/Semantic contracts: 18 + 3 + 22 passed; one authenticated Codex
  evaluation remains intentionally ignored in local automation.
- Tauri: 70 passed, 0 failed, 4 external-environment tests ignored.
- Frontend: 8 passed; typecheck, ESLint, Knip, Prettier, and production build
  passed.
- Canonical provider gate: 9 project runs, 10 language contracts, 0 skipped.
- Independent cache runs produced identical semantic digests and identical
  canonical SQLite bytes in debug and release profiles.
- Code Memory, Tauri, fact-model, semantic-model, and semantic-compiler passed
  Clippy with `-D warnings`.
- Locked optimized Code Memory build passed, and verification fixtures were
  removed without leaving Temp artifacts.

External CI is the final repository certification after this checkpoint is
published. It must not replace any of the local gates above.

## Remaining cleanup

### P1. Physical Language IR module split

The 800-line coordinator has been removed, but `language_ir/adapter.rs` remains
a large physical file. Move existing helper groups into source inventory,
definition reconciliation, relation mapping, and receipt modules without
changing semantic/bundle digests.

Completion: no circular ownership, focused module tests remain local, and the
canonical fixture digests are unchanged.

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
