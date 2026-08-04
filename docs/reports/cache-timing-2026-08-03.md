# Cache timing — 2026-08-03

Measured from the same five-repository run as
`engine-baseline-2026-08-03.md`. Timings are emitted by
`language-index.timings[]`; no timing is inferred from wall-clock totals.

| Repository | Files | file walk | source hashing | cache invalidation | key hashing | cache I/O | JSON deserialize | lookup/planning | discovery + cache lookup | Discovery / file |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| Django + DRF + TypeScript | 3,973 | 35 ms | 439 ms | 15 ms | 90 ms | 0 ms | 0 ms | 414 ms | 1,556 ms | 0.39 ms |
| Spring Boot / Java | 84 | 6 ms | 11 ms | 0 ms | 24 ms | 0 ms | 0 ms | 33 ms | 365 ms | 4.35 ms |
| ASP.NET / C# | 4,069 | 48 ms | 426 ms | 17 ms | 35 ms | 0 ms | 0 ms | 273 ms | 1,220 ms | **0.30 ms** |
| FastAPI + TypeScript | 142 | 1 ms | 2,479 ms | 14 ms | 59 ms | 0 ms | 0 ms | 78 ms | 4,555 ms | 32.08 ms |
| Rust | 1,840 | 19 ms | 31,944 ms | 20 ms | 446 ms | 0 ms | 0 ms | 647 ms | 33,002 ms | 17.94 ms |

## Result

The 4,000-file acceptance case is ASP.NET / C# at 0.30 ms per file for
`file_discovery_and_cache_lookup`, below the 2 ms limit. The new stages also
show that the measured run was not dominated by cache JSON I/O or JSON
deserialization; source hashing and provider/semantic work dominate the slow
Rust and FastAPI cases. The implementation therefore keeps the existing JSON
cache format and avoids an unnecessary format migration.

Cache compatibility remains covered by the existing snapshot migration tests:
old schema snapshots are migrated or invalidated before reuse, and the index
output remains structurally equivalent.
