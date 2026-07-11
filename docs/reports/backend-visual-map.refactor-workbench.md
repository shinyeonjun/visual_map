# Backend Visual Map Refactor: Workbench Split

Date: 2026-07-06

## Changed Files

- `src/components/WorkbenchView.tsx`
- `src/components/WorkbenchChrome.tsx`
- `src/components/workbench/WorkbenchView.tsx`
- `src/components/workbench/WorkbenchTopBar.tsx`
- `src/components/workbench/WorkbenchRail.tsx`
- `src/components/workbench/WorkbenchLeftPanel.tsx`
- `src/components/workbench/WorkspaceSection.tsx`
- `src/components/workbench/CodeSourceSection.tsx`
- `src/components/workbench/DatabaseSourceSection.tsx`
- `src/components/workbench/WorkbenchRightPanel.tsx`
- `src/components/workbench/ModePanel.tsx`
- `src/components/workbench/InspectorPanel.tsx`
- `src/components/workbench/WorkbenchStatusBar.tsx`
- `src/components/workbench/workbenchModes.ts`

## Results

- Split the oversized Workbench view into small render-only components.
- Kept existing class names, copy, props, and behavior.
- Left root `WorkbenchView.tsx` and `WorkbenchChrome.tsx` as compatibility re-exports.

## Checks

- `npm run typecheck` passed.
- `npm run build` passed.
- `cargo test` passed: 40 tests.

## Skipped

- Atlas split, CSS split, shared component moves, type cleanup, and Rust splits were not included in this step.
- No feature logic, engine behavior, DB access, or graph behavior changed.
