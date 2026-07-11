# Backend Visual Map Phase 61 Report

## Summary

Normalized candidate confidence display around Korean high/medium/low labels and removed raw confidence strings from the primary canvas/inspector UI.

## Changed Files

- `src/confidence.ts`
- `src/components/WorkbenchCanvas.tsx`
- `src/components/workbench/InspectorPanel.tsx`
- `src/styles/workbench.css`
- `docs/plans/backend-visual-map-product-completion.md`

## Results

- Candidate edge labels now show `높음`, `보통`, or `낮음`.
- Inspector confidence badges use the same normalized labels and tones.
- Numeric score-like values are bucketed before display, so primary UI does not become score-first.
- Inspector now includes a simple confidence reason for candidate relationships.

## Checks

- `npm run typecheck`: passed
- `npm run build`: passed
- `npm run tauri dev`: passed runtime smoke with `.app-shell` mounted

## Skipped Work

- `cargo test`: skipped because this phase did not touch Rust source.
- Raw/debug score detail: not added; primary UI intentionally stays label-first.
