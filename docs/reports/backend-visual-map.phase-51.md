# Backend Visual Map Phase 51

Date: 2026-07-06

## Changed Files

- `src/types/projection.ts`
- `src/types/visualMap.ts`
- `docs/plans/backend-visual-map-product-completion.md`

## Results

- Added an app-owned projection contract separate from raw inventory and sidecar output.
- Defined projection nodes, edges, groups, badges, confidence, evidence, source ids, hidden counts, and warnings.
- Added map mode identifiers: `atlas`, `api-flow`, `table-usage`, `column-impact`, and `search-focus`.
- Added per-mode visible node caps.

## Checks

- `npm run typecheck` passed.
- `npm run build` passed.

## Skipped Work

- Did not implement Phase 52 grouped atlas projection.
- Did not change React Flow rendering behavior yet.
- Did not render raw full graphs directly.
- Did not add DB row-data access.

## Risks

- Existing runtime still consumes the older `VisualMap` shape until Phase 52 starts using the projection contract.
