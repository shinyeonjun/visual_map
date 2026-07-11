# Backend Visual Map Phase 52

Date: 2026-07-06

## Changed Files

- `src-tauri/src/atlas/visual_map.rs`
- `src-tauri/src/atlas/tests.rs`
- `src/App.tsx`
- `src/hooks/useVisualMap.ts`
- `src/components/workbench/workbenchModes.ts`
- `src/styles/canvas.css`
- `docs/plans/backend-visual-map-product-completion.md`

## Results

- Default overview now returns grouped architecture atlas nodes instead of raw inventory nodes.
- Routes are grouped by route prefix.
- Code items are grouped by folder and kind.
- DB tables are grouped by schema/default DB group.
- Candidate code-to-DB links remain candidate edges when aggregated between visible groups.
- Default atlas output is capped to 40 group nodes.
- Workbench mode ids now use `atlas` and `search-focus`, with backend compatibility retained for old `explore`.

## Checks

- `cargo fmt` passed.
- `cargo test` passed: 50 tests.
- `npm run typecheck` passed.
- `npm run build` passed.
- `npm run tauri dev` startup smoke passed; stopped with Ctrl-C.
- Meeting-overlay atlas smoke skipped: repo or code engine was missing.

## Skipped Work

- Did not implement Phase 53 layout guardrails beyond the 40-node atlas cap.
- Did not implement API Flow/Table Usage/Column Impact maps.
- Did not render raw full graphs directly.
- Did not add DB row-data access.

## Risks

- Atlas grouping is heuristic and based on route text, source path, item kind, and DB schema until richer sidecar metadata is available.
