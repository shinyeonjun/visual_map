# 지원 언어 공통 품질 구현 계획

Status: In Progress — common flow/DB baseline and capability status projection complete; pack-wide flow and live DB matrix remain
Scale: Large

## Goal

모든 active 지원 언어가 동일한 핵심 분석 계약과 동일한 실패 처리 품질을 통과하도록 한다. 지원 언어·framework·ORM 범위가 실제 provider/pack/CI와 어긋나지 않게 하고, Project/API/Code/DB 화면이 하나의 증거 그래프와 품질 상태를 공유하게 한다.

## Current Facts

- `code_memory/rust/src/model.rs`의 language bridge는 현재 12개 언어를 고정한다.
- `code_memory/packs/framework`도 12개 언어와 84개 pack 기준이다.
- 제품 지원 계약은 active 12개 언어와 target-only Kotlin/Swift를 분리한다.
- language semantic gate는 12개 언어의 cross-file `CALLS`와 source range를 검증한다.
- framework provider gate는 84개 pack의 detection, fact, source range, handler ownership을 검증한다.
- framework provider gate는 전체 내부 호출·DB 경로를 보장하지 않는다.
- framework flow gate는 12개 활성 언어의 대표 pack에서 엔트리포인트 → 핸들러 → 서비스
  cross-file `CALLS`, source range, duplicate-free 관계를 검증한다.
- 공통 DB evidence 테스트는 12개 활성 언어의 대표 정적 SQL 실행식과 exact table/column
  join 전제의 언어별 문법 차이를 같은 판별기로 검사한다.
- Tauri code inventory는 unresolved call을 gap으로 보존하고, API flow는 confirmed HANDLES/CALLS만 따른다.
- architecture projection은 language/provider coverage와 framework adapter status를 공통
  `languages`/`frameworks` 요약으로 보존하고, 코드 패널에서 동일 상태를 표시한다.

## Proposed Behavior

- active/target language 집합을 별도로 표시한다.
- active 언어는 공통 core gate를 모두 통과해야 supported로 표시한다.
- provider/framework/DB 실패는 공통 상태와 gap schema로 전달한다.
- confirmed 관계는 source endpoint, range, strategy, target evidence가 없으면 생성하지 않는다.
- API focused flow는 확정 prefix를 보여주고 끊긴 지점과 원인을 보존한다.
- Project 화면은 언어별 feature matrix를, API 화면은 flow quality를, Code 화면은 symbol evidence를, DB 화면은 exact join 상태를 보여준다.

## Success Criteria

1. 문서·bridge·pack·CI의 active language 목록이 일치한다.
2. active 언어 전체가 uniform core quality gate를 통과한다.
3. language별 core fixture에서 expected direct call, source range, duplicate-free graph가 일치한다.
4. provider failure, unsupported, partial, stale가 모든 언어에서 동일하게 분류된다.
5. framework pack fixture가 fact뿐 아니라 최소 route-to-handler-to-service flow를 검증한다.
6. DB 관계는 exact snapshot match 없이는 confirmed가 되지 않는다.
7. 실프로젝트 결과가 fixture 규칙과 다른 경우 release gate가 통과하지 않는다.
8. Kotlin/Swift는 provider와 동일 gate가 준비되기 전까지 active supported로 표시되지 않는다.

## Non-Goals

- 정적 분석으로 모든 reflection/dynamic dispatch를 확정하지 않는다.
- 이름만으로 관계를 보완하지 않는다.
- 언어별 UI·별도 truth model을 만들지 않는다.
- row data, secret, raw connection string을 저장하지 않는다.
- 모든 framework 버전을 한 번에 인증한다고 약속하지 않는다.

## Architecture

```text
Provider output
  -> common semantic relation contract
  -> framework/ORM capability adapter
  -> normalized snapshot + stable gap IDs
  -> API/Code/DB/Project projections
  -> identical UI states and evidence
```

공통 core와 capability를 분리한다.

- Core: coverage, symbols, imports, direct calls, ranges, deterministic IDs, diagnostics
- Framework: entrypoint, middleware, handler, DI, events, RPC
- Data: query, DbReference, exact database join
- Projection: focused flow, code tree/call graph, table usage, capability dashboard

## Implementation Phases

### Phase 1: 범위·계약 정합성 고정

Goal:
- active 12개와 target 14개를 혼동하지 않게 한다.

Deliverables:
- language/pack/catalog/document 목록 비교 gate
- uniform core quality contract와 CI 명령 추가
- Kotlin/Swift는 provider·fixture가 없는 동안 target/unsupported 상태로 분리

Verification:

```powershell
.\tests\gates\run-uniform-core-quality-gate.ps1
.\tests\gates\run-framework-pack-gate.ps1
.\tests\gates\run-language-semantic-gate.ps1
```

Rollback:
- 새 gate만 CI에서 비활성화하고 기존 분석 출력은 변경하지 않는다.

### Phase 2: 공통 실패·근거 전달

Goal:
- provider부터 UI까지 같은 `gap_id`, status, source evidence를 보존한다.

Deliverables:
- provider diagnostic → inventory gap → snapshot link → API answer 연결
- missing/failed/partial/stale/unsupported 분류 표준화
- unresolved call의 caller range와 target 후보 보존

Verification:
- provider 실패, 대상 미존재, 동명이인, DB snapshot 없음 fixture
- 동일 실패가 언어별로 같은 상태와 UI 문구로 표시

Rollback:
- 새 진단 필드는 optional로 읽고, 기존 unknown fallback을 유지한다.

### Phase 3: 언어 공통 core conformance

Goal:
- active 모든 언어에서 파일→심볼→직접 호출→근거의 동일 baseline 확보

Deliverables:
- 언어별 동일 의미 fixture
- expected stable relation set
- positive/negative/ambiguous/metamorphic 변형
- deterministic output comparison

Verification:
- 12개 active language strict gate
- provider 없는 언어는 성공 처리하지 않음
- 모든 confirmed relation의 endpoint/range/source validation

Rollback:
- baseline을 통과하지 못한 language는 supported에서 partial/unsupported로 강등한다.

### Phase 4: framework flow conformance

Goal:
- pack detection이 아니라 route/event→handler→service까지 실제 흐름을 검증

Deliverables:
- 각 pack의 최소 flow fixture
- middleware 순서와 handler ownership
- DI ambiguity/duplicate handler negative fixture
- controller/service/repository cross-file call fixture

Verification:
- Express, FastAPI, Spring부터 real project와 fixture 비교
- 이후 각 active pack으로 확장

Rollback:
- 인증되지 않은 pack은 structural detection만 제공하고 HANDLES/flow confirmed를 내보내지 않는다.

### Phase 5: query/DB 통합 conformance

Goal:
- 모든 언어에서 code-side query evidence와 DB exact join의 실패 품질 통일

Deliverables:
- static SQL/ORM/DbReference 공통 shape
- exact/ambiguous/missing/stale DB join 상태
- table/column READS/WRITES evidence

현재 진행:
- 공통 semantic linker가 JavaScript/TypeScript, Python, Java, C#, C/C++, Go, Rust,
  PHP, Ruby, Dart의 대표 정적 실행식을 같은 `READS`/`WRITES`/`USES_COLUMN` 계약으로
  처리하도록 conformance 테스트를 고정했다.
- C 계열의 `sqlite3_exec`/`sqlite3_prepare_v2`처럼 SQL이 두 번째 인자인 native call과
  PHP PDO의 `->query`를 bounded execution marker로 추가했다.
- 동적 SQL, generic receiver, 다중 schema/column ambiguity, CTE/multi-statement는
  계속 확정하지 않는 negative 테스트를 유지한다.

Verification:
- SQL literal, ORM mapping, dynamic SQL, duplicate schema/table, missing DB fixture

Rollback:
- DB join이 모호하면 code-side candidate만 유지하고 confirmed를 차단한다.

### Phase 5/6 현재 결과

- 대표 12개 active 언어의 route/event → handler → service flow가 `12/12` 통과했다.
- 대표 12개 active 언어의 정적 query 형태가 exact table/column snapshot join 테스트를 통과했다.
- architecture schema가 `v2`로 올라가며 언어 상태, 파일 coverage, framework adapter 상태,
  fact/relation count를 공통으로 전달한다.
- 코드 패널은 이 요약을 `정상/부분/확인 필요`로 표시하고, 확정 관계가 아닌 gap은 기존
  `partial` 상태와 함께 유지한다.

### Phase 6: Project/API/Code/DB 통합 UI

Goal:
- 4개 화면이 같은 snapshot과 capability matrix를 사용

Deliverables:
- Project: language/framework/feature quality matrix
- API: focused flow, branch, gap, flow quality
- Code: directory + symbol + call neighborhood
- DB: exact schema + code access evidence
- 모든 선택에서 source line과 evidence 역추적

Verification:
- 0/1/many language, mixed-language monorepo, partial provider, large graph
- visual map과 code explorer의 stable ID 일치

Rollback:
- UI는 기존 map projection을 fallback으로 유지하고 새 capability panel만 숨긴다.

진행 상태:
- Code 화면의 분석 품질 요약과 언어/framework별 상태 상세를 구현했다.
- Project/API/DB의 별도 projection은 기존 snapshot/gap 계약을 계속 사용한다.
- 전체 4면 공통 capability matrix와 pack별 고급 flow 표시는 아직 Phase 6 후속 범위다.

### Phase 7: release hardening

Goal:
- 동일 품질 기준을 CI·installer·실프로젝트 release gate로 고정

Deliverables:
- strict provider gate without missing-provider bypass
- pack/provider/external project matrix
- output checksum/source revision/stale cache check
- per-language quality report and release receipt

Verification:

```powershell
.\tests\gates\run-uniform-core-quality-gate.ps1
.\tests\gates\run-framework-provider-gate.ps1
.\tests\gates\run-external-project-gate.ps1 -ProjectRoot <fixture>
```

Rollback:
- 실패한 language/capability만 releaseReady=false 또는 partial로 내리고 다른 결과를 숨기지 않는다.

## Test Plan

- unit: ID normalization, range, duplicate, status classification
- contract: common output shape for every active language
- negative: dynamic dispatch, ambiguity, missing provider, stale, missing DB
- metamorphic: rename/move/reorder/unrelated same-name/extra branch
- differential: repeated run and provider-output normalization comparison
- integration: node-express-boilerplate, spring-petclinic-microservices, FastAPI fixture
- performance: file/symbol/edge cap, timeout, cancellation, memory

## Risks And Assumptions

- Kotlin/Swift provider 추가는 별도 큰 작업이며, gate 없이 문서만 14개로 유지하지 않는다.
- 동일 품질은 동일 문법이 아니라 동일 core contract와 failure semantics를 뜻한다.
- framework/ORM 고급 지원은 capability별 인증이며 프로젝트 전체 complete를 자동으로 의미하지 않는다.
- 현재 dirty UI 변경과 이 계획의 provider 계약 변경은 서로 다른 commit 단위로 분리한다.

## Codex/Claude Prompt

```text
Read this plan and the uniform core quality contract. Implement Phase 1 only.
First inspect active languages in the bridge, framework catalog, support docs,
and CI. Do not add Kotlin/Swift support by guessing. Add the smallest strict
consistency gate and tests, preserve existing output compatibility, and report
the exact active/target language mismatch and verification commands.
```
