# Backend Visual Map Production Completion - Phase 7

Status: Complete locally  
Date: 2026-07-11

## Result

- Table and column focus render four independent lanes: direct DB impact, code candidates, unknowns and recommended checks.
- Direct impact includes available FK/PK/unique/check/index evidence without row-data access.
- Candidate lanes are independently capped and retain confidence, reason and source location.
- Compound column candidates now require the full identifier (`order_id` or `orderId`); partial `order`/`id` matches no longer create false candidates such as `_is_ordered_line` or `record_error`.
- No exact code result becomes an explicit unknown instead of a claim that code impact is absent.
- The review board exports a compact Markdown summary.

## Verification

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml
powershell -File scripts/smoke-candidate-ranking.ps1
powershell -File scripts/smoke-ui.ps1 -Scenario change-impact
npm run typecheck
npm run build
```

Result: `order_items.order_id` false candidates fell from 33 to 0; direct DB facts remained visible and code-search limitations moved to the unknown lane.
