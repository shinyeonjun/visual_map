# Product Trust And UX Completion

## Goal

Make Backend Visual Map safe and fast enough for a developer to use as an evidence-backed codebase reading tool. The UI must distinguish verified facts, incomplete answers, stale sources, and heuristic grouping without requiring the user to understand the engines.

## Completion Record (2026-07-20)

The repository implementation is complete for the product boundary described below. Current frontend, native debug, release-profile build, and local installer bundle checks pass on this workstation.

- Trust and lifecycle: real path validation, workspace and DB-profile deletion, structured command errors, explicit source freshness, managed GitHub refresh, command-line secret removal, serialized snapshot persistence, and guarded snapshot restoration are implemented.
- Reading workflows: architecture, API reading path, and change-impact views render only engine-backed data. Change intent, coverage, candidate strength, source evidence, and unknown regions remain explicit.
- Projection structure: `visual_map.rs` is now a small dispatcher and shared-neighborhood coordinator. Architecture, API flow, DB impact, review policy, and projection helpers live in focused modules.
- Frontend structure: canvas guidance, relation rendering, setup, impact review, API reading, source jumps, and inspector modeling are separated into focused modules. The retired duplicate Atlas/workbench shells and their CSS were removed.
- Dead-code control: Knip is part of local verification and CI, with explicit entry points for the native smoke and design-generation scripts.
- UX stability: fixed navigation, non-reloading active mode clicks, bounded search/list rendering, stable async enrichment, and minimum-viewport layout are covered by tests. The left panel now owns one stable analysis criterion; same-mode loading retains the committed target until the left criterion, center focus, and inspector subject can update together. Engine execution, Git operations, snapshot fingerprinting, projection, and workspace scans are dispatched away from the Tauri main thread. The current native build passed the core reading flows, minimum viewport, clean first-run onboarding, and fixed inspector scrolling without source actions being covered by the next-check footer.
- Answer-first navigation: the fixed top switch owns the only transition between answers and advanced structure. The answer surface owns API, code, table, and column targets; the advanced surface owns only overview and multi-target relationships. The former duplicate API/code/DB/impact navigation and its second target browser were removed.

Live PostgreSQL 16, MySQL 8.4, SQL Server 2022, and Oracle Database Free 23.26.2 adapter tests passed on 2026-07-21 against disposable local Docker databases. The desktop product smoke indexed all four sources and verified non-empty table and column inventory; an empty network database now fails the smoke instead of being reported as a successful metadata check. Oracle was exercised through Oracle Instant Client 19.30, which remains an explicit runtime prerequisite. SQLite DDL and DB evidence run without external infrastructure.

On 2026-07-22 the product adapter moved to `database-memory 0.2.0 / contract 2` at source commit `35ed83de33e51eef74a5276c625cb03b24e020c4`. It now accepts only metadata-only, authoritative `complete` snapshots and fails closed on pagination, count, identity, or source mismatches. The candidate binary is pinned for local/internal validation but remains `releaseReady=false` until a public `v0.2.0` artifact is explicitly published.

## Success Criteria

- A tracked source edit or local DB/DDL file edit marks the saved snapshot stale.
- A stale snapshot is never silently re-saved as fresh from an old engine cache.
- Code and DB snapshot writes finish inside the global operation boundary, and live reads cannot start while a saved snapshot is still being restored.
- API rows show HTTP method and confirmed-handler state; the default route prefers confirmed evidence after fresh load and snapshot restore.
- Availability counts are not presented as answer completeness.
- Changing modes closes a floating search result popover.
- Large code and DB lists remain bounded, keep the selected item visible, and disclose hidden counts.
- Search keeps exact result counts while rendering only the best results per group; typing does not rerender the full app per keystroke.
- Source revision and stale state remain visible in the fixed context ribbon.
- High-value canvas text and controls meet the product readability floor without breaking 1180x760.
- Existing truth classes, evidence, limits, and empty-state honesty remain intact.
- Typecheck, production build, Clippy, Rust tests, and focused Tauri UI smoke pass.

## Static Shell Interaction Contract

The workbench is stable; its evidence changes. A user must not have to relearn the screen after choosing another mode or target.

- Left column: on the answer surface, choose one stable target kind in this order: API, code, table, column. On the advanced surface, choose only overview or multi-target relationships. Each target list keeps its width, filter position, scroll state, and current-item marker.
- Top bar: the answer/structure switch stays visible at the 800px product minimum. It is the only control that changes the left-column navigation grammar.
- Center column: answer the selected question. Its header, focus row, canvas bounds, zoom controls, and relationship evidence area keep stable ownership even when their data changes.
- Right column: explain the current target. The reading order is always summary, direct relationships, evidence, source, and next check.
- An inactive mode click changes the answer type. Clicking the active mode does not reload or implicitly open another panel. At compact widths, the target list opens only from the explicit `항목` control.
- A target click is one operation: left criterion, center subject, and right evidence commit together. There is no hidden first-click focus and second-click selection state.
- While a request is running, the last committed answer remains visible and non-interactive with a preparation indicator. The requested mode is marked pending; the old and new subjects are never mixed. The new result replaces all three regions atomically.
- Node and relationship selection are mutually exclusive. Selecting one cannot leave the inspector showing the other.
- Data may change height or density inside a mode-specific canvas, but top-level navigation, column geometry, control locations, and inspector section order do not move.
- A relationship-free target renders as one target with explicit inbound/outbound zero states. Nearby inventory items without relationship evidence are not dimmed into a misleading pseudo-graph.

Mode-specific center answers:

| Mode | Left target | Center answer | Right evidence |
| --- | --- | --- | --- |
| Overview | Structure domain | Grouped architecture boundary and domain detail | Domain summary, direct links, grouping evidence |
| API | Route | Confirmed route-to-handler/call path with separated DB candidates | Selected stage or relation evidence and source |
| Code | Symbol, class, or file | Focus neighborhood using only represented relations | Symbol source, direct relations, evidence, next check |
| DB | Table | Confirmed schema facts plus code-use candidates | Table structure, relation class, source profile |
| Impact | Column | Direct impact, candidates, unknowns, and recommended checks | Column facts, candidate warning, evidence, action |

## In Scope

- Snapshot source fingerprint metadata and staleness checks for code repositories and path-based DB sources.
- Safe stale snapshot restoration behavior.
- Existing API route parsing, ordering, labels, and readiness copy.
- Search popover lifecycle.
- Existing shell labels, source identity, readability, and accessibility fixes.
- Regression tests and current-flow screenshots.

## Out Of Scope

- New indexing engines or speculative middleware/test/migration extraction.
- Runtime row-data access.
- Branch comparison, snapshot history, cloud sync, telemetry, or a new design system.
- Automatic live-network-DB change detection without an engine metadata fingerprint.
- A new framework or dependency.

## Affected Areas

- `src-tauri/src/atlas/model.rs`
- `src-tauri/src/atlas/snapshot.rs`
- `src-tauri/src/atlas/tests.rs`
- `src/App.tsx`
- `src/hooks/useVisualMap.ts`
- `src/hooks/useCodeInventory.ts`
- `src/inventory/snapshotRestore.ts`
- `src/types/visual-map.ts`
- `src/types/workspace.ts`
- `src/types/controls.ts`
- `src/app/controlBuilders.ts`
- Workbench/Atlas top bars, context ribbon, route list, readiness panels, and focused CSS

## Implementation Steps

1. Add optional source revision fields to snapshot metadata.
2. Fingerprint Git HEAD plus dirty/untracked file contents; fall back to indexed source files for non-Git folders. Hash local SQLite/DDL source files.
3. Compare saved and current revisions during snapshot load and add explicit stale reasons.
4. Keep stale snapshot metadata for explanation, but do not restore stale lists or persist recovered engine caches as a fresh snapshot.
5. Reuse `codeInventoryDefaultRoute` everywhere a default API is selected and preserve handled-first ordering on snapshot restore.
6. Parse the existing canonical route ID for method display and show confirmed-handler state in the route list.
7. Rename global mode availability so it cannot be read as answer completeness.
8. Close search results when leaving search mode and show source/stale state in the context ribbon.
9. Bound large side lists, retain exact hidden counts, and keep search input work local until a short debounced commit.
10. Raise the readability floor for the API/impact board and compact shell controls; clarify top-level view names and independent DB sources.
11. Run focused tests, full verification, Tauri smoke screenshots, and a diff-only review.

## Test Commands

```powershell
npm run typecheck
npm run build
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --locked --manifest-path src-tauri/Cargo.toml
powershell -File scripts/smoke-ui.ps1 -Scenario api-flow -Width 1440 -Height 900
powershell -File scripts/smoke-ui.ps1 -Scenario change-impact -Width 1180 -Height 760
```

## Final Verification

- `npm run deadcode`: passed with no findings.
- `npm test`: 20 files and 98 tests passed, including stale snapshots becoming an actionable re-read state instead of a canvas failure, direct source-management entry from the stale status, visible code and DB re-read actions, stale-view and stale-workspace response suppression, snapshot-persistence ordering, restoration status, atomic analysis-target transitions, pending-request replacement, mutually exclusive node/edge selection, compact context accessibility, honest no-relation rendering, confirmed-only call restoration, fixed inspector scroll/footer ownership, and the actionable Oracle Client prerequisite error.
- `npm run typecheck`: passed.
- `npm run build`: passed.
- `npm audit --audit-level=high`: 0 vulnerabilities.
- `npm run security:audit`: passed.
- `npm run verify:inventory`: 707 dependencies verified.
- `npm run smoke:code-matrix`: passed against pinned Java/Spring, C#/.NET, and Python/FastAPI + TypeScript repositories. Scored CALLS split into confirmed/candidate/unknown as 35/96/137, 261/13/432, and 329/136/148 respectively.
- `cargo clippy --locked --all-targets -- -D warnings`: passed.
- Latest `cargo test --locked` run: 175 passed, 1 ignored manual benchmark, 0 failed. CALLS now preserves engine confidence, strategy, and callee expression; only scores of at least 85% enter confirmed paths, and unscored legacy calls force a code re-read.
- Current native smoke run: the final release-profile app passed Atlas drilldown, API flow, change impact, source jump, large repository, and stable navigation at 1180x820. Stable navigation also passed at 820x820 with the explicit compact `항목` panel, focus restoration, selection restoration, and no document overflow.
- Clean first-run native smoke: an isolated empty app-data directory showed no fabricated data, opened this repository, indexed 2,268 code symbols and 260 files, and indexed the two-table/five-column/one-FK DDL fixture. A source edit correctly blocked the stale snapshot; after reindexing without another source change, restart restored code 2,528, DB 2, the four-lane impact board, and `main.orders.id` without a stale warning. Persisted profile data kept `passwordStored: false`, and URL-shaped credentials found in the analyzed test fixtures were stored only as `[REDACTED]`.
- Stale-source recovery smoke: an isolated native app indexed a two-file FastAPI fixture into one route, nine code items, and two files. Editing the source and dispatching the real window-focus path changed the fixed status to `오래됨` with the exact source-drift reason, showed zero operation errors, opened source management from the status itself, exposed `다시 읽기` without expanding project details, and returned to `마지막 읽기` after reindexing.
- DB stale-source recovery smoke: an isolated native app indexed a two-table SQLite DDL fixture with one FK. Two successive schema edits produced the exact DB stale state; the visible DB `다시 읽기` action updated the inventory from five to seven columns, and `db:column:main.orders:updated_at` was returned by the persisted inventory search with zero operation errors.
- DB scope honesty smoke: the same adapter reported six source-level capability limits. The overview no longer presents them as six project-specific missing facts; coverage now reports `기록된 누락 0 · 지원 제한 6`, while column impact exposes one scope card with six evidence entries.
- Native transition trace: a same-mode target change sampled every 20 ms kept the old target in the left criterion, center focus, and inspector while loading, then committed the new target to all three together. Real DB and impact selections synchronized `main.orders` and `main.orders.user_id` across the same regions.
- Recovery and deletion lifecycle: a deliberately corrupt isolated `workspace.json` surfaced a backup warning and was repaired from the preserved backup. DB deletion removed only the selected profile cache and retained the code cache. Project deletion removed only the app-owned workspace while preserving the source repository and Git metadata.
- Local installer lifecycle: the current internal installer was silently installed to an isolated temporary directory with both application data and WebView2 browser data redirected under that directory. The app and both bundled engines were verified, the installer was silently removed, and no process, install directory, app data, or WebView cache was left behind.
- Historical release build: the `v0.1.2` installer evidence passed at the time, but it is superseded by the contract-v2 adapter and does not qualify the current `0.2.0` candidate for public release. A new installer checksum and install lifecycle smoke are required after the explicit `v0.2.0` publication decision.
- Final native captures: `design/ui-concepts/qa-static-shell-1180.png`, `design/ui-concepts/qa-static-shell-820.png`, and `design/ui-concepts/qa-static-shell-context-820.png`.
- Manual bounded projection matrix: 10k, 50k, and 100k inventory inputs completed without dangling edges.

## Codex Implementer Prompt

```text
Read docs/plans/product-trust-ux-completion.md. Implement the next incomplete step only with the smallest safe patch. Reuse codeInventoryDefaultRoute and existing truth classes. Do not add dependencies or fabricate data. Run the smallest relevant checks and report changed files, skipped work, and the next step.
```

## Codex Reviewer Prompt

```text
You did not write this code. Review the current diff only against docs/plans/product-trust-ux-completion.md. Do not modify files. Findings first. Focus on stale data being blessed as fresh, source fingerprint false negatives, API identity regressions, misleading readiness copy, accessibility regressions, and avoidable complexity. If there are no issues, say so clearly.
```
