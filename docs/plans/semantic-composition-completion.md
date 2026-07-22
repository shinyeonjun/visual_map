# Semantic Code-DB And Composition Completion

Status: Implemented and locally verified; independent usability and public distribution gates pending

## Progress

- Phase 1 complete: direct static SQL evidence now produces bounded, confirmed
  table read/write and explicit column-use links with source-line evidence.
- Phase 2 complete: 2-8 selected subjects project bounded confirmed paths with
  candidate fallback and honest disconnected warnings.
- Phase 3 complete: the fixed `관계` mode exposes WHAT selection and four HOW
  views without changing the existing single-target navigation contract.
- Phase 4 product artifacts and automated/native verification complete:
  `docs/product-support.md`,
  `docs/usability-test-protocol.md`, and a native `semantic-composition` smoke
  scenario define the support, trust, and task-test gates.
- Independent junior/mid/senior participant receipts and a public
  `database-memory 0.2.0` artifact remain external gates and are not claimed.

## Verification Result

- Frontend: 22 test files / 111 tests, typecheck, production build, and Knip
  dead-code check passed.
- Rust: format and Clippy with warnings denied passed; 207 tests passed and 2
  explicitly environment-gated tests were ignored.
- Product engines: pinned code contract, three-repository language matrix, DDL
  database contract, evidence contract, integrity, notices, inventory, and
  security audit passed.
- Native UI: `loadOrder` and `main.orders` produced one confirmed `DB 조회`
  relation at both 1440 x 900 and 820 x 900 without overflow or lost selection.
- Internal distribution: optimized desktop binary, NSIS installer, and internal
  installer smoke passed. The public engine verification intentionally failed
  closed because `database-memory` remains `releaseReady=false`.
- Detailed receipt: `docs/reports/semantic-composition-completion.md`.

## Goal

Complete the two missing product-core capabilities without modifying the pinned
codebase-memory or database-memory engines:

1. promote only explicit, inspectable SQL evidence from code-to-DB candidates to
   confirmed read/write/column relationships;
2. let a developer select two to eight API, code, table, or column subjects and
   view only the relationships that connect them.

## Success Criteria

- A bounded `search_code` result is promoted only when the referenced source is
  inside the registered repository and the surrounding source contains an
  explicit SQL statement used by a recognized execution call.
- `SELECT`, `INSERT`, `UPDATE`, `DELETE`, and `MERGE` produce confirmed table
  read/write evidence; explicit known columns produce confirmed column-use
  evidence.
- Dynamic SQL, ambiguous schemas, unmatched objects, comments, and plain text do
  not become confirmed links.
- Existing candidate links remain visible and are not silently upgraded.
- API flow and DB impact prefer confirmed code-to-DB links while keeping
  candidates and unknowns separate.
- A user can select two to eight subjects across existing modes, remove them,
  clear the set, and open one stable relationship view.
- Relationship views support `connections`, `calls`, `data`, and `impact`, stay
  under existing node/edge bounds, retain disconnected selected subjects, and
  explain missing paths.
- Existing single-target modes and their saved context keep working.
- Supported engine/database boundaries remain honest; no universal language or
  RDB claim is added.

## In Scope

- Product-side source evidence enrichment in `backend_map`.
- Canonical snapshot links for explicit query reads, writes, and column use.
- A bounded multi-subject projection over existing snapshot relationships.
- Selection controls and one fixed relationship mode in the current workbench.
- Capability/support documentation and a repeatable developer usability task
  protocol.
- Regression, build, dead-code, engine-contract, and native smoke checks.

## Out Of Scope

- Changes to `codebase-memory-mcp` or `db_mcp`.
- Runtime tracing, row-data access, query execution, or credential persistence.
- Treating ORM naming, dynamic SQL, generated queries, or name overlap as direct
  proof.
- DB2, universal ODBC certification, Azure SQL certification, or new database
  adapters.
- Publishing/tagging `database-memory 0.2.0`, code signing, or distributing an
  official installer; those remain explicit owner/external gates.
- Claiming real-user validation before independent backend developers execute
  the protocol.

## Affected Areas

- `src-tauri/src/atlas/linker.rs`
- `src-tauri/src/atlas/semantic_links.rs`
- `src-tauri/src/atlas/composition.rs`
- `src-tauri/src/atlas/api_flow.rs`
- `src-tauri/src/atlas/impact_review.rs`
- `src-tauri/src/atlas/model.rs`
- `src-tauri/src/atlas/mod.rs`
- `src-tauri/src/lib.rs`
- `src/hooks/useVisualMap.ts`
- `src/types/controls.ts`
- `src/types/visual-map.ts`
- `src/app/controlBuilders.ts`
- `src/components/workbench/ModePanel.tsx`
- `src/components/atlas/AtlasCanvas.tsx`
- focused workbench styles and tests

## Implementation Steps

### Phase 1: Explicit SQL Evidence

1. Return mapped source node IDs and match lines from the existing focused-code
   evidence application.
2. Read only canonical files inside the registered repository, with strict byte,
   line-window, match, and result limits.
3. Recognize explicit SQL string literals adjacent to execution APIs and map
   unambiguous tables/columns from the DB snapshot.
4. Emit confirmed `code_db_read`, `code_db_write`, and
   `code_db_uses_column` links with operation, location, and query-execution
   evidence; never persist query text.
5. Surface confirmed links in API reading and impact review before candidates.

### Phase 2: Multi-Subject Projection

1. Add a validated composition request with two to eight unique inventory IDs.
2. Build bounded shortest connecting paths for connection/calls/data views and a
   bounded one-hop union for impact.
3. Keep selected disconnected nodes and return actionable warnings.
4. Reuse existing `VisualNode`, `VisualEdge`, truth classes, and evidence.

### Phase 3: WHAT x HOW Workbench

1. Add a fixed `관계` mode without moving existing controls.
2. Add accessible per-item checkboxes, a bounded selection summary, clear/remove
   actions, relation-view segmented controls, and full-inventory search add.
3. Commit selection, center map, and inspector state atomically through the
   existing request-generation guard.
4. Keep compact-width navigation and existing single-target behavior stable.

### Phase 4: Product Boundary And Validation

1. Record exact language/framework and RDB certification boundaries in one
   product-facing support document.
2. Add a task-based usability protocol for onboarding, API-to-DB tracing,
   multi-subject comparison, and column-change review.
3. Keep publication and independent-participant receipts explicitly pending.

## Test Commands

```powershell
npm test
npm run build
npm run deadcode
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --locked --manifest-path src-tauri/Cargo.toml
npm run smoke:code
npm run smoke:rdb
```

Focused native smoke after Phase 3:

```powershell
powershell -File scripts/smoke-ui.ps1 -Scenario semantic-composition -Width 1440 -Height 900
powershell -File scripts/smoke-ui.ps1 -Scenario semantic-composition -Width 820 -Height 900
```

## Ready-To-Send Codex Implementer Prompt

```text
Read docs/plans/semantic-composition-completion.md. Implement only the next
incomplete phase in backend_map. Do not modify either memory engine, fabricate
direct evidence, or add a parser dependency. Reuse the existing focused search,
snapshot truth classes, request-generation guard, and bounded projections. Run
the smallest relevant tests first and report skipped external gates honestly.
```

## Ready-To-Send Codex Reviewer Prompt

```text
You did not write this code. Review the current diff only against
docs/plans/semantic-composition-completion.md. Do not edit files. Findings first.
Focus on false confirmed code-to-DB links, repository path escape, unbounded
source/projection work, stale async state, broken single-target modes,
accessibility regressions, and unnecessary abstractions. If there are no issues,
say so clearly.
```
