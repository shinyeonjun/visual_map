# Backend Visual Map Production Readiness Research

Status: Decision baseline
Scale: Large
Date: 2026-07-10

## Purpose

Backend Visual Map을 데모 가능한 그래프 UI가 아니라, 시니어 백엔드 개발자가 실제 변경 전 조사와 코드 읽기에 반복 사용해도 되는 로컬 Windows 개발 도구로 완성하기 위한 제품·기술 결정을 기록한다.

이 문서는 현재 구현 사실과 앞으로의 결정을 분리한다. 제안 동작은 `docs/plans/backend-visual-map-production-completion.md`에서 단계별로 관리한다.

## Repository Facts

- `D:\project\backend_map`은 Tauri 2 + Rust + React 제품 소스지만 아직 Git 저장소가 아니다.
- `https://github.com/shinyeonjun/visual_map`은 2026-07-10 기준 비어 있는 공개 저장소다.
- `D:\project\db_mcp`는 별도 Git 저장소이며 remote는 `shinyeonjun/rdb-memory-mcp`다. 제품 요구에 맞춰 변경할 수 있는 자체 엔진이다.
- `D:\project\codebase-memory-mcp`는 `DeusData/codebase-memory-mcp`의 외부 upstream 소스다. 제품에서 fork하거나 임의 수정하지 않는다.
- `backend_map/src-tauri/engines`의 두 EXE는 위 두 엔진 저장소의 현재 release 산출물과 SHA-256이 일치한다.
- 이 일치는 로컬 산출물과의 byte equality만 증명한다. 현재 파일만으로 official tag/commit provenance나 재현 가능한 빌드를 증명하지 못한다.
- 로컬 코드 엔진 checkout은 `v0.8.1` 이후 커밋을 포함하고 `release/`가 untracked이며, DB 엔진에는 미커밋 PostgreSQL adapter 변경이 있다. 현 상태의 로컬 binary를 provenance 없이 공개 release에 사용하지 않는다.
- `codebase-memory-mcp.exe`는 약 269MB다. GitHub는 일반 Git 객체 100MB를 초과하면 push를 차단하므로 제품 저장소에 일반 파일로 커밋할 수 없다.
- 제품 `LICENSE`는 아직 재배포를 허용하지 않는 placeholder이고 `THIRD_PARTY_NOTICES.md`도 완성되지 않았다.

Sources:

- https://github.com/shinyeonjun/visual_map
- https://github.com/DeusData/codebase-memory-mcp
- https://docs.github.com/en/repositories/working-with-files/managing-large-files/about-large-files-on-github

## Current Product Facts

### Strong Foundations

- 워크스페이스 생성·복구, 로컬 폴더, GitHub clone 흐름이 존재한다.
- 코드와 DB 엔진 캐시는 워크스페이스별로 격리된다.
- 네트워크 DB 연결 문자열은 세션 입력이며 워크스페이스에 저장되지 않는다.
- 엔진 실행 timeout, stdout/stderr secret 마스킹, installer/MCP 등록 인자 차단이 구현돼 있다.
- 코드/DB 인벤토리를 앱 소유 `InventorySnapshot`으로 변환하고 Rust가 `VisualMap` projection을 만든다.
- 스냅샷 source path/profile 기반 stale 판정과 재시작 복구가 있다.
- DB row data 조회와 임의 SQL 콘솔은 제품 경계 밖이다.
- `npm run typecheck`, `npm run build`, Rust 단위 테스트 59개가 현재 통과한다.

### Product-Critical Gaps

1. 코드 인벤토리가 엔진 label이 아니라 광범위한 BM25 문구에 의존한다.
   - `api route endpoint`는 실제 Route가 아닌 일반 Function을 반환할 수 있다.
   - `file module`은 File이 아닌 Function/Field를 반환할 수 있다.
   - 앱은 이 결과 bucket을 각각 API/File로 다시 분류하므로 가짜 종류가 생길 수 있다.
2. CALLS query의 `AS from`, `AS to` 별칭은 현재 엔진에서 정상적인 두 컬럼 결과를 만들지 못한다.
   - 실제 sidecar 실행에서 한 컬럼만 반환되어 `extract_code_calls`가 모든 행을 버렸다.
   - `AS source`, `AS target`은 정상 동작했다.
3. 코드 엔진의 Route 연결은 `handler -[:HANDLES]-> Route`인데 앱은 CALLS만 읽고 Route에서 outbound CALLS를 탐색한다.
   - 따라서 alias 수정만으로 Route → Handler → CALLS 읽기 순서는 완성되지 않는다.
4. Rust는 `group:*` atlas projection을 만들지만 현재 `AtlasCanvas`는 projection group node가 아니라 원본 인벤토리 카드를 렌더링한다.
5. DB 엔진 `describe-table`은 index, unique/primary flag, inbound/outbound FK, capability warning을 반환하지만 앱 모델은 column과 outbound FK 일부만 보존한다.
6. 코드↔DB 후보는 코드 이름/경로와 테이블명의 단순 포함 여부만 사용한다.
7. snapshot이 코드 line을 보존하지 않아 재시작 후 정확한 source jump를 제공할 수 없다.
8. 현재 테스트는 parser helper와 fixture projection을 검증하지만 실제 bundled sidecar와의 contract drift를 막지 못한다.

## Finished-Product Bar

시니어 개발자가 신뢰하려면 다음이 동시에 성립해야 한다.

- 잘못 분류한 API/File을 보여주지 않는다. 데이터가 없으면 없다고 말한다.
- API 흐름은 Route, Handler, CALLS 순서를 엔진의 실제 edge type으로 구성한다.
- 끊긴 구간을 이름 추론으로 몰래 메우지 않고 `확인 필요`로 표시한다.
- 테이블/컬럼 변경 답은 DB 직접 구조, 코드 후보, 알 수 없음, 권장 검증을 분리한다.
- 모든 답 항목은 파일/라인, DB object key, edge type, evidence 중 하나 이상의 근거로 이동할 수 있다.
- overview는 원본 노드를 나열하지 않고 안정적인 도메인 카드 20~40개로 시작한다.
- 10k+ code node / 40k+ edge에서도 UI가 멈추지 않고 선택 주변만 상세화한다.
- 새 엔진 버전은 contract fixture를 통과하기 전 제품에 번들되지 않는다.
- 정상 종료, 엔진 실패, stale snapshot, 손상된 workspace 중 어느 경우에도 데이터 손실이나 가짜 fallback이 없다.

## Decision: Repository Boundaries

Context:

- 제품과 두 엔진의 릴리스 주기와 소유권이 다르다.
- 외부 코드 엔진을 제품 저장소에 vendor하면 업데이트와 라이선스 추적 비용이 커진다.
- 자체 DB 엔진은 제품 요구에 따라 CLI contract 확장이 필요하다.

Options:

- A: 세 소스를 하나의 monorepo로 합친다.
  - 장점: 한 번에 빌드 가능.
  - 단점: 외부 upstream history와 자체 엔진 release 경계가 무너진다.
- B: 제품, DB 엔진, 외부 코드 엔진을 독립 저장소로 유지한다.
  - 장점: 소유권, 버전, 라이선스, rollback이 명확하다.
  - 단점: artifact manifest와 contract test가 필요하다.

Decision:

- B를 선택한다.
- `shinyeonjun/visual_map`에는 `backend_map` 제품 소스만 둔다.
- `db_mcp`는 `shinyeonjun/rdb-memory-mcp`에서 독립 release한다.
- `codebase-memory-mcp`는 official upstream release를 사용한다.

Consequences:

- 엔진 통합은 source dependency가 아니라 versioned executable contract다.
- 제품 저장소 CI는 두 엔진 버전, URL, SHA-256, expected contract version을 manifest로 관리해야 한다.

## Decision: Engine Binary Distribution

Context:

- 코드 엔진 EXE는 GitHub 일반 파일 제한을 넘는다.
- release build는 두 엔진을 Tauri resource로 포함해야 한다.
- 개발자와 CI가 같은 byte를 사용해야 재현성이 생긴다.

Options:

- A: Git LFS로 EXE를 추적한다.
- B: GitHub Release asset에서 다운로드하고 checksum을 검증한다.
- C: 항상 로컬 경로를 수동 지정한다.

Decision:

- B를 기본으로 선택한다.
- 저장소에는 작은 engine manifest와 PowerShell 준비 스크립트만 둔다.
- 로컬 override인 `BACKEND_VISUAL_MAP_ENGINE_DIR`는 개발·긴급 rollback 용도로 유지한다.

Consequences:

- offline source build는 사전 다운로드 cache가 필요하다.
- release asset이 사라지거나 checksum이 바뀌면 build가 즉시 실패해야 한다.
- GitHub는 큰 바이너리 배포에 Release asset 사용을 안내하며, release asset은 파일당 2GiB 미만을 지원한다.

Sources:

- https://docs.github.com/en/repositories/working-with-files/managing-large-files/about-large-files-on-github
- https://docs.github.com/en/repositories/releasing-projects-on-github/about-releases

## Decision: Canonical Product Contract

Context:

- 현재 Rust와 TypeScript에 중복된 snapshot 변환이 있다.
- raw engine JSON을 화면이 직접 이해하면 엔진 upgrade마다 제품 의미가 흔들린다.

Options:

- A: raw engine payload를 프론트엔드까지 전달한다.
- B: Rust adapter가 versioned canonical inventory/evidence contract로 변환한다.

Decision:

- B를 선택한다.
- 엔진별 raw JSON은 Rust adapter 경계 밖으로 노출하지 않는다.
- snapshot 생성, normalization, dedupe, source location 보존은 Rust에서 한 번만 수행한다.
- TypeScript는 canonical `InventorySnapshot`과 `VisualMap`만 소비한다.

Consequences:

- 현재 TypeScript `buildInventorySnapshot`은 단계적으로 제거된다.
- snapshot schema version과 backward migration이 필요하다.
- contract fixture는 실제 bundled EXE 출력으로 생성하되 secret과 절대 사용자 경로를 제거한다.

## Decision: Truth And Confidence Model

Context:

- senior 사용자는 false positive 자체보다 false certainty를 더 위험하게 본다.
- code→DB 관계는 여러 약한 신호의 조합이다.

Decision:

- 관계 판정은 네 종류로 고정한다.
  - `confirmed`: 엔진 CALLS/HANDLES, DB constraint/index처럼 직접 읽은 관계.
  - `structural`: parent/contains, deterministic group membership처럼 앱이 구조적으로 유도한 관계.
  - `candidate`: SQL literal, repository name, route token, call proximity 등 근거가 있으나 확정하지 못한 관계.
  - `unknown`: 필요한 source/edge/capability가 없어 판단할 수 없는 구간.
- 숫자 confidence는 내부 ranking에만 사용하고 UI는 high/medium/low와 근거 목록을 표시한다.
- candidate가 confirmed로 자동 승격되는 규칙은 만들지 않는다.

Consequences:

- `unknown`은 빈 화면이 아니라 제품 데이터다.
- 모든 candidate에는 evidence kind, source location, target DB object, ranking reason이 필요하다.

## Decision: Projection And UI Ownership

Context:

- 제품 가치는 raw graph 시각화가 아니라 질문별 답이다.
- 현재 backend projection과 frontend inventory rendering이 이중화돼 있다.

Decision:

- Rust `VisualMap` projection을 화면의 유일한 node/edge source로 만든다.
- 화면별 고정 문법을 유지한다.
  - Architecture: domain card → top routes/code/tables → `+N`.
  - API Flow: Route → Handler → Function/Service → Repository/Query → DB candidate.
  - Change Impact: direct DB → code candidate → unknown → recommended checks.
- Atlas group click은 group membership query를 통해 focus projection을 새로 만든다.
- React는 layout/selection/accessibility만 책임지고 관계 의미를 재계산하지 않는다.

Consequences:

- `AtlasCanvas`의 inventory 기반 ranking/filter helper는 projection metadata로 대체된다.
- UI와 backend가 서로 다른 node cap/ranking을 갖지 않는다.

## Decision: DB Engine Evolution

Context:

- `db_mcp`는 자체 엔진이며 core에는 impact analysis와 relationship trace가 이미 있다.
- 현재 제품이 번들하는 CLI는 index/describe/find만 노출한다.

Decision:

- DB core graph를 제품 앱에서 직접 읽지 않는다.
- `db_mcp` CLI에 versioned JSON command를 추가한다.
  - schema capabilities/version
  - describe object with all constraints/indexes
  - impact analysis
  - relationship trace
- CLI JSON contract는 golden fixture와 semantic version으로 고정한다.

Consequences:

- DB 엔진 변경과 제품 adapter 변경을 각각 release하고 compatibility matrix로 묶는다.
- `unsafe-row-sampling` feature는 제품 release에서 금지하고 CI가 비활성 상태를 검사한다.

## Decision: Source Jump

Context:

- 복사는 가능하지만 개발자의 실제 작업 지점인 IDE로 이동하지 못한다.
- 임의 executable/argument 실행은 새로운 trust boundary다.

Decision:

- VS Code와 Cursor를 명시적 allowlist editor로 지원한다.
- 앱이 구성한 absolute repo-contained path와 양의 line/column만 전달한다.
- shell string을 만들지 않고 executable + argument array로 실행한다.
- workspace root 밖 경로와 임의 command template은 기본 제품에서 허용하지 않는다.

Consequences:

- source location은 snapshot 필수 필드가 된다.
- editor 미설치 시 복사와 탐색기 열기로 안전하게 degrade한다.

## Decision: Operations And Distribution

Context:

- 현업 도구는 설치 성공뿐 아니라 upgrade, rollback, 진단이 필요하다.
- 로컬 코드/DB 메타데이터는 민감하므로 telemetry 기본 수집은 부적절하다.

Decision:

- telemetry는 기본적으로 없다.
- 사용자가 직접 내보내는 redacted diagnostics bundle만 제공한다.
- GitHub Actions Windows runner에서 typecheck, build, Rust tests, real engine fixture, installer build를 실행한다.
- Node/Rust toolchain, lockfile 사용, engine provenance와 product version 일치를 CI에서 고정한다.
- release는 pinned sidecar checksum, SBOM/license notices, installer signature, smoke evidence가 모두 있어야 생성한다.
- updater는 서명된 artifact와 rollback 절차가 준비된 뒤에만 활성화한다.

Consequences:

- Windows code signing은 public release gate다.
- Tauri updater는 서명 검증을 요구하므로 key 관리와 recovery 절차가 필요하다.

Sources:

- https://v2.tauri.app/distribute/windows-installer/
- https://v2.tauri.app/distribute/sign/windows/
- https://v2.tauri.app/plugin/updater/
- https://v2.tauri.app/security/capabilities/

## Rollout And Rollback Principles

- 각 phase는 snapshot schema/engine contract/UI 중 하나의 위험 경계만 바꾼다.
- 새 engine version은 manifest 한 줄 rollback으로 이전 checksum artifact로 되돌릴 수 있어야 한다.
- snapshot schema migration 실패 시 원본 cache를 삭제하지 않고 재인덱싱 선택지를 제공한다.
- feature가 불완전하면 fake fallback 대신 이전 projection 또는 명시적 unavailable state를 사용한다.
- public release 전에는 내부 alpha → 실저장소 beta → signed release candidate 순으로 승격한다.

## Verification Strategy

- Unit: normalization, dedupe, ID stability, confidence/evidence rules.
- Contract: 실제 두 EXE의 JSON fixture와 adapter parser.
- Projection: node cap, no dangling edge, deterministic order, unknown gap.
- UI: empty/loading/error/stale/narrow screen/keyboard/source jump.
- Security: secret persistence, path traversal, command allowlist, row-data query scan.
- Performance: 10k/40k 기준 synthetic snapshot과 최소 세 개의 실제 backend repository.
- Release: clean Windows profile installer, no PATH, signed binary, uninstall, rollback.

## Research Stop

현재 결정으로 첫 contract/data-model 단계는 안전하게 시작할 수 있다. 제품 라이선스 형태, 코드 서명 공급자, 자동 업데이트 채널은 public release phase 전에 소유자 결정이 필요하지만 초기 정확도·projection 구현을 막지 않는다.
