# Backend Visual Map Refactor: Atlas Split

Date: 2026-07-06

## Changed Files

- `src/components/AtlasView.tsx`
- `src/components/AtlasCanvas.tsx`
- `src/components/atlas/AtlasView.tsx`
- `src/components/atlas/AtlasTopBar.tsx`
- `src/components/atlas/AtlasLeftPanel.tsx`
- `src/components/atlas/AtlasRepositoryPanel.tsx`
- `src/components/atlas/AtlasDatabasePanel.tsx`
- `src/components/atlas/AtlasRightPanel.tsx`
- `src/components/atlas/AtlasModeList.tsx`
- `src/components/atlas/AtlasCanvas.tsx`
- `src/components/atlas/atlasModes.ts`

## Results

- Split the Atlas view into top bar, left panel, repository panel, database panel, right panel, mode list, and canvas files.
- Kept root `AtlasView.tsx` and `AtlasCanvas.tsx` as compatibility re-exports.
- Kept existing class names, copy, props, and behavior.

## Checks

- `npm run typecheck` passed.
- `npm run build` passed.
- `cargo test` passed: 40 tests.

## Skipped

- CSS split, shared component moves, type cleanup, and Rust splits were not included in this step.
- No feature logic, engine behavior, DB access, or graph behavior changed.
