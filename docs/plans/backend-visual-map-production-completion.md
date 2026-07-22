# Backend Visual Map Production Completion Plan

Status: Complete for MIT source distribution and local installer validation
Scale: Large
Date: 2026-07-10

## Goal

Backend Visual Map을 Windows에서 설치해 처음 보는 대규모 backend repository와 RDB schema를 분석하고, API 읽기 순서와 테이블/컬럼 변경 영향을 근거·불확실성·다음 행동과 함께 제공하는 현업용 로컬 개발 도구로 완성한다.

완성 기준은 기능의 개수가 아니라 다음 세 질문을 IDE 검색보다 빠르고 신뢰 가능하게 답하는 것이다.

1. 이 API는 실제로 어떤 handler/function/repository를 거치는가?
2. 이 table/column과 직접 연결되거나 후보로 의심되는 코드는 무엇인가?
3. 변경 전에 어떤 파일과 DB 제약을 어떤 순서로 확인해야 하는가?

## Current Facts

- 제품 앱은 `D:\project\backend_map`에 있고 공개 저장소 `https://github.com/shinyeonjun/visual_map`에 게시할 준비가 되었다.
- 코드 엔진은 외부 `DeusData/codebase-memory-mcp`이며 수정하지 않는다.
- DB 엔진은 자체 `shinyeonjun/rdb-memory-mcp`의 `0.2.0 / contract 2` 고정 source commit을 사용한다.
- 현재 bundled DB binary는 manifest의 candidate checksum과 동일하지만 아직 공개 release가 아니므로 `releaseReady=false`이며 로컬/내부 빌드에서만 허용된다.
- DB adapter는 metadata-only runtime contract와 complete snapshot 인증서를 확인하고, 페이지·개수·stable key가 하나라도 불일치하면 부분 결과를 저장하지 않는다.
- workspace, clone, engine execution, redaction, cache isolation, snapshot, projection, packaging 기반이 있다.
- 현재 실제 sidecar contract에서 code kind 오분류, 빈 CALLS, HANDLES 누락이 재현된다.
- backend atlas group projection과 frontend canvas rendering의 source of truth가 다르다.
- DB engine이 가진 index/constraint/capability 정보가 앱 snapshot에서 손실된다.
- source line이 snapshot에 보존되지 않고 editor jump가 없다.
- `npm run typecheck`, `npm run build`, Rust test 59개는 통과하지만 real sidecar contract drift를 막지 못한다.

## Proposed Behavior

- 정확한 engine label과 edge type만 confirmed inventory/relationship으로 수용한다.
- Rust adapter가 raw engine output을 canonical versioned snapshot으로 한 번만 정규화한다.
- React는 backend `VisualMap` projection만 렌더링하며 관계 의미나 ranking을 재계산하지 않는다.
- Architecture, API Flow, Change Impact의 세 핵심 화면이 고정된 시각 문법을 사용한다.
- 모든 결과는 confirmed/structural/candidate/unknown 중 하나이며 evidence와 source action을 가진다.
- engine binary는 repository에 커밋하지 않고 version/checksum manifest로 준비한다.
- release는 실제 엔진 contract, 대규모 snapshot, installer, 보안 검사를 모두 통과해야 한다.

## Success Criteria

### Accuracy

- golden repositories에서 engine label이 Route가 아닌 항목은 API로 표시되지 않는다.
- engine label이 File이 아닌 항목은 File inventory로 표시되지 않는다.
- API Flow의 첫 confirmed hop은 HANDLES 근거가 있는 Route ↔ Handler 연결이다.
- confirmed CALLS edge는 source와 target 두 endpoint가 실제 inventory node와 일치한다.
- dangling edge, duplicate node ID, bucket 기반 kind 덮어쓰기가 0건이다.
- DB direct impact에 PK, FK, unique/check constraint, index가 가능한 범위에서 누락 없이 표시된다.
- 데이터가 없거나 adapter capability가 부족하면 `unknown`으로 표시하고 추론으로 대체하지 않는다.

### Developer Value

- API 선택 한 번으로 읽을 파일/함수 순서, confirmed calls, DB 후보, 끊긴 구간이 같은 화면에 나온다.
- table/column 선택 한 번으로 직접 영향, 코드 후보, 확인 필요, 권장 검증이 같은 화면에 나온다.
- file/line evidence에서 VS Code 또는 Cursor로 이동할 수 있다.
- candidate마다 왜 후보인지와 검증할 source location을 볼 수 있다.
- overview group에서 2회 이하 클릭으로 top API/code/table detail에 도달한다.

### Scale

- 10k+ code nodes, 40k+ code edges, 200+ tables synthetic fixture에서 overview visible group은 40개 이하다.
- focus projection은 기본 36개 이하 node와 연결된 edge만 반환한다.
- 기존 snapshot이 있을 때 workspace restore와 첫 projection은 기준 Windows 장비에서 2초 이내를 목표로 측정한다.
- search interaction은 300ms 이내를 목표로 측정하고 느린 engine enrichment는 취소 가능한 별도 상태로 보인다.
- UI thread에서 전체 raw graph layout이나 O(nodes × tables) candidate scan을 실행하지 않는다.

### Reliability And Release

- clean Windows 환경에서 PATH 없이 설치·실행·인덱싱·재시작·제거가 가능하다.
- pinned engine checksum이 다르면 build와 runtime 모두 명확히 실패한다.
- secret persistence, row-data access, arbitrary command execution 검사가 통과한다.
- third-party notices, dependency inventory와 product license가 public source release gate를 통과한다.
- 이전 engine manifest와 app installer로 rollback하는 절차가 문서화되고 smoke된다.

## Non-Goals

- chatbot-first Q&A
- raw graph 전체 렌더링
- DB row data 조회 또는 SQL console
- 자동 코드 수정, migration 실행, schema 변경
- code→DB 후보의 자동 confirmed 승격
- cloud upload, team sync, multi-user collaboration
- external code engine fork 유지보수
- PR Impact, architecture drift, git history hotspot은 세 핵심 화면이 완성되기 전 구현하지 않는다.

## Architecture

```text
Local repo / GitHub clone             RDB metadata source
          |                                   |
          v                                   v
codebase-memory pinned sidecar        database-memory pinned sidecar
          |                                   |
          +---------- raw JSON contract ------+
                              |
                              v
                   Rust engine adapters
            validate / normalize / dedupe / version
                              |
                              v
                  Canonical Snapshot v2+
      nodes / relationships / evidence / locations / capabilities
                              |
                              v
                    Rust projections
       Architecture / API Flow / Change Impact / Search Focus
                              |
                              v
                  React answer workspace
       fixed panels / selection / layout / keyboard / source jump
```

### Repository Boundary

- `visual_map`: product source, adapters, snapshot/projection/UI, CI, packaging scripts.
- `rdb-memory-mcp`: DB metadata engine and versioned CLI JSON contract.
- `DeusData/codebase-memory-mcp`: upstream artifact only.

### Canonical Snapshot Minimum Contract

- `schemaVersion`
- `workspaceId`, `savedAt`
- engine id/version/checksum and source metadata
- nodes with stable id, semantic kind, layer, display name, source location, parent/group
- relationships with type, truth class, direction, evidence, engine edge type
- DB objects with schema-qualified identity, constraint/index metadata and capability warnings
- explicit unknown/gap records
- migration/reindex requirement metadata

### Compatibility Rules

- app adapter supports an explicit engine version range, never `unknown means compatible`.
- fixture output from each pinned EXE is committed as small sanitized JSON.
- new engine artifact is accepted only after contract and real repository smoke pass.
- old snapshot loads through a migration or becomes `reindex required`; it is never silently interpreted as the latest schema.

## Implementation Phases

### Phase 0: Product Repository And Reproducible Inputs

Goal:

- Make `backend_map` safe to version, review, reproduce, and release before feature work expands.

Deliverables:

- Initialize product Git history locally and connect it to `shinyeonjun/visual_map` only after explicit owner approval.
- Ignore `src-tauri/engines/*.exe`, installer output, local caches, screenshots generated only for temporary QA, and secret-bearing environment files.
- Add `src-tauri/engines/manifest.json` with engine id, semantic version/commit, release URL, filename, SHA-256, license id and contract version.
- Add idempotent PowerShell engine preparation/checksum script.
- Pin supported Node and Rust toolchains and require `npm ci` plus Cargo `--locked` in CI.
- Add a product-version consistency check for `package.json`, Rust `Cargo.toml` and `tauri.conf.json`.
- Add baseline GitHub Actions workflow for npm/Rust checks without publishing.
- Preserve existing `db_mcp` working-tree modification; do not copy or overwrite it.
- Record product license as an owner decision gate; do not pretend the placeholder permits distribution.
- Confirm source visibility before the first push because `visual_map` is currently public.

Verification:

```powershell
git status --short
git ls-files | rg "\.exe$|target/|node_modules/|dist/"
powershell -File scripts/prepare-engines.ps1 -VerifyOnly
npm ci
npm run typecheck
npm run build
cargo test --locked --manifest-path src-tauri/Cargo.toml
```

Rollback:

- Before the first push, remove only newly created Git metadata/config if the owner rejects the repository layout.
- Engine manifest preparation does not delete existing local binaries.

### Phase 1: Real Engine Contract Truth

Goal:

- Stop false API/File inventory and restore confirmed Route→Handler→CALLS data before UI expansion.

Deliverables:

- Replace bucket semantics with exact code engine label queries and stable pagination.
- Fix CALLS query aliases (`source`/`target`) and parse two-column results.
- Ingest HANDLES separately and normalize it into a product Route→Handler relationship without changing upstream direction.
- Preserve source file, start/end line, qualified name, engine label and project id.
- Dedupe nodes shared across queries by qualified name; reject conflicting kinds.
- Isolate engine caches by engine id/version/contract so an upgrade never opens an incompatible cache in place.
- Add sanitized real-sidecar fixtures covering Route, Handler, File, Function, CALLS and HANDLES.
- Add an optional real-binary contract smoke that runs when pinned engines are present.
- Keep old UI behavior except that false items disappear and confirmed flows become available.

Verification:

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml workspace::
powershell -File scripts/smoke-code-engine-contract.ps1
npm run typecheck
npm run build
```

Acceptance:

- no-route fixture reports zero routes.
- route fixture reports only engine Route nodes.
- every CALLS row has two known endpoints.
- every API flow starts with confirmed HANDLES when the engine supplies it.

Rollback:

- Keep the existing adapter behind a temporary test-only fixture comparison until the new contract passes; do not keep a runtime fake fallback.

### Phase 2: Canonical Snapshot V2 And Single Ownership

Goal:

- Remove Rust/TypeScript normalization drift and preserve every source/evidence field needed by later screens.

Deliverables:

- Add snapshot schema version and Rust-owned builder.
- Move normalization, ID generation, dedupe and relationship creation into Rust.
- Preserve line/column/end-line, engine label, group membership and architecture facts.
- Remove frontend `buildInventorySnapshot` responsibility after parity tests pass.
- Add V1→V2 migration for safe fields; mark incompatible snapshots `reindex required`.
- Add round-trip, stale, migration, redaction and duplicate-ID tests.

Verification:

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml atlas::
npm run typecheck
npm run build
```

Rollback:

- Retain original snapshot file as backup until V2 save completes atomically.

### Phase 3: DB Evidence Contract And Engine Release

Goal:

- Expose the DB graph facts already available in `db_mcp` without reading rows or opening its cache directly.

Deliverables In `db_mcp`:

- Add CLI version/schema-capability JSON command.
- Add all-constraint/index table description JSON.
- Expose impact analysis and relationship trace as bounded JSON commands.
- Include stable object keys, snapshot key, edge type, depth and capability warnings.
- Keep `unsafe-row-sampling` disabled and add a release guard test.
- Version and publish a Windows CLI release with checksum.

Deliverables In `backend_map`:

- Parse DB contract into Snapshot V2 DB objects/relationships.
- Preserve PK, FK, unique/check, index and inbound/outbound direction.
- Display unsupported metadata as unknown capability, not empty confirmed data.
- Add SQLite DDL and PostgreSQL golden fixtures.

Verification:

```powershell
cargo test --locked --manifest-path D:\project\db_mcp\Cargo.toml
cargo test --locked --manifest-path src-tauri/Cargo.toml workspace::
powershell -File scripts/smoke-rdb-productization.ps1
```

Rollback:

- Pin the previous DB engine manifest artifact; Snapshot V2 retains engine version and rejects unsupported newer payloads.

### Phase 4: Evidence Ranking Without False Certainty

Goal:

- Improve code↔DB candidates with inspectable evidence and bounded cost.

Deliverables:

- Introduce a product `CandidateEvidence` model with kind, source location, target DB object, rank contribution and human reason.
- Evidence sources in priority order:
  - exact quoted SQL/table/column literal;
  - migration/DDL reference;
  - repository/query/DAO naming;
  - confirmed call-path proximity;
  - route/domain token overlap;
  - existing name/path match.
- Compute expensive literal/call enrichment on selected focus and cache it by snapshot+focus, not for every node×table pair.
- Keep all code→DB results candidate.
- Add positive, negative, pluralization, schema-qualified and generic-column fixtures.
- Add a repeatable focused ranking smoke script using a small real repository/DDL pair.

Verification:

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml candidate
powershell -File scripts/smoke-candidate-ranking.ps1
npm run typecheck
```

Rollback:

- Fall back to fewer candidate evidence types, not to unlabelled or fabricated links.

### Phase 5: Architecture Domain Cards And Drilldown

Goal:

- Make the first screen a stable, actionable project map instead of inventory card sampling.

Deliverables:

- Enrich Rust atlas projection with group membership, top items, hidden counts, importance and evidence.
- Combine engine clusters/packages, route prefixes, folder boundaries and DB schemas deterministically.
- Render `VisualMap` group nodes as the only overview card source.
- Group click produces a bounded drilldown projection with API → code → DB ordering.
- Sort groups by confirmed degree, routes, tables and bounded relevance; use stable tie-breakers.
- Add empty, single-layer, monorepo, duplicate-schema and 40+ group fixtures.
- Add a small `scripts/smoke-ui.ps1 -Scenario <name>` orchestrator around the existing CDP helper for repeatable UI assertions.

Verification:

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml atlas::
npm run typecheck
npm run build
powershell -File scripts/smoke-ui.ps1 -Scenario atlas-drilldown
```

Rollback:

- Keep the previous bounded inventory list accessible only as a diagnostic view, not as the product overview.

### Phase 6: Evidence-Backed API Reading Path

Goal:

- Answer “이 API를 고치려면 무엇을 어떤 순서로 읽는가?” with confirmed calls and explicit gaps.

Deliverables:

- Build Route → Handler from HANDLES, then bounded CALLS traversal.
- Classify Handler, Service/Function, Repository/Query using engine label/properties and deterministic product rules.
- Produce an ordered reading path with file/line for each step.
- Append DB candidates only after confirmed code reachability; show evidence and unknown breaks.
- Show branch/fan-out collapse and `+N` without hiding the selected path.
- Right panel sections: reading order, confirmed relations, candidates, unknowns, recommended checks.

Verification:

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml api_flow
npm run typecheck
npm run build
powershell -File scripts/smoke-ui.ps1 -Scenario api-flow
```

Rollback:

- When HANDLES is unavailable, show `handler unknown`; do not re-enable name-token flow as confirmed.

### Phase 7: Four-Lane Change Impact Review Board

Goal:

- Make table/column changes reviewable before code or migration edits begin.

Deliverables:

- Direct impact lane: PK/FK/unique/check/index and related DB objects.
- Code candidate lane: ranked functions/files/APIs with evidence.
- Unknown lane: unsupported capabilities, missing source, disconnected call paths, stale data.
- Recommended checks lane: ordered files, constraints, tests/migrations to inspect.
- Support table and column focus with stable deep links and copyable Markdown summary.
- Never call row-data or arbitrary SQL paths.

Verification:

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml column_impact
npm run typecheck
npm run build
powershell -File scripts/smoke-ui.ps1 -Scenario change-impact
rg -n "SELECT\s+\*|unsafe-row-sampling" src src-tauri scripts
```

Rollback:

- Keep direct DB facts visible when candidate enrichment fails; mark candidate/unknown lanes unavailable independently.

### Phase 8: Source Jump And Investigation Workflow

Goal:

- Remove copy/paste navigation friction between the map and the editor.

Deliverables:

- Add allowlisted VS Code/Cursor detection and selection.
- Open repo-contained absolute path at validated line/column using argument arrays.
- Add safe Explorer fallback and existing copy actions.
- Add a local investigation tray containing selected path, evidence and checked state.
- Export a compact Markdown investigation summary without source contents or secrets.
- Persist only paths/evidence identifiers, not code bodies.

Verification:

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml source_jump
npm run typecheck
npm run build
powershell -File scripts/smoke-ui.ps1 -Scenario source-jump
```

Rollback:

- Disable editor launch while preserving copy/export if executable detection or path validation fails.

### Phase 9: Search, Cancellation And Large-Repository Performance

Goal:

- Keep interaction fast and deterministic on production-sized repositories.

Deliverables:

- Search canonical snapshot locally first; enrich selected results asynchronously.
- Add operation generation/cancellation so stale engine responses cannot overwrite current workspace state.
- Move heavy projection/ranking off render paths and remove frontend duplicate computations.
- Add synthetic 10k/40k/200-table benchmark fixture and timing logs.
- Add layout snapshot tests for 1180×760 and 1440×900.
- Add memory, projection time and visible-node diagnostic metrics without telemetry upload.

Verification:

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml
cargo test --locked --manifest-path src-tauri/Cargo.toml --release large_snapshot
npm run typecheck
npm run build
powershell -File scripts/smoke-ui.ps1 -Scenario large-repo
```

Rollback:

- Bound each enrichment independently and return the last complete projection plus a visible timeout warning.

### Phase 10: Recovery, Security And Diagnostics Hardening

Goal:

- Make failures supportable without leaking source or corrupting workspaces.

Deliverables:

- Atomic workspace/snapshot writes and per-workspace corruption isolation.
- Actionable recovery for missing/corrupt engine cache, snapshot and workspace file.
- Redacted opt-in diagnostics bundle with version, timings, counts and error classes only.
- Path traversal, symlink/reparse point, editor command, clone URL and sidecar argument security tests.
- Dependency/license/SBOM generation and secret scanning.
- Verify Tauri capability/CSP least privilege after editor launch additions.
- Add a single security audit script that runs source scans and the relevant focused tests.

Verification:

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml
npm audit --omit=dev
npm run typecheck
npm run build
powershell -File scripts/security-audit.ps1
```

Rollback:

- Diagnostics export and editor launch remain separately disableable; core local analysis continues.

### Phase 11: Release Readiness And Field Validation

Goal:

- Prove the finished product on clean Windows installations and varied real codebases.

Deliverables:

- GitHub Actions Windows release workflow using verified engine assets.
- Complete product license and third-party notices for both engines and application dependencies.
- Publish source only; do not distribute an official Windows installer or enable an automatic updater.
- Test matrix:
  - TypeScript/Node API repository;
  - Java/Spring or C#/.NET repository;
  - Python/FastAPI or Django repository;
  - monorepo;
  - DB-only SQLite DDL;
  - PostgreSQL plus one additional network DB.
- Clean-profile install/update/rollback/uninstall smoke.
- Senior-developer task validation for onboarding, API modification and column change review.
- Add release smoke and third-party notice verification scripts used by CI and local release review.

Verification:

```powershell
npm ci
npm run typecheck
npm run build
cargo test --locked --manifest-path src-tauri/Cargo.toml --release
powershell -File scripts/prepare-engines.ps1 -VerifyOnly
npm run tauri build
powershell -File scripts/release-smoke.ps1
```

Rollback:

- Keep the previous engine manifest available so local builders can roll back pinned artifacts.

## Test Plan

### Always-On Pull Request Checks

```powershell
npm ci
npm run typecheck
npm run build
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --locked --manifest-path src-tauri/Cargo.toml
```

### Engine Contract Checks

```powershell
powershell -File scripts/prepare-engines.ps1 -VerifyOnly
powershell -File scripts/smoke-code-engine-contract.ps1
powershell -File scripts/smoke-rdb-productization.ps1
```

### Release Checks

```powershell
powershell -File scripts/security-audit.ps1
powershell -File scripts/verify-third-party-notices.ps1
npm run tauri build
powershell -File scripts/release-smoke.ps1
```

## Phase Exit Discipline

- 한 phase에서 다음 phase 기능을 미리 구현하지 않는다.
- 각 phase 완료 후 `docs/reports/backend-visual-map-production-completion.phase-N.md`를 작성한다.
- 구현자와 검토자는 역할을 분리한다.
- 실제 sidecar contract test가 실패하면 UI 작업으로 넘어가지 않는다.
- 문서의 완료 표시는 테스트와 실제 smoke evidence가 있을 때만 변경한다.

## Risks And Assumptions

- `codebase-memory-mcp`의 CLI contract는 upstream에서 바뀔 수 있다. manifest pin과 fixture가 필수다.
- code engine Route/HANDLES 품질은 언어/framework마다 다르므로 unknown gap이 정상 결과가 될 수 있다.
- DB adapter capability는 source마다 다르다. UI는 source별 누락을 확정 부재로 오해하면 안 된다.
- `db_mcp`의 기존 PostgreSQL 수정은 사용자 작업이므로 별도 검토 없이 덮어쓰지 않는다.
- `visual_map` remote 연결과 첫 push는 외부 변경이므로 소유자 승인 후 실행한다.
- 공개 source와 MIT license는 확정됐고 공식 Windows installer 배포는 non-goal이다.
- Windows-first 범위를 유지한다. 다른 OS는 Windows release가 안정화된 후 별도 계획으로 다룬다.

## Ready-To-Send Implementer Prompts

### Phase 0

```text
Read docs/research/backend-visual-map-production-readiness.md and docs/plans/backend-visual-map-production-completion.md. Implement Phase 0 only. Do not push or connect a remote without explicit owner approval. Do not commit engine EXEs. Preserve all existing user changes in db_mcp. Add reproducible engine manifest/preparation and baseline checks with the smallest safe patch. Run Phase 0 verification and write docs/reports/backend-visual-map-production-completion.phase-0.md.
```

### Phase 1

```text
Read docs/research/backend-visual-map-production-readiness.md and docs/plans/backend-visual-map-production-completion.md. Implement Phase 1 only. Fix the real code sidecar contract: exact labels, pagination, source/target CALLS aliases, HANDLES ingestion, stable dedupe and source locations. Do not change product UI beyond removing false data. Run fixture plus real-binary contract checks and write docs/reports/backend-visual-map-production-completion.phase-1.md.
```

### Phases 2-11

```text
Read docs/research/backend-visual-map-production-readiness.md and docs/plans/backend-visual-map-production-completion.md. Implement Phase <N> only with the smallest safe patch. Preserve confirmed/structural/candidate/unknown semantics and all privacy guardrails. Do not pull later phases forward. Run every Phase <N> verification command that is available, record honest skips, and write docs/reports/backend-visual-map-production-completion.phase-<N>.md.
```

## Ready-To-Send Reviewer Prompts

### Phase 0

```text
You did not write this code. Review the current diff only against Phase 0 of docs/plans/backend-visual-map-production-completion.md. Do not edit files. Findings first. Check repository pollution, binary/checksum reproducibility, accidental remote/push changes, license claims, CI gaps and preservation of existing user files.
```

### Phase 1

```text
You did not write this code. Review the current diff only against Phase 1 of docs/plans/backend-visual-map-production-completion.md. Do not edit files. Findings first. Re-run or inspect real sidecar fixtures. Focus on false Route/File classification, CALLS column parsing, HANDLES direction, pagination, duplicate IDs, missing source locations, fallback fabrication and missing regression tests.
```

### Phases 2-11

```text
You did not write this code. Review the current diff only against Phase <N> of docs/plans/backend-visual-map-production-completion.md. Do not edit files. Findings first, ordered by severity. Focus on correctness, engine contract drift, data loss, false certainty, security/privacy, performance ceilings, rollback safety, missing tests and simpler existing-code alternatives.
```

## First Recommended Implementation

Start with Phase 0 repository hygiene and engine manifest locally, then Phase 1 engine contract truth. Do not begin domain-card polish or the four-lane board while routes, files, CALLS and HANDLES can still be wrong.
