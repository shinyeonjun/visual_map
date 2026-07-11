# Backend Visual Map Refactor: Common Components

Date: 2026-07-06

## Changed Files

- `src/App.tsx`
- `src/components/common/PanelHeader.tsx`
- `src/components/common/EngineStatus.tsx`
- `src/components/common/ViewSwitch.tsx`
- `src/components/common/DevDiagnostics.tsx`
- `src/components/workbench/WorkspaceCard.tsx`
- `src/components/PanelHeader.tsx`
- `src/components/EngineStatus.tsx`
- `src/components/ViewSwitch.tsx`
- `src/components/DevDiagnostics.tsx`
- `src/components/WorkspaceCard.tsx`
- `src/components/atlas/AtlasView.tsx`
- `src/components/atlas/AtlasTopBar.tsx`
- `src/components/atlas/AtlasDatabasePanel.tsx`
- `src/components/workbench/CodeSourceSection.tsx`
- `src/components/workbench/DatabaseSourceSection.tsx`
- `src/components/workbench/WorkspaceSection.tsx`
- `src/components/workbench/WorkbenchStatusBar.tsx`
- `src/components/workbench/WorkbenchTopBar.tsx`
- `src/components/workbench/WorkbenchView.tsx`

## Results

- Moved shared components to `src/components/common`.
- Moved `WorkspaceCard` to the Workbench component folder.
- Kept root component files as compatibility re-exports.
- Updated direct imports to the new locations.

## Checks

- `npm run typecheck` passed.
- `npm run build` passed.
- `cargo test` passed: 40 tests.

## Skipped

- No behavior, copy, styling, engine, DB, or graph logic changed.
