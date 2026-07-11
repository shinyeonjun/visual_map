# Backend Visual Map Refactor: Type Files

Date: 2026-07-06

## Changed Files

- `src/types/visual-map.ts`
- `src/types/controls.ts`
- `src/types/visualMap.ts`
- `src/types/visualMapControls.ts`
- `src/types/workspaceControls.ts`
- Type imports across `src/**/*.ts` and `src/**/*.tsx`

## Results

- Moved visual map types to `visual-map.ts`.
- Consolidated workspace, DB profile, and visual map control types into `controls.ts`.
- Kept the previous type files as compatibility re-exports.
- Updated direct app imports to the new files.

## Checks

- `npm run typecheck` passed.
- `npm run build` passed.
- `cargo test` passed: 40 tests.

## Skipped

- No runtime behavior, component rendering, engine behavior, DB access, or graph logic changed.
