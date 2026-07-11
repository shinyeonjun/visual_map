# Backend Visual Map Product Completion Plan

Status: Hold (Phases 41-75 implemented; release blockers remain)
Scale: Large
Date: 2026-07-06

## Purpose

This plan continues after Phases 1-40 and the refactor pass. It is not an MVP plan. It is the path from "the app can call the engines and show something" to "the app is a credible local desktop product for understanding backend systems visually."

Source of truth:

- `docs/plans/backend-visual-map-final-product.md`

## Product Target

Backend Visual Map must become a backend map, not a raw graph viewer.

The app reads:

- a Git repository through the code engine
- RDB metadata through the database engine

Then it turns those inventories into visual maps for these questions:

- What is the overall backend architecture?
- What happens when this API is called?
- Where is this table used?
- What can break if this column changes?
- Where is this keyword across API, code, and DB?

## Hard Rules

- Do not render the full raw code graph directly.
- Default maps must be grouped and readable.
- Default visible node count should stay under 40.
- Do not read DB row data.
- Do not add a SQL console.
- Do not persist DB passwords or full secret connection strings.
- Do not auto-register MCP servers into Codex, Claude, or other AI tools.
- Treat `codebase-memory-mcp.exe` and `database-memory.exe` as internal sidecar engines only.
- Keep code-to-DB links as candidates unless there is direct evidence.
- Show confidence and evidence for candidate links.
- Prefer small patches per phase.
- Write a report for every phase.

## Current Baseline

Already complete:

- Tauri v2 + React desktop shell
- workspace storage
- sidecar engine detection
- code indexing command path
- DB indexing command path
- RDB sources: SQLite, SQLite DDL, PostgreSQL, MySQL/MariaDB, SQL Server, Oracle
- basic inventory loading
- basic Workbench and Atlas views
- Korean UI direction
- refactor pass for Workbench, Atlas, CSS, common components, types, Rust workspace, Rust atlas

Known gaps:

- Atlas to Workbench navigation can break.
- Loading/progress/error UX is not reliable enough.
- Large code graphs can still appear as a one-line/raw graph.
- GitHub URL clone flow is not supported yet.
- Workspace restore and snapshot validity need stronger rules.
- Product maps are not yet strong enough for large projects.
- Release packaging with sidecars is not complete.

## Phase Discipline

For every phase:

- Implement only that phase.
- Do not pull later phases forward.
- Run the listed checks.
- If checks fail, stop and fix before moving on.
- Write `docs/reports/backend-visual-map.phase-<N>.md`.
- Include changed files, checks, results, skipped work, and known risks.

Use direct file search and tests for implementation review. Do not depend on a running codebase-memory MCP server during agent work.

## Phase 41: Navigation And View-State Stabilization

Goal:

- Make Workbench/Atlas switching reliable.

Tasks:

- Make one source of truth for the current top-level view.
- Fix switching from Atlas back to Workbench.
- Preserve selected workspace, selected DB profile, and loaded snapshots when switching.
- Make unavailable view states explicit instead of silently clearing inventories.
- Add a regression note or test around view switching.

Done when:

- Workbench -> Atlas -> Workbench works repeatedly without losing loaded state.
- Reloading the app restores the expected view or falls back safely.

Checks:

- `npm run typecheck`
- `npm run build`
- manual `npm run tauri dev` navigation smoke

## Phase 42: Workspace Onboarding UX

Goal:

- Make creating the first workspace obvious.

Tasks:

- Add folder picker flow for local repository path.
- Improve workspace create form validation.
- Show path errors before engine calls.
- Make empty workspace state explain the next action.
- Keep the UI Korean-first.

Done when:

- A new user can create a workspace with the folder picker without reading docs.

Checks:

- `npm run typecheck`
- `npm run build`
- manual first-run smoke

## Phase 43: Operation State Model

Goal:

- Every long operation must have visible state.

Tasks:

- Add a shared operation status shape for indexing/loading/cloning.
- Show pending/running/success/error states in Workbench and status bar.
- Replace raw backend errors with user-readable messages plus expandable details.
- Keep logs redacted.

Done when:

- Code indexing, DB indexing, inventory load, and map generation all show progress or a clear final state.

Checks:

- `npm run typecheck`
- `npm run build`
- `cargo test`

## Phase 44: Per-Workspace Engine Cache Isolation

Goal:

- Avoid collisions with global MCP/server caches and stale indexes.

Tasks:

- Run the code engine with workspace-scoped cache directories.
- Keep DB engine cache/snapshot outputs workspace-scoped.
- Store engine cache metadata in workspace state.
- Never reuse the global codebase-memory cache by default.
- Add tests for cache path derivation.

Done when:

- Two workspaces can index different repos without sharing code graph cache files.

Checks:

- `cargo test`
- `npm run typecheck`
- engine path smoke if binaries are available

## Phase 45: Engine Runtime Contract Hardening

Goal:

- Make sidecar calls predictable and product-safe.

Tasks:

- Centralize sidecar argument building.
- Centralize timeout handling.
- Centralize stdout/stderr redaction.
- Show version/availability per engine.
- Confirm the app never runs installer scripts or auto-registers MCP config.

Done when:

- Engine calls have consistent logs, errors, timeout behavior, and redaction.

Checks:

- `cargo test`
- `npm run typecheck`
- missing-engine smoke

## Phase 46: GitHub URL Clone Flow

Goal:

- Let users start from a GitHub repo URL.

Tasks:

- Add "local folder" vs "GitHub URL" source mode.
- Clone public GitHub URLs into an app-managed workspace repo directory.
- Show clone progress and clone errors.
- Do not handle private auth in v1 unless already available through local git.
- Prevent overwriting existing workspace repos.

Done when:

- Pasting a public GitHub URL can create a workspace and then index the cloned repo.

Checks:

- `npm run typecheck`
- `npm run build`
- manual public repo clone smoke

## Phase 47: Snapshot Lifecycle And Staleness

Goal:

- Make inventory snapshots reliable and explain stale data.

Tasks:

- Store snapshot timestamps, engine versions, source path, and source type.
- Mark code snapshots stale when repo path changes.
- Mark DB snapshots stale when DB profile/source changes.
- Do not silently render fake or old snapshots as current.
- Add restore behavior after app restart.

Done when:

- Users can tell whether they are seeing fresh, stale, missing, or failed snapshot data.

Checks:

- `cargo test`
- `npm run typecheck`
- restart smoke

## Phase 48: Database Profile UX V1

Goal:

- Make DB source setup understandable for all supported RDB sources.

Tasks:

- Use source-specific forms for SQLite, SQLite DDL, PostgreSQL, MySQL/MariaDB, SQL Server, and Oracle.
- Keep network DB secrets session-only.
- Show required inputs per source type.
- Explain metadata-only access.
- Validate obvious missing fields before running the engine.

Done when:

- A user can choose the right DB source without guessing which input matters.

Checks:

- `npm run typecheck`
- `npm run build`

## Phase 49: DB Connection And Metadata Smoke UX

Goal:

- Make DB indexing feel safe and inspectable.

Tasks:

- Add "test metadata connection" behavior when supported.
- Distinguish connection failure, auth failure, missing driver, and metadata parse failure.
- Show table/column counts after successful indexing.
- Keep all DB operation logs redacted.

Done when:

- DB indexing failure tells the user what to fix next.

Checks:

- `cargo test`
- `npm run typecheck`
- SQLite DDL smoke
- PostgreSQL smoke when env is available

## Phase 50: Code Inventory Normalization

Goal:

- Convert code engine output into product concepts.

Tasks:

- Normalize routes, handlers, services, repositories, files, functions, classes, and modules.
- Keep unknown code nodes grouped as "code" instead of dropping them.
- Preserve source file and line evidence.
- Add count summaries by category.

Done when:

- Code inventory has stable categories usable by maps and side panels.

Checks:

- `npm run typecheck`
- `cargo test` if normalization is Rust-side
- meeting-overlay code inventory smoke

## Phase 51: Visual Projection Contract

Goal:

- Define the app-owned map model between raw inventory and React Flow.

Tasks:

- Define projection nodes, edges, groups, badges, confidence, evidence, and source ids.
- Keep projection contract separate from raw engine output.
- Add map mode identifiers: atlas, api-flow, table-usage, column-impact, search-focus.
- Add caps for visible nodes and hidden counts.

Done when:

- Each map mode can render from the same projection contract.

Checks:

- `npm run typecheck`
- `npm run build`

## Phase 52: Architecture Atlas Projection

Goal:

- Replace raw graph rendering with grouped architecture atlas.

Tasks:

- Group routes by route prefix/domain.
- Group code by folder/domain/layer.
- Group DB by schema/table group.
- Show counts for collapsed groups.
- Keep default visible nodes under 40.

Done when:

- Large repos open to a readable atlas, not a one-line raw graph.

Checks:

- `npm run typecheck`
- `npm run build`
- meeting-overlay atlas smoke

## Phase 53: Layout Guardrails

Goal:

- Prevent unreadable one-line or giant maps.

Tasks:

- Add deterministic layer layout for grouped maps.
- Add node caps per map mode.
- Add "narrow focus" state when results are too broad.
- Make auto-layout operate on projection groups, not raw nodes.
- Keep labels readable at 1440x900.

Done when:

- Large projects produce grouped, layered layouts with no giant single-line chain.

Checks:

- `npm run typecheck`
- `npm run build`
- screenshot QA note in report

## Phase 54: Canvas State UX

Goal:

- Make the center canvas trustworthy in every state.

Tasks:

- Add explicit empty, loading, error, stale, and loaded states.
- Add clear primary actions in empty states.
- Avoid raw error text on the canvas.
- Keep status bar and canvas messages consistent.

Done when:

- The canvas never looks broken when data is missing or work is running.

Checks:

- `npm run typecheck`
- `npm run build`

## Phase 55: API Flow Map

Goal:

- Show route-to-code-to-DB candidate flow for one API.

Tasks:

- Let users select an API route.
- Build a focused 1-2 hop map around the route.
- Show confirmed code relations separately from candidate DB links.
- Show evidence for candidate table/column links.

Done when:

- Selecting a route produces a readable API flow map.

Checks:

- `npm run typecheck`
- `npm run build`
- meeting-overlay API route smoke

## Phase 56: Table Tree And Table Detail Map

Status: Complete

Goal:

- Make DB schema exploration useful before candidate links are perfect.

Tasks:

- Improve schema/table/column tree.
- Show table columns, PK/FK/indexes, and related tables.
- Render a table detail map from confirmed DB metadata only.
- Keep DB internal relationships visually distinct from code candidates.

Done when:

- A DB-only workspace can still produce a useful schema map.

Checks:

- `npm run typecheck`
- SQLite DDL smoke
- PostgreSQL smoke when env is available

## Phase 57: Table Usage Candidate Map

Status: Complete

Goal:

- Show where a table is probably used in code.

Tasks:

- Link table names to candidate files/functions/routes using existing evidence.
- Rank candidates by confidence.
- Display high/medium/low confidence.
- Make candidate edges dashed.

Done when:

- Selecting a table shows confirmed DB details plus candidate code usage.

Checks:

- `npm run typecheck`
- `npm run build`
- meeting-overlay table usage smoke

## Phase 58: Column Impact Map

Status: Complete

Goal:

- Help users reason about column changes.

Tasks:

- Show column -> constraints/FKs/indexes -> related tables.
- Add candidate code references for the selected column.
- Show impact summary in the inspector.
- Separate direct DB impact from candidate code impact.

Done when:

- Selecting a column answers "what can break if this changes?"

Checks:

- `npm run typecheck`
- SQLite DDL smoke
- PostgreSQL smoke when env is available

## Phase 59: Search Focus Map

Status: Complete

Goal:

- Turn search into a map, not a list only.

Tasks:

- Search API, code, files, tables, and columns together.
- Group search results by type.
- Selecting a result creates a local focus map.
- Broad search asks the user to narrow focus.

Done when:

- Searching "session" can jump to a focused visual map.

Checks:

- `npm run typecheck`
- `npm run build`
- meeting-overlay search smoke

## Phase 60: Evidence Inspector

Status: Complete

Goal:

- Make every displayed relationship explainable.

Tasks:

- Show source type: confirmed, inferred, candidate.
- Show evidence snippets/paths/lines where available.
- Show confidence and reason for candidate links.
- Add copy buttons for path, symbol, table, column, and route.

Done when:

- A user can inspect why any non-obvious edge exists.

Checks:

- `npm run typecheck`
- `npm run build`

## Phase 61: Confidence Model

Status: Complete

Goal:

- Make inferred links simple and consistent.

Tasks:

- Normalize confidence to high/medium/low.
- Define confidence reasons.
- Avoid numeric scores in primary UI.
- Keep detailed scores only in inspector/debug detail.

Done when:

- Candidate links are understandable without reading raw JSON.

Checks:

- `npm run typecheck`
- `npm run build`

## Phase 62: Large Project Performance Guardrails

Status: Complete

Goal:

- Keep UI responsive on large repos.

Tasks:

- Cap projection size before rendering.
- Avoid expensive render loops.
- Memoize heavy map transforms.
- Add large-result warnings.
- Keep broad maps grouped by default.

Done when:

- A large project does not freeze the UI or generate unreadable maps.

Checks:

- `npm run typecheck`
- `npm run build`
- large fixture or meeting-overlay stress smoke

## Phase 63: Canvas Interaction Polish

Status: Complete

Goal:

- Make the visual map feel like a tool, not a screenshot.

Tasks:

- Polish zoom, fit view, minimap, selection, pan, and reset.
- Ensure node click and edge click update inspector.
- Keep keyboard shortcuts discoverable but not noisy.
- Keep all controls Korean-first.

Done when:

- Users can navigate maps comfortably at 1440x900.

Checks:

- `npm run typecheck`
- `npm run build`
- screenshot QA note

## Phase 64: Workbench Information Architecture

Status: Complete

Goal:

- Make left rail actions match the user's mental model.

Tasks:

- Separate workspace setup, code source, DB source, and inventory browsing.
- Reduce repeated buttons.
- Keep "index", "load", and "view map" actions distinct.
- Show latest successful snapshot time.

Done when:

- A user can understand what to do next without guessing.

Checks:

- `npm run typecheck`
- `npm run build`
- manual first-run smoke

## Phase 65: Atlas Information Architecture

Status: Complete

Goal:

- Make Atlas a read-only exploration space, not another setup panel.

Tasks:

- Keep setup actions in Workbench.
- Make Atlas focus on architecture, dependencies, impact, schema, and API modes.
- Fix sidebar modes so each one changes the map or explains why unavailable.
- Add reliable return to Workbench.

Done when:

- Atlas is clearly for exploration after data exists.

Checks:

- `npm run typecheck`
- `npm run build`
- Workbench/Atlas navigation smoke

## Phase 66: Meeting Overlay Product Smoke

Status: Complete

Goal:

- Prove the product on the real target repo.

Tasks:

- Use `meeting-overlay-assistant` as a real code smoke.
- Load at least one real DB metadata source for the same workspace.
- Produce atlas, API flow, table usage, and column impact screenshots or notes.
- Record bugs found and whether fixed or deferred.

Done when:

- The real project can be loaded end-to-end with useful maps.

Checks:

- `npm run typecheck`
- `npm run build`
- `cargo test`
- meeting-overlay smoke report

## Phase 67: RDB Smoke Matrix

Status: Complete

Goal:

- Verify supported DB sources at product level.

Tasks:

- Always test SQLite DDL.
- Always test PostgreSQL when local env is available.
- Gate MySQL/MariaDB, SQL Server, and Oracle behind env/availability.
- Record skips clearly.
- Confirm metadata-only behavior.

Done when:

- Product smoke matrix is honest and reproducible.

Checks:

- smoke scripts
- `cargo test`
- report with pass/skip/fail matrix

## Phase 68: Korean Copy Completion

Status: Complete

Goal:

- Make the product consistently Korean-first.

Tasks:

- Audit visible UI text.
- Translate mixed English labels where appropriate.
- Keep technical terms only where they help.
- Make empty/error/loading text concise and useful.

Done when:

- Main user flows no longer feel half-translated.

Checks:

- `npm run typecheck`
- `npm run build`
- screenshot QA note

## Phase 69: Security And Privacy Audit

Status: Complete

Goal:

- Make local-product trust explicit.

Tasks:

- Audit persisted workspace files.
- Audit logs and reports for secrets.
- Confirm no DB rows are read.
- Confirm no password persistence.
- Confirm no auto MCP registration.
- Confirm no unexpected network calls except clone/user-selected DB.

Done when:

- Security/privacy report can be shown to users.

Checks:

- `cargo test`
- manual persistence audit
- security report

## Phase 70: Sidecar Packaging

Status: Complete

Goal:

- Bundle engines so users do not need PATH setup.

Tasks:

- Package `codebase-memory-mcp.exe` as internal code engine.
- Package `database-memory.exe` as internal DB engine.
- Add license notices.
- Verify dev and release engine lookup.
- Do not run installer scripts.

Done when:

- Fresh app install can find both bundled engines.

Checks:

- `cargo test`
- release build smoke
- missing-engine smoke

## Phase 71: Windows Installer Smoke

Status: Complete

Goal:

- Prove the app works like software, not a dev project.

Tasks:

- Build Windows installer.
- Install on a clean-ish local profile if possible.
- Run without Rust, Node, or PATH assumptions.
- Create workspace, index code, index DB metadata.
- Uninstall and verify no surprising global config changes.

Done when:

- A normal Windows user can install and run the app.

Checks:

- installer smoke report
- release build logs

## Phase 72: Screenshot QA

Status: Complete

Goal:

- Catch visual regressions before release.

Tasks:

- Capture 1440x900 Workbench empty state.
- Capture 1440x900 Workbench loaded state.
- Capture 1440x900 Atlas grouped map.
- Capture 1440x900 API Flow/Table Usage/Column Impact if available.
- Check clipping, overlap, unreadable labels, and broken states.

Done when:

- Screenshots look credible for a final product demo.

Checks:

- screenshot files or report references
- `npm run build`

## Phase 73: Demo Workspace And Exhibition Story

Status: Complete

Goal:

- Make the product demo easy to understand in 3 minutes.

Tasks:

- Prepare a demo workspace or repeatable demo script.
- Choose one API story.
- Choose one table usage story.
- Choose one column impact story.
- Write short demo notes.

Done when:

- A viewer can understand the product without backend context.

Checks:

- demo report
- screenshots

## Phase 74: Documentation Finalization

Status: Complete

Goal:

- Make install/use/troubleshooting docs complete.

Tasks:

- Update README quickstart.
- Add engine/sidecar explanation.
- Add DB metadata-only explanation.
- Add troubleshooting for missing engine, failed clone, failed DB connection, stale snapshots.
- Add limitations.

Done when:

- A new user can install, create a workspace, index code/DB, and understand limitations from docs.

Checks:

- docs walkthrough

## Phase 75: Release Candidate Review

Status: Complete

Goal:

- Freeze v1 release readiness.

Tasks:

- Review all phase reports from 41-74.
- Verify required checks.
- Verify product target criteria.
- List release blockers and non-blocking known issues.
- Mark the product completion plan complete only if blockers are gone.

Done when:

- There is a clear release candidate decision: ship, hold, or cut scope.

Checks:

- `npm run typecheck`
- `npm run build`
- `cargo test`
- real code engine smoke
- real DB engine smoke for SQLite DDL and PostgreSQL
- screenshot QA
- final release checklist

## Deferred After V1

These are valuable, but should not block v1:

- PR Impact map
- Migration Risk map
- Test Scope map
- Architecture Drift over time
- Team shared map/cloud sync
- Private GitHub auth UX
- GitHub PR integration
- Figma/exportable architecture diagrams

## Implementation Prompt

Use this for each phase:

```text
Read D:\project\backend_map\docs\plans\backend-visual-map-final-product.md and D:\project\backend_map\docs\plans\backend-visual-map-product-completion.md.

Implement Phase <N> only.
Do not implement later phases early.
Use the smallest safe patch.
Do not use codebase-memory MCP tools for agent-side code discovery.
Do not render raw full graphs directly.
Do not add DB row-data access.
Do not persist DB passwords.
Keep code-to-DB links as candidates unless direct evidence proves them.
Run the phase checks.
Write D:\project\backend_map\docs\reports\backend-visual-map.phase-<N>.md with changed files, checks, results, skipped work, and risks.
Stop if checks fail.
```

## Review Prompt

Use this after a phase:

```text
Review the current diff against Phase <N> in D:\project\backend_map\docs\plans\backend-visual-map-product-completion.md.
Do not implement new features.
Findings first, ordered by severity.
Focus on bugs, regressions, missing tests, secret leakage, row-data access, raw graph rendering, UI state loss, and phase-scope creep.
```
