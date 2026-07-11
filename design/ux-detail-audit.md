# UX Detail Audit

Date: 2026-07-07

## Scope

Backend Visual Map workbench detail pass:

- Overview
- API flow
- DB impact
- 1440px and 1280px desktop viewports

## Research Basis

- NN/g progressive disclosure: defer secondary or advanced controls so complex apps are easier to learn and less error-prone.
- NN/g visual hierarchy: guide the eye with contrast, scale, grouping, and spacing.
- Fluent command bar guidance: commands should operate on the content region they sit above; overloaded command bars become harder to use.
- W3C WCAG 2.2 target size: pointer targets should be at least 24 x 24 CSS pixels.
- W3C WCAG focus visible: keyboard users need a visible focus indicator.

## Audit Findings And Fixes

1. API flow zoom and minimap overlapped content at 1280px.
   - Fix: removed the nonessential trace zoom/minimap elements.

2. Impact metrics wrapped secondary text and made the top row noisy at 1440px and below.
   - Fix: hide metric hints under 1450px and keep only label/value/status visible.

3. Left API selected badge wrapped into two lines at narrow width.
   - Fix: shortened the badge copy from "선택됨" to "선택".

4. Impact card selected badge covered item titles.
   - Fix: removed the badge from compact impact items and kept the active border state.

5. View switch labels wrapped in the left panel.
   - Fix: shortened API mode labels and forced segment labels to stay on one line.

6. Interactive targets were below 24px in two places.
   - Fix: increased the top search input and small action buttons to meet the 24px minimum.

## Verification

- `npm run typecheck`
- `npm run build`
- Playwright screenshots:
  - `design/final/final-standard-overview.png`
  - `design/final/final-standard-api.png`
  - `design/final/final-standard-impact.png`
  - `design/final/final-badge-api.png`
  - `design/final/final-badge-impact.png`
- Target-size scan at 1280px:
  - Overview: 0 failures
  - API flow: 0 failures
  - DB impact: 0 failures
