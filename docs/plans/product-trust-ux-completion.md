# Product Trust And UX Completion

## Goal

Make Backend Visual Map safe and fast enough for a developer to use as an evidence-backed codebase reading tool. The UI must distinguish verified facts, incomplete answers, stale sources, and heuristic grouping without requiring the user to understand the engines.

## Completion Record (2026-07-19)

The repository implementation is complete for the product boundary described below. Current frontend, native debug, and local installer-lifecycle checks pass. A fresh release-profile executable and installer still require verification on a Windows runner that permits Cargo build scripts; the current workstation blocks newly generated unsigned release build-script executables through enterprise Code Integrity policy (`os error 4551`).

- Trust and lifecycle: real path validation, workspace and DB-profile deletion, structured command errors, explicit source freshness, managed GitHub refresh, command-line secret removal, serialized snapshot persistence, and guarded snapshot restoration are implemented.
- Reading workflows: architecture, API reading path, and change-impact views render only engine-backed data. Change intent, coverage, candidate strength, source evidence, and unknown regions remain explicit.
- Projection structure: `visual_map.rs` is now a small dispatcher and shared-neighborhood coordinator. Architecture, API flow, DB impact, review policy, and projection helpers live in focused modules.
- Frontend structure: canvas guidance, relation rendering, setup, impact review, API reading, source jumps, and inspector modeling are separated into focused modules. The retired duplicate Atlas/workbench shells and their CSS were removed.
- Dead-code control: Knip is part of local verification and CI, with explicit entry points for the native smoke and design-generation scripts.
- UX stability: fixed navigation, non-reloading active mode clicks, bounded search/list rendering, stable async enrichment, and minimum-viewport layout are covered by tests. Engine execution, Git operations, snapshot fingerprinting, projection, and workspace scans are dispatched away from the Tauri main thread. The current debug native build passed the core reading flows, minimum viewport, and clean first-run onboarding; the release installer must still be repeated on an allowed runner.

Live PostgreSQL, MySQL, SQL Server, and Oracle smoke tests still require their respective external connection environment variables. Their adapter contracts are covered locally; SQLite DDL and DB evidence run without external infrastructure.

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
- `npm test -- --run`: 10 files and 19 tests passed, including stale-view and stale-workspace response suppression, snapshot-persistence ordering, restoration status, and immediate removal of deleted workspaces from the visible list.
- `npm run typecheck`: passed.
- `npm run build`: passed.
- `npm audit --audit-level=high`: 0 vulnerabilities.
- `npm run security:audit`: passed.
- `npm run verify:inventory`: 707 dependencies verified.
- `cargo clippy --locked --all-targets -- -D warnings`: passed.
- Latest executable `cargo test --locked` run: 152 passed, 1 ignored manual benchmark, 0 failed. The current tree still compiles through `cargo test --locked --lib --no-run`; this workstation's enterprise Code Integrity policy blocks the newly generated test executable from starting.
- Current native smoke run: Atlas drilldown, API flow, change impact, source jump, large repository, and the 1180x760 minimum viewport passed sequentially against a real 10,308-item workspace in the current debug build.
- Clean first-run native smoke: an empty app-data directory created a local workspace, indexed 2,445 code items from this repository, indexed the two-table/five-column/one-FK DDL fixture, and restored code, DB, and impact counts after an app restart without stale warnings. Native deletion then removed the DB cache and app workspace immediately while preserving the original repository.
- Local installer lifecycle: the current internal debug build produced a 27.7 MB NSIS installer, silently installed it to an isolated temporary directory, verified the app and both bundled engines, kept the app running through the smoke window, silently uninstalled it, and left no process or install directory behind.
- Fresh release build: blocked locally when Windows Code Integrity rejects Cargo build-script executables with `os error 4551`. CI now builds a release-mode desktop binary, and the release workflow builds, silently installs, launches, checks bundled engines, and removes an unuploaded local installer on an allowed runner.
- Manual bounded projection matrix: 10k, 50k, and 100k inventory inputs completed without dangling edges.

## Codex Implementer Prompt

```text
Read docs/plans/product-trust-ux-completion.md. Implement the next incomplete step only with the smallest safe patch. Reuse codeInventoryDefaultRoute and existing truth classes. Do not add dependencies or fabricate data. Run the smallest relevant checks and report changed files, skipped work, and the next step.
```

## Codex Reviewer Prompt

```text
You did not write this code. Review the current diff only against docs/plans/product-trust-ux-completion.md. Do not modify files. Findings first. Focus on stale data being blessed as fresh, source fingerprint false negatives, API identity regressions, misleading readiness copy, accessibility regressions, and avoidable complexity. If there are no issues, say so clearly.
```
