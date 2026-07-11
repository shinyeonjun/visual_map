# Visual template rules

## Goal
Turn project data into fixed, readable comprehension templates for developers:
overview, API flow trace, table usage, and column impact. The UI can be polished,
but the product view must only show data backed by the current workspace, code
inventory, DB inventory, visual map, or explicit empty states.

## Rule 0: no fake product data
- Do not render invented projects, repositories, API paths, tables, files, line
  numbers, reviewers, PRs, confidence percentages, evidence counts, or risks in
  production views.
- Do not use realistic fallback values such as sample repo names or sample DB
  names when engine data is missing.
- Missing required data must render as an empty state, disabled action, or
  "engine support needed" state.
- Unsupported template slots must be omitted from the current view and listed as
  v2 engine requirements, not filled with sample content.

## Template contract
Each visual mode must declare:
- Input sources: which engine or inventory fields are allowed.
- Ranking: how visible items are ordered.
- Limits: maximum visible cards/rows before collapse.
- Confidence: whether an item is confirmed, candidate, or unavailable.
- Evidence: what source proves the relationship.
- Empty state: what the user sees when the input source is missing.

## Current v1 slots
- Overview: workspace, routes, code services/files, DB tables, selected item,
  snapshot time.
- API flow: routes and code inventory only until real call-path edges are
  available.
- Table usage: DB tables, columns, PK/FK metadata, and selected table.
- Column impact: DB table/column metadata and confirmed FK relationships.

## v2 engine requirements
- Real code call graph edges for request-to-handler-to-service traces.
- SQL/query reference extraction for code-to-table links.
- Test, migration, batch, queue, middleware, owner, reviewer, and PR metadata.
- Evidence ledger rows with file, line, reason, and confidence from the engine.

## Validation
Before accepting a UI change:
```powershell
npm run typecheck
npm run build
rg -n "Prod-Backend|github.com/acme|auth_sessions|@auth-team|/api/v1/sessions|server/app.ts:58|api/v1/auth.py" src
```

The final search should return no product-view mock data unless the hit is a
test fixture or documentation explicitly marked as non-product sample data.
