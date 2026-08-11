# Design QA — codebase workspace redesign

## Comparison target

- Source visual truth — overview: `C:\Users\plosind\AppData\Local\Temp\codex-clipboard-44f231d9-8f97-4d24-ac57-7bea8afc0e44.png`
- Source visual truth — selected drill-in direction: `C:\Users\plosind\AppData\Local\Temp\codex-clipboard-43d3715f-c586-4286-a986-d1cc297cc081.png`
- Intentionally not mixed into the drill-in: screenshot 3 (`codex-clipboard-12d1c31e-9564-4c43-9c55-46be5e141ffb.png`)
- Implementation URL: `http://localhost:4173/?preview=1`
- Implementation screenshots:
  - `D:\project\visual_map\artifacts\design-qa\overview-selected-2048x1152.png`
  - `D:\project\visual_map\artifacts\design-qa\trace-2048x1152.png`
  - `D:\project\visual_map\artifacts\design-qa\trace-responsive-1024x768.png`
- Full-view comparison evidence:
  - `D:\project\visual_map\artifacts\design-qa\comparison-overview-2048.png`
  - `D:\project\visual_map\artifacts\design-qa\comparison-trace-2048.png`
- Focused-region comparison evidence:
  - `D:\project\visual_map\artifacts\design-qa\comparison-overview-focus.png`
  - `D:\project\visual_map\artifacts\design-qa\comparison-trace-focus.png`

## Normalization and state

- Source pixels: 2048 × 1152 for both selected reference images.
- Implementation pixels: 2048 × 1152.
- CSS viewport: 2048 × 1152; `devicePixelRatio` 1.0.
- Density normalization: none required for the final pass; source and implementation were compared at equal pixel dimensions. Earlier 1280 × 720 evidence was used only for the initial app-window readability pass.
- Theme/state: light desktop workbench; `commerce-platform`; overview with `주문` selected; drill-in at `주문 생성 / POST /orders`; right evidence dock visible.
- The topology and counts are deliberately not pixel-identical mock data. Fidelity was judged on shell composition, hierarchy, lane model, evidence treatment, density, typography, and interaction behavior. The implementation continues to label preview data as non-analysis data.

## Findings

No actionable P0, P1, or P2 findings remain.

- Fonts and typography: passed. Bahnschrift is used for technical/display hierarchy, Segoe UI Variable for UI copy, and Cascadia Mono for code and machine-readable values. Navigation labels are 13/10px at the normal app viewport, overview card hierarchy scales to remain readable, and compact trace titles ellipsize instead of breaking mid-symbol.
- Spacing and layout rhythm: passed. The left structure rail is 144px on wide desktops, 136px at the normal app viewport, and 64px at the compact breakpoint. The evidence dock is bounded separately, the overview auto-fits up to 120%, and all five trace stages fit without horizontal scrolling at 1280px and above.
- Colors and visual tokens: passed. The implementation preserves the reference's bright technical-workbench balance with blue selection/flow, green verified facts, gray structural facts, and amber candidates. Borders and shadows stay low contrast and no decorative gradient was introduced.
- Image quality and asset fidelity: passed. The source contains no required photographic or illustrative assets. Visible UI icons come from the Fluent icon library; no emoji, custom inline SVG, placeholder illustration, or raster substitute was used.
- Copy and content: passed. Korean responsibility and evidence labels remain paired with the underlying English identifiers. Candidate and preview states are explicitly named so inferred or development-only information cannot be mistaken for verified analysis.
- Icons and affordances: passed. Header search, navigation, selected cards, truth filters, zoom controls, evidence tabs, and source rows use one consistent Fluent icon family and retain semantic button/tab roles.
- Accessibility and responsiveness: passed for the tested desktop range. The 1024 × 768 shell has no page-level overflow; the trace itself scrolls horizontally by design while persistent navigation, filters, status, and evidence controls remain visible. Reduced-motion handling and accessible names are present.

## Comparison history

### Iteration 1 — overview readability

- Earlier P2: the fitted overview landed near 50%, leaving card labels too small and the structure rail too wide for the useful canvas.
- Earlier evidence: `D:\project\visual_map\artifacts\design-qa\iteration-01-overview-before.png`.
- Fixes: reduced the navigation rail, compacted the world bounds and preview positions, increased card/navigation type, changed the default layout to three readable columns, and allowed small maps to fit up to 120% on large displays.
- Post-fix evidence: `D:\project\visual_map\artifacts\design-qa\overview-selected-2048x1152.png` and `comparison-overview-focus.png`.

### Iteration 2 — drill-in overflow and density

- Earlier P2: 170px minimum lanes overflowed at the normal 1280px app window, cropping the data-boundary stage and forcing unnecessary horizontal navigation.
- Earlier evidence: `D:\project\visual_map\artifacts\design-qa\iteration-01-trace-before.png`.
- Fixes: selected screenshot 2 as the only drill-in model, reduced the minimum lane width to 148px, bounded wide-screen lanes to 1120px, tightened the evidence dock, raised trace typography, and moved parent-area relationship aggregates into a clearly labelled boundary section.
- Post-fix evidence: `D:\project\visual_map\artifacts\design-qa\iteration-02-trace-no-overflow.png`, `trace-2048x1152.png`, and `comparison-trace-focus.png`.

### Iteration 3 — relation clutter and runtime cleanliness

- Earlier P2: persistent relation count pills overlapped the selected overview card, and repeated relation labels produced duplicate React keys in the inspector.
- Fixes: relation labels now appear only for small hovered neighborhoods; ambient count pills were removed; identical label/truth tallies are aggregated because the selection contract does not expose target names.
- Post-fix evidence: the final overview comparison has no overlapping relation pills, and a fresh browser session reported zero warnings or errors after overview selection, drill-in, truth filtering, node selection, back navigation, and symbol search.

### Iteration 4 — compact desktop wrapping

- Earlier P2: long Java identifiers wrapped at arbitrary characters at 1024px.
- Fix: the compact breakpoint uses single-line ellipsis while preserving the full accessible button name.
- Post-fix evidence: `D:\project\visual_map\artifacts\design-qa\trace-responsive-1024x768.png`; document width remained exactly 1024px.

## Primary interactions tested

- Overview area selection and evidence-dock update.
- `주문 생성` drill-in and return to the full structure.
- Verified/structural/candidate relation filtering.
- Trace-node selection (`OrderService`) and inspector synchronization.
- Header search for `OrderService` and result-driven drill-in.
- 1280 × 720, 2048 × 1152, and 1024 × 768 desktop layouts.
- Fresh-session browser console: zero warnings and zero errors.

## Follow-up polish

- P3: a minimap could be added later for very large analyzed repositories. It is not required for the seven-area reference state, and adding a decorative or nonfunctional minimap would reduce trust.
- Expected product constraint: the chosen screenshot-2 reference shows exact branch callsites. The current published frontend contract exposes one verified linear trace plus area-level relationship aggregates, so the implementation does not invent node-level branches it cannot prove.

## Implementation checklist

- [x] One overview composition and one screenshot-2-style drill-in.
- [x] Narrower structure rail and larger readable type.
- [x] Functional selection, tabs, filters, search, zoom, and drill navigation.
- [x] Preview state visibly separated from real analyzed data.
- [x] Legacy UI paths and unused exports removed; dead-code check clean.
- [x] Browser, lint, formatting, tests, dead-code, and production build checks passed.

final result: passed
