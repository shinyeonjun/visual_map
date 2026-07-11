# Backend Visual Map Refactor: App Hooks

Date: 2026-07-06

## Scope

- Refactored `src/App.tsx` state and Tauri command orchestration into hooks.
- Preserved existing Workbench and Atlas UI behavior.
- Did not implement workspace/session snapshot auto-restore in this pass.

## Changed Files

- `src/App.tsx`
- `src/hooks/useWorkspaces.ts`
- `src/hooks/useCodeInventory.ts`
- `src/hooks/useDbProfiles.ts`
- `src/hooks/useVisualMap.ts`
- `src/hooks/useEngineRegistry.ts`

## Results

- `App.tsx` now composes hooks and view controls instead of owning workspace, code inventory, DB profile, engine registry, and visual map command flows directly.
- Workspace create/open/list commands moved to `useWorkspaces`.
- Code indexing/inventory load commands moved to `useCodeInventory`.
- DB profile save/index/inventory load commands moved to `useDbProfiles`.
- Visual map load/search/snapshot save flow moved to `useVisualMap`.
- Engine availability lookup moved to `useEngineRegistry`.

## Checks

- `npm run typecheck`: passed
- `npm run build`: passed
- `cargo test`: passed, 40 tests

## Skipped

- Workspace/session/snapshot auto-restore: next priority.
- Workbench/Atlas component folder split: later priority.
- CSS split: later priority.
- Rust `workspace.rs` split: later priority.
- codebase-memory graph reindex: unavailable in this turn; codebase-memory tools were not exposed by tool discovery.
