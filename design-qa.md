# Design QA: Guided Workspace and Connection Map

## Review target

- Source of visual truth: `design/ui-concepts/selected-direction-01-guided-workspace.png`
- API connection-map target: `design/ui-concepts/target-connection-map-api-path.png`
- API implementation capture: `design/ui-concepts/implemented-connection-map-api-path.png`
- API side-by-side comparison: `design/ui-concepts/comparison-connection-map-api-path.png`
- API responsive captures: `design/ui-concepts/implemented-connection-map-api-path-1180.png`, `design/ui-concepts/implemented-connection-map-api-path-820.png`
- Final static-shell captures: `design/ui-concepts/qa-static-shell-1180.png`, `design/ui-concepts/qa-static-shell-820.png`, `design/ui-concepts/qa-static-shell-context-820.png`
- Reference state crop: `.codex/design-qa-source-setup.png`
- Final implementation capture: `.codex/redesign-inline-setup-final.png`
- Side-by-side comparison: `.codex/design-qa-comparison-setup.png`
- Primary review viewport: 1280 x 720
- Minimum supported viewport checked: 1180 x 760
- Primary state: no workspace, source setup visible inline
- Supplementary native states: overview, domain detail, API path, code focus, table usage, column impact, source manager, global search

## Fidelity review

### Typography

- Product identity, navigation labels, section hierarchy, status labels, and compact metadata follow the reference's dense developer-tool scale.
- Large marketing-style headings were avoided; the largest page title is reserved for the first-run task.
- Monospace is limited to paths, symbols, routes, and database identifiers.

### Spacing and layout

- The fixed left navigation, stable top bar, central work surface, and conditional evidence panel match the reference structure.
- Main navigation does not move between modes.
- Overview density is capped at seven domain rows, with explicit expansion for the remainder.
- At 1180 x 760 there is no document-level horizontal overflow, top-bar collision, or clipped setup control.

### Color and tokens

- Neutral surfaces, cobalt navigation/action color, emerald database/source state, and amber review state match the selected direction.
- Color is never the only status signal; every state includes a text label or icon.
- Decorative color effects were removed. The canvas dot grid is retained only as a spatial work-surface cue.

### Image and icon fidelity

- The product is an information workspace and requires no photographic or illustrative assets.
- Existing `lucide-react` icons are used consistently for navigation, sources, actions, status, and disclosure controls.
- No text symbols, handcrafted SVGs, or placeholder artwork are used as UI assets.

### Copy and content

- All project names, routes, functions, tables, counts, confidence, freshness, and evidence come from actual application state.
- Missing data renders as an honest empty or locked state; no fake fallback repository, count, path, reviewer, confidence score, or database object is shown.
- Table usage copy distinguishes confirmed schema facts from code candidates and does not claim read/write behavior the engine cannot prove.

## Interaction verification

- Fixed navigation: overview, API, code, database, and change impact.
- Overview: top-domain scan, expand-all control, domain drill-down, and evidence-on-selection.
- API: compact route switching, confirmed path selection, candidate separation, branch reveal, and three working views (`연결 지도`, `계층`, `목록`).
- Code: focused symbol view with related evidence and without crossing-line overload.
- Database: table usage and column impact views.
- Global search: `Ctrl+K`, result list, code/API/table routing, and query reset after selection.
- Source management: open, close, Escape dismissal, initial focus, Tab focus loop, focus restoration, and one trusted maintenance action (`다시 읽기`).
- Mode changes: the shell stays fixed and the last committed canvas/evidence remains visible but non-interactive until the requested projection is ready; the left criterion, center subject, and inspector then commit together.
- Target consistency: the active left item, center focus strip/card, and right evidence subject resolve from one map focus; targets outside the first 100 items are pinned first.
- Compact inspector: changing modes or targets keeps the central answer visible; the evidence overlay opens after an explicit target, card, node, or relation selection.
- Compact navigation: the explicit `항목` control opens the API/code/table/column context panel; current target pinning, filtering, close control, focus restoration, and Escape dismissal were exercised at 820 px.
- First run: project source is the primary task; database placement remains visible and explains why it is locked.
- Browser console checked after final reload: no errors or warnings.

## Comparison history

### Pass 1 findings and fixes

- P1: Dual shells and permanently dense side panels obscured the primary task. Replaced with one product shell, fixed navigation, a conditional inspector, and a source drawer.
- P1: The overview rendered too many groups and an always-on ledger. Limited the first scan to seven groups and deferred the ledger until selection.
- P1: Database selection could leak into code mode. Code focus now accepts only code inventory targets.
- P2: Initial source setup was hidden behind a drawer. It now appears inline as the first-run main task.
- P2: Source management repeated long code and table inventories. It now shows compact project, code, and database summaries.
- P2: Search used a stale focus selector and retained the previous query. The selector is stable and successful navigation clears the query.

### Pass 2 findings and fixes

- P1: During mode changes, the new navigation state briefly appeared beside the previous canvas and evidence. The previous committed navigation, canvas, and evidence now remain aligned and non-interactive until the next projection commits; only the requested mode receives a pending marker.
- P1: Persisted code selection could disagree across the left context list, center focus strip, and inspector. Current-map focus is now authoritative, and the focused item is pinned in every capped context list.
- P1: Generic Rust projections inserted focus before sorting but could drop it when applying the 32-node cap. The requested focus is now pinned before the cap; the native command returned 32 nodes with the requested `resolve` node included.
- P2: Column containment was counted as direct change impact in the inspector while the review board correctly showed zero. Parent containment is now structural only and excluded from direct impact totals.

### Post-fix evidence

- Final first-run comparison: `.codex/design-qa-comparison-setup.png`
- Final browser state: `.codex/redesign-inline-setup-final.png`
- Native Tauri walkthrough covered all five destinations, source management, and search routing using the current workspace data.
- Minimum-width DOM measurements confirmed stable navigation and no page overflow at 1180 x 760.

## Intentional differences and residual P3 items

- The reference suggests database configuration beside an unattached project. The implementation keeps the database card visible but locked until a workspace owns the connection. This is intentional: it prevents orphaned or misleading connection state while preserving placement predictability.
- The storyboard and responsive browser capture do not share an identical physical canvas size. The hierarchy and fixed regions are preserved as a responsive adaptation.
- The current native workspace has no DB candidate attached to any of its 50 API routes. The final capture therefore stops at the last confirmed code node and explicitly says that DB non-use is not proven; it does not fabricate the target image's `auth_sessions` node. Candidate attachment and source-edge selection are covered by `ApiReadingPath.test.tsx`.
- A minimap is omitted for the bounded primary path. The existing pan, zoom, reset, branch expansion, route list, and alternate views provide navigation without rendering a decorative or misleading miniature.

## API Connection Map QA

### Pass 1 findings and fixes

- P1: The API answer was a five-column review list, so users still had to reconstruct execution order. It now renders one deterministic path selected only from real `code_handle` and `code_call` edges.
- P1: An expected stale snapshot surfaced as a canvas failure on startup. It now keeps real restore failures as errors while presenting source drift as an explicit re-read state.
- P1: DB candidates could visually read like confirmed execution stages. Candidate links now originate from their actual `candidate_uses.from` node and use a distinct dashed amber path; no candidate is shown when none exists.
- P1: The right inspector was constrained by the legacy `52vh` rule and could collapse relation rows. The product-shell inspector now owns the full available height and scrolls its real evidence.
- P1: At 820 px the inspector covered the map with no dismissal path. A responsive close control now clears selection, closes the panel, and selecting a node reopens it.
- P2: API mode had no local route context. The left rail now pins the active route, shows two nearby routes, and expands to the complete real route inventory on demand.
- P2: The design's alternate-view controls risked becoming dead UI. All three controls now switch to actual connection, hierarchy, and ordered-list representations of the same projection.
- P2: Persisted investigation items from unrelated code crowded the contextual API inspector. Source actions remain available, while the global investigation tray stays hidden in this focused inspector.

### Post-fix evidence

- Native Tauri API smoke at 1440 x 1024: primary nodes, selected-route synchronization, all three views, branch expand/collapse, inspector selection, and zero root overflow passed.
- Native Tauri API smoke at 1180 x 760 and 820 x 760 passed. The 820 px inspector close-and-reopen flow was exercised directly through the native WebView CDP endpoint.
- Side-by-side review confirms stable top bar, fixed navigation, question-first header, central path, separated evidence panel, compact nodes, and real-data-only copy.
- Frontend: 64 tests passed across 16 files; TypeScript, Knip dead-code scan, and production build passed.
- Rust: formatter and Clippy passed; 157 tests passed with one manual performance matrix ignored and zero failures.

## Source Management QA

- Removed `구조 테스트` and cached-list reload actions; only real source indexing is exposed after a trusted snapshot is unavailable.
- This prevents stale engine caches from being saved again with a current source fingerprint.
- Removed duplicate code tabs, code/table filters, and 80-row inventory lists from the source drawer. Main mode context panels now own exploration; the drawer owns only connection, counts, failures, and reindexing.
- Native WebView smoke at 1440 x 900 and 820 x 900 verifies stable navigation plus exactly one DB source action.
- Native captures: `design/ui-concepts/source-manager-simplified-final.png`, `design/ui-concepts/source-manager-simplified-820-final.png`.

## Inspector Scroll QA

- P1: The sticky `다음 확인` section could cover the second row of source actions at 1180x760. The inspector now has a fixed header and next-check footer with only `요약` through `소스` in the scroll region.
- The stable-navigation smoke now starts from any persisted mode, chooses a code target through the fixed left context list when needed, scrolls the Explorer source action into view, and verifies its rectangle does not intersect the next-check footer.
- Native Tauri smoke passed at 1180x760 and 820x760. Captures: `design/ui-concepts/qa-inspector-fixed-footer-1180.png`, `design/ui-concepts/qa-inspector-fixed-footer-820.png`.
- Current frontend verification: 16 files and 64 tests passed; production build and Knip passed.

## First-Run And Restore QA

- An isolated empty app-data directory rendered the source-first onboarding with zero project, code, DB, or current-looking fallback values.
- The visible UI flow opened this repository, read 2,268 code symbols and 260 files, saved the bundled DDL fixture, and read 2 tables, 5 columns, and 1 FK.
- Change impact now exercises the intended neutral first-entry state by selecting a real column from the fixed left context list before asserting the four-lane board. The smoke no longer assumes an automatically chosen column.
- Editing the analyzed repository after indexing produced the expected stale-source block. Reindexing and restarting without another source change restored code 2,528, DB 2, `main.orders.id`, and all four impact lanes with no stale warning.
- The isolated `workspace.json` stored `passwordStored: false`; URL-shaped credentials found in analyzed test fixtures were persisted only as `[REDACTED]`.
- Captures: `design/ui-concepts/qa-first-run-empty-1180.png`, `design/ui-concepts/qa-first-run-impact-1180.png`; the restart capture was kept outside the analyzed repository so QA itself could not invalidate the source fingerprint.

## Stable Selection And Lifecycle QA

- The left panel is a fixed analysis-context rail: selecting an item changes the analysis criterion, focused center content, and inspector subject without moving controls or replacing the shell.
- A 20 ms native WebView trace of a same-mode code target change kept the previous target in all three regions while loading, then committed the new target to all three regions together. No contradictory intermediate subject was rendered.
- Database and impact walkthroughs used real fixture data. `main.orders` and `main.orders.user_id` remained synchronized across the left criterion, center review board, and right evidence summary.
- Compact mode labels remain visible at 820 px (`개요`, `API`, `코드`, `DB`, `영향`), and the evidence overlay opens only after an explicit center selection.
- Corrupting the isolated primary `workspace.json` produced a visible backup warning. The `백업 복구` action restored a valid primary document with the same workspace ID and DB profile.
- Deleting the isolated DB connection removed its profile cache and cleared `activeDbProfileId` while preserving the code cache and source repository.
- Deleting the isolated project removed only the app-owned workspace directory. `D:\project\backend_map`, its `package.json`, and its `.git` directory remained intact.
- Native captures for this pass were stored outside the analyzed repository so the QA process could not make the source snapshot stale.

## Static Shell And One-Click Selection QA

- The mode rail and target browser have separate actions. Clicking an active mode is a no-op; compact target browsing opens only from the explicit `항목` control and returns focus there on Escape.
- Cross-mode and same-mode requests keep the last committed mode, target list, canvas, and evidence together until the requested result is ready. The requested mode is exposed as pending without replacing the active criterion early.
- Repeated target clicks during loading replace the pending request with the latest target instead of selecting evidence from the retained result.
- A left target click automatically becomes the center focus and inspector subject after map commit. Re-clicking the active target selects it immediately without another projection request.
- Node and relationship selections are mutually exclusive, preventing a clicked node from leaving stale relationship evidence in the inspector.
- A focused code target with no scoped relationships now displays one central target, explicit incoming/outgoing zero states, and a snapshot-qualified explanation. Unrelated nearby inventory is omitted instead of faded into an implied graph.
- Focused regression coverage passes in `ModePanel.test.tsx`, `useVisualMap.test.ts`, and `StableTransition.test.tsx`.
- The final release-profile app passed stable-navigation smoke at 1180x820 and 820x820, plus Atlas drilldown, API flow, change impact, source jump, and large-repository smoke at 1180x820.

## Source Freshness And DB Scope QA

- Code and DB source cards keep `다시 읽기` visible in the ready state; connection details stay collapsed. A regression test locks both interactions.
- An isolated native DDL flow moved from five to seven columns across two real file edits, surfaced `오래됨` on window focus, and returned the exact `main.orders.updated_at` inventory hit after re-reading.
- Adapter capability limits are separated from project-specific metadata gaps. The overview remains focused, coverage shows `기록된 누락 0 · 지원 제한 6`, and change impact groups the six limitations into one evidence-backed item.
- Current verification: 20 frontend files and 98 tests passed; 175 Rust tests passed with one manual scale benchmark ignored; TypeScript, production build, Knip, formatting, and Clippy passed.
- The latest internal installer smoke redirects application data and WebView2 user data into the disposable install root, verifies both directories were actually used, and fails if cleanup leaves the root behind.
- Native captures: `design/ui-concepts/qa-db-stale-proof-fresh.png`, `design/ui-concepts/qa-db-stale-reread-action.png`, `design/ui-concepts/qa-db-stale-reread-complete.png`, and `design/ui-concepts/qa-db-capability-separated.png`.

## Result

No unresolved P0, P1, or P2 visual, interaction, accessibility, or trust issue remains in the reviewed scope.

final result: passed
