# Phase 2: Static App Shell Layout

Status: Complete

## Changed Files

- `package.json`
- `package-lock.json`
- `src/App.tsx`
- `src/App.css`
- `src-tauri/tauri.conf.json`
- `docs/plans/backend-visual-map.md`
- `docs/reports/backend-visual-map.phase-2.md`

## What Was Implemented

- Built a static Tauri + React app shell based on `design/ui-concepts`.
- Added top bar, left app rail, Code Source panel, Database Source panel, central visual canvas, right View/View Mode/Inspector panel, and bottom status bar.
- Added Workbench / Atlas segmented switching with static dummy map variants.
- Added dummy route, table, mode, inspector, legend, and status data only.
- Updated the Tauri window target to 1440x900 with minimum dimensions.
- Added `lucide-react` for shell icons.

## Deliberately Skipped

- Real repository scanning.
- DB connection or profile persistence.
- Engine executable execution.
- React Flow or real graph data.
- Tauri commands for workspace, scan, query, or map operations.
- Phase 3 app-local path creation.

## Commands Run

```powershell
npm install lucide-react
npm run typecheck
npm run tauri dev
npm run dev
```

## Check Results

- `npm install lucide-react`: passed. Audited 73 packages, 0 vulnerabilities.
- `npm run typecheck`: passed.
- `npm run tauri dev`: passed compile and launched `target\debug\backend-visual-map.exe`.
- `npm run dev`: Vite launched at `http://localhost:1420/`.

Note: `npm run tauri dev` was stopped with Ctrl-C after successful launch, so the wrapper process ended with `STATUS_CONTROL_C_EXIT`.

## Visual Check

- Compared implementation direction against:
  - `design/ui-concepts/01-workbench-map.png`
  - `design/ui-concepts/02-layered-atlas.png`
  - `design/ui-concepts/Backend Visual Map (standalone).html`
- In-app browser screenshot capture was unavailable in this session (`Browser is not available: iab`), so automated screenshot comparison was blocked.

## Known Risks

- The central canvas is a static DOM mock, not a graph renderer.
- Status bar engine labels are dummy UI text and do not indicate real engine availability.
- Placeholder Tauri icons remain from the scaffold.

## Review Fix

- Updated the implementation plan status to `In Progress (Phases 1-2 implemented)`.
- Changed product name letter spacing from `-0.01em` to `0`.

## UI Polish

- Reworked the static shell against `design/ui-concepts/01-workbench-map.png`, `design/ui-concepts/02-layered-atlas.png`, and `design/ui-concepts/Backend Visual Map (standalone).html`.
- Split the static UI out of `src/App.tsx` into:
  - `src/components/WorkbenchView.tsx`
  - `src/components/AtlasView.tsx`
  - `src/components/ViewSwitch.tsx`
  - `src/components/DevDiagnostics.tsx`
- Kept dummy data only; no repo scan, DB connection, engine execution, React Flow, Rust command, or Phase 5 behavior was added.
- Verified at 1440x900 in the local browser:
  - Workbench map title, table list, and related files fit without clipping.
  - Atlas inspector shows all Confidence Summary rows and the impact button.
  - Atlas left panel shows Shared and database enum/function rows without footer overlap.
  - Dev diagnostics chip stays docked in the status/footer area without covering primary UI.
- Re-ran `npm run typecheck` and `npm run build`; both passed.

## Next Recommended Phase

Phase 3: App Path And Workspace Directory.
