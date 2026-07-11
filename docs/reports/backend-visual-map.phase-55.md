# Backend Visual Map Phase 55

Date: 2026-07-06

## Changed Files

- `src-tauri/src/atlas/visual_map.rs`
- `src-tauri/src/atlas/tests.rs`
- `src/styles/canvas.css`
- `docs/plans/backend-visual-map-product-completion.md`

## Results

- Added a focused `api-flow` map path for selected API routes.
- Route focus maps include related code items by route/domain token and candidate DB table/column links.
- Candidate code-to-DB links remain dashed `candidate_uses` edges with high/medium confidence and evidence.
- Added separate `code_flow` edges for route-to-code flow so they are visually distinct from candidate DB links.
- Focused API maps stay under the configured cap and warn when truncated.

## Checks

- `cargo fmt` passed.
- `cargo test` initially failed on a helper type mismatch; fixed the iterator mapping.
- `cargo test` passed after the fix: 52 tests.
- `npm run typecheck` passed.
- `npm run build` passed.
- Meeting-overlay API route smoke skipped: repo or code engine was missing.

## Skipped Work

- Did not implement Phase 56 table detail work.
- Did not add new engine calls.
- Did not render raw full graphs directly.
- Did not add DB row-data access.

## Risks

- Route-to-code flow currently uses route/domain token matching until the sidecar exposes stronger call relation evidence in the loaded snapshot.
