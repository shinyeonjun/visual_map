# Backend Visual Map Production Completion - Phase 4

Status: Complete locally  
Date: 2026-07-11

## Result

- Code-to-DB relationships remain candidates and carry inspectable evidence; no candidate is promoted to confirmed.
- Static ranking uses bounded table aliases, migration/DDL paths, repository/query/DAO roles and path/name overlap while rejecting generic single-word identifiers.
- Candidate generation is capped per code item and avoids an unbounded all-node/all-table render path.
- Selected table/column views enrich evidence with one bounded compact `search_code` request.
- Column searches are limited to files already matched for the table; broad generic-column repository searches are not performed.
- Exact code-search evidence records file and line without retaining source bodies.
- Pluralization, duplicate schemas, Unicode paths, generic terms, partial results and search failures are represented without false certainty.

## Verification

Passed:

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml candidate
powershell -File scripts/smoke-candidate-ranking.ps1
npm run typecheck
```

The smoke reuses the real `codebase-memory-mcp.exe` contract fixture and verifies typed table/column search locations in addition to the Rust candidate-ranking tests.

## Remaining Gates

- Real-project precision/recall still needs field validation across more languages and ORM styles in Phase 11.
- A failed enrichment keeps the base map and adds an unknown gap; it never fabricates a stronger relationship.
