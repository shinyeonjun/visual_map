# Design QA — canvas-first desktop redesign

- source visual truth: `C:\Users\plosind\AppData\Local\Temp\codex-clipboard-a7a79748-c9df-458d-9a24-039e874ef8db.png` (CodeSee canvas composition)
- focused source visual truth: `C:\Users\plosind\AppData\Local\Temp\codex-clipboard-de360ef4-6b75-4c25-b267-1858bc4c2ceb.png` (Visual Map hierarchy and evidence language)
- overview implementation: `C:\Users\plosind\.codex\visualizations\2026\08\03\019fc4ed-db8e-77d2-8ae8-eb081612ee9a\map-final-overview-v2.png`
- focused implementation: `C:\Users\plosind\.codex\visualizations\2026\08\03\019fc4ed-db8e-77d2-8ae8-eb081612ee9a\map-code-target-drilldown.png`
- minimum-width implementation: `C:\Users\plosind\.codex\visualizations\2026\08\03\019fc4ed-db8e-77d2-8ae8-eb081612ee9a\map-responsive-820-after.png`
- full-view comparison: `C:\Users\plosind\.codex\visualizations\2026\08\03\019fc4ed-db8e-77d2-8ae8-eb081612ee9a\design-qa-comparison.png`
- focused comparison: `C:\Users\plosind\.codex\visualizations\2026\08\03\019fc4ed-db8e-77d2-8ae8-eb081612ee9a\design-qa-focused-comparison.png`
- viewport: `1440 x 900` CSS px; responsive gate also checked at the configured minimum `820 x 760`
- source pixels: `1209 x 675` and `1487 x 1058`; implementation pixels: `1440 x 900`
- density normalization: device scale factor `1`; the overview source was scaled to `1440 x 804` and vertically padded to `1440 x 900`; the focused source was scaled to `1265 x 900` and horizontally padded to `1440 x 900`
- state: populated `plane` workspace, light desktop theme; overview with Layers open, then `전체 프로젝트 → Plane → module` with evidence open

## Findings

No actionable P0/P1/P2 findings remain.

- Fonts and typography: the Korean UI uses one restrained system stack with clear 12/14/16px hierarchy; labels truncate instead of widening cards. Monospace remains limited to paths, IDs, counts, and code evidence.
- Spacing and layout rhythm: the canvas occupies the entire workspace under the 52px top bar. The 48px tool rail and floating panels do not resize or relocate map nodes. At `820px`, the Layers and Evidence panels automatically alternate so the canvas remains usable.
- Colors and tokens: neutral white/blue canvas tokens match the selected references; API/code/data and confirmed/candidate semantics stay distinct without inventing runtime telemetry colors.
- Image and icon fidelity: this is a native desktop tool with no raster content in the target surface. Visible controls use the installed Lucide icon family consistently; no placeholder illustration, custom SVG, or CSS illustration substitutes are present.
- Copy and content: labels describe real static-analysis data. Missing relations are stated as missing rather than filled with fake latency, traffic, quality, or health numbers.
- Layout and behavior: left API/code selection preserves the same canvas, opens the matching package and module in place, and opens evidence only after selection. Closing either floating panel reveals the canvas immediately.
- Accessibility: semantic buttons, labelled regions, focus styles, `aria-pressed`, `aria-expanded`, live status, and keyboard-search behavior are present. There is no root horizontal or vertical overflow at `1440 x 900`, `1024 x 768`, or `820 x 760`.

## Focused comparison evidence

The focused composite checks the two high-information regions at readable scale: nested package/module cards and the evidence inspector. The implementation deliberately differs from the source when the `plane` snapshot has no confirmed runtime/DB path: it keeps the structural boxes and reports missing evidence instead of drawing a fabricated API → service → repository → DB chain. A smaller crop was not needed because both panels and card labels remain legible in the normalized `2880 x 900` comparison.

## Primary interactions and runtime checks

- Layers and Sources open as mutually exclusive floating tools without moving the first canvas card (`x=118`, `y=164` before and after panel toggling).
- Package and module cards expand in place; siblings remain on the same canvas.
- Leaf selection keeps hierarchy depth unchanged and opens the Evidence panel.
- First API selection resolves `apps/api/plane/api/views/base.py` to `Plane → api`.
- First code selection resolves `apps/api/plane/db/integration/github.py` to `Plane → db` even when that inventory node is absent from the current map projection; inventory evidence remains visible.
- At the configured `820px` minimum width, opening Evidence closes Layers; reopening Layers closes Evidence. Root document remains exactly `820 x 760` with no overflow.
- Pan/select/fit/zoom, breadcrumb collapse, minimap fit, panel close, search, source management, and database entry points were exercised in the live Tauri WebView.
- Tauri/Vite terminal and rendered error boundaries were checked; no runtime exception or surface error was present.

## Comparison history

1. Replaced the fixed three-column legacy shell and blocking preparation/loading screen with a full-workspace canvas and floating tools.
2. Removed Atlas/Workbench surface switching and made containment depth the only navigation axis.
3. Fixed leaf selection so it no longer replaces or collapses the central map.
4. Preserved API source paths and used the deepest matching package/module; real WebView verification changed `apps` misrouting to `Plane → api`.
5. Kept inventory-only code evidence locally when map synchronization cannot find that node, preventing the inspector from disappearing.
6. At `820px`, both side panels left only 66px of visible canvas. The final responsive pass makes them alternate; post-fix evidence is `map-responsive-820-after.png`.

## Verification

- tests: `30` files, `164` tests passed
- typecheck: passed
- lint: passed with zero warnings
- dead code: `knip` reports zero files/issues
- production build: passed (`408.52 kB` JS, `72.29 kB` CSS before gzip)

## Follow-up polish

- P3: when the engine supplies confirmed cross-module edges, the overview can gain more visible connection lines without changing this layout.
- P3: a future user preference may remember panel open/closed state per workspace; it is intentionally not added without a product requirement.

final result: passed
