# meeting-overlay-assistant 분석 결과 감사

작성일: 2026-08-10
원본 저장소: `D:\meeting-overlay-assistant`
분석 제품: `D:\project\visual_map`

## 1. 결론

현재 엔진은 **파일 목록과 근거를 정직하게 보존하는 데는 강하지만, 코드의 실행 구조를 충분히 복원하지는 못한다.**

- 지원 언어의 코드·테스트 파일 1,037개는 모두 현재 스냅샷과 일치하며 Fact Graph에 파일 단위로 등록됐다.
- 저장된 evidence의 파일·해시·줄 범위는 전수 대조에서 모두 유효했다.
- FastAPI의 일반 HTTP route 53개는 현재 공식 서버 기준으로 모두 찾았다.
- AI가 만든 58개 의미 영역은 형식·참조 무결성은 모두 통과했고, 54개 의미 이름도 대체로 배정된 경로의 책임과 맞는다.

그러나 다음은 제품 핵심 목표에 비해 미달이다.

- Python 원본 정의 4,319개 중 canonical graph에 남은 것은 3,017개로, 독립 AST 기준 recall은 69.85%다. 특히 메서드 31.62~40.19%, 현재 서버 생성자 0%가 가장 큰 결함이다.
- 독립적으로 확정 가능한 Python project-local 호출 2,442건 중 816건만 static graph에서 확인됐다. 이는 보수적인 하한 측정에서도 33.42%다.
- 현재 59개 대표 TracePath는 전부 `gap`이고, API에서 DB까지 완성된 경로는 0개다.
- DB·table·query node와 reads/writes/executes-query 관계가 0개다. 포함된 SQL 6개는 census만 되고 사실 그래프에는 들어오지 않는다.
- AI는 관계를 복구하지 않는다. 현재 계약상 영역 이름·요약·계층·배정만 만들며, 정적 분석에서 빠진 코드 객체·간선·실행 단계를 새로 만들 수 없다.
- 프론트 overview가 실제로 받는 구현 node는 50개뿐이다. canonical node 4,806개의 약 1.04%, AI anchor 1,212개의 약 4.13%다.
- 프론트 검색도 58개 영역과 overview에 보이는 50개 node만 검색한다. 나머지 canonical symbol은 ID를 알지 못하면 사용자 경로에서 찾을 수 없다.
- `legacy/`의 참조용 코드 198개 파일이 현재 코드와 함께 분석되어 26개 의미 영역에서 current와 legacy가 섞였다.

따라서 현재 제품을 한 문장으로 판정하면 다음과 같다.

> **구조 영역 지도와 근거 조회는 부분 합격, API부터 코드 내부·저장소까지의 실행 흐름 지도는 불합격이다.**

## 2. 감사 대상 스냅샷

문서나 과거 로그가 아니라 현재 원본과 실제 저장물을 비교했다.

| 항목 | 값 |
|---|---|
| 원본 Git HEAD | `49f515cd5723fc9b5f57d0bbfcf3419f564d95a7` |
| 현재 Source Manifest digest | `b7e8540d5b01986162bb49bd93f632517fa6a5092d30aafe82fd7238602fc317` |
| 저장 Fact snapshot | `snapshot-a7f4734e3da297ff038b8bd0ae40e47b0d863cb7518ad0584b207d813ebd7786` |
| 저장 canonical SQLite | `canonical-8823565edec0452b0269bdcf80a903a187ab6d5ed752a45bef5eed4de6942888.sqlite` |
| 현재 semantic revision | `semantic-revision-ad09fb58e27b0ed60fcfef27dc2c314d5e8f84139b1d211f124e0c827589a134` |
| AI | `gpt-5.6-terra`, high |
| prompt policy | `base-semantic-policy-v5` |

현재 원본에서 manifest를 새로 만들고 1,066개 included 파일의 digest를 다시 계산한 결과, 저장 snapshot과 다른 파일은 0개였다. 따라서 이 보고서는 오래된 cache를 현재 코드로 오인한 결과가 아니다.

원본 worktree에는 생성 schema 수정과 `.code_memory/`, 일부 tsconfig 변경이 있으나, 이 변경까지 포함한 현재 파일 digest가 저장 snapshot과 일치한다.

## 3. 모든 파일의 처리 범위

### 3.1 Source Census

| 상태 | 파일 | 의미 |
|---|---:|---|
| 전체 열거 | 1,240 | 제외 규칙 밖에서 발견한 전체 파일 |
| included | 1,066 | manifest와 digest에 포함 |
| excluded | 38 | 문서 34개와 민감 설정 예시 4개 |
| unsupported | 136 | 현재 언어/파일 adapter가 분석하지 못하는 형식 |

included는 112,004줄, 96,442 non-blank lines, 약 3.89MB다.

included 1,066개 중 실제 언어 provider와 canonical Fact Graph까지 간 것은 다음 1,037개다.

| 언어 | 코드·테스트 파일 |
|---|---:|
| Python | 884 |
| JavaScript | 149 |
| Rust | 4 |
| 합계 | 1,037 |

나머지 included 29개는 config 19, SQL 6, deployment 4다. 이 파일들은 census와 hash에는 들어가지만 code facts, AI region, frontend map에는 들어가지 않는다.

### 3.2 분석하지 않는 주요 형식

| 형식 | 파일 | 원본 줄 수 | 현재 처리 |
|---|---:|---:|---|
| PowerShell | 60 | 2,451 | unsupported. 실행·배포 흐름 없음 |
| CSS | 9 | 7,188 | unsupported. UI 구조·스타일 관계 없음 |
| JSON | 48 | 13,145 | unsupported. schema/config 의미 없음 |
| HTML | 3 | 535 | unsupported |
| SQL | 6 | 2,475 | included지만 census만 수행 |

오디오 fixture, icon, lockfile 같은 바이너리·생성물은 코드 지도의 범위가 아니므로 분석하지 않는 것이 정상이다. 반면 PowerShell, CSS, JSON contract, SQL은 사용자가 기대하는 범위에 따라 명시적인 미지원 capability로 보여줘야 한다.

### 3.3 경로별 실제 산출물

`defs`는 file node를 제외하고 canonical에 남은 정의, `call`은 calls+constructs, `visible`은 현재 overview MapView에 실린 구현 node다.

| 경로 묶음 | 코드 파일 | 줄 | file coverage | defs | call | imports | type | AI anchors | visible |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| `server/app` 현재 백엔드 | 591 | 41,567 | 591 | 1,464 | 1,365 | 1,237 | 0 | 674 | 35 |
| server tools/experiments | 22 | 8,787 | 22 | 310 | 434 | 53 | 0 | 40 | 0 |
| `tests/server` | 96 | 14,220 | 96 | 683 | 78 | 309 | 0 | 0 | 0 |
| `client/overlay` | 88 | 6,802 | 88 | 344 | 576 | 184 | 11 | 185 | 4 |
| `client/web` | 23 | 4,110 | 23 | 84 | 54 | 31 | 0 | 61 | 3 |
| `legacy/backend` | 166 | 14,907 | 166 | 587 | 295 | 24 | 336 | 166 | 3 |
| `legacy/frontend` | 32 | 3,454 | 32 | 183 | 303 | 70 | 7 | 58 | 3 |
| root/other | 19 | 1,197 | 19 | 60 | 17 | 15 | 0 | 16 | 1 |

1037개 code/test 파일 중 canonical definition이 0개인 파일은 255개, call/construct/import/type 관계가 0개인 파일은 284개다. `__init__.py`, 상수 전용 파일 등이 포함되므로 이 수치 자체가 오류는 아니지만, 파일이 indexed라는 상태만으로 의미 분석이 충분하다고 판단하면 안 된다.

## 4. 정적 Fact Graph 감사

### 4.1 저장된 전체 크기

| 테이블/사실 | 수 |
|---|---:|
| nodes | 4,806 |
| edges | 10,662 |
| evidence | 10,495 |
| edge-evidence links | 10,729 |
| file coverage | 1,037 |
| capability receipts | 77 |
| gaps | 466 |
| issues | 0 |

모든 7개 AnalysisPlan unit의 상태는 `partial`이다. `issues=0`을 분석 완전성으로 읽어서는 안 된다.

### 4.2 근거 무결성

전수 검사 결과는 강하다.

- evidence path 누락: 0
- evidence content digest 불일치: 0
- 줄 범위가 원본 밖인 evidence: 0
- calls/constructs 3,122개 중 call-site token과 target display name이 다른 관계: 0
- 원본 문맥 수동 표본 40건에서 잘못 연결된 target: 0
- current에서 legacy를 직접 호출하거나 import한 관계: 0

이는 **이미 만들어진 관계의 근거 품질이 높다**는 뜻이다. 모든 실제 관계를 찾았다는 뜻은 아니다. 40/40 표본도 전 관계의 precision 100%를 증명하지 않는다.

### 4.3 정의 coverage

독립 parser로 원본을 다시 읽어 canonical node와 path·line·name을 대조했다.

| 언어 | 독립 ground truth | canonical match | recall | 해석 |
|---|---:|---:|---:|---|
| Python | 4,319 | 3,017 | 69.85% | method/constructor 대량 누락 |
| JavaScript | 659 named definitions | 598 | 90.74% | helper·handler 일부 누락 |
| Rust | 33 named definitions | 33 | 100% | 단 4파일의 작은 corpus |

Python 세부 결과가 핵심이다.

| 범위 | classes | functions | methods | constructors | test cases | 전체 |
|---|---:|---:|---:|---:|---:|---:|
| 현재 `server/app` | 359/364 | 815/823 | 290/917 | 0/102 | - | 1,464/2,206 |
| `legacy/backend` | 179/179 | 181/186 | 168/418 | 31/51 | - | 559/834 |
| tests | 192/204 | 43/71 | 5/170 | 0/69 | 461/461 | 701/975 |

원인은 provider가 정의를 못 뽑은 것만이 아니다. provider receipt는 Python definition 6,921개를 emitted했다고 기록하지만 canonical relevance gate가 parent가 있는 method/constructor를 기본 보존 대상에서 제외한다. 관계 endpoint로 다시 표시된 일부만 남는다.

관련 구현은 `code_memory/rust/src/static_pipeline/canonical/linker.rs`의 `baseline_relevant`와 `baseline_definition_kind`, `canonical/store.rs`의 `retain_relevant_nodes_and_evidence`다.

### 4.4 조용한 parse 실패

Python 884개 중 882개는 표준 AST parse에 성공했고 2개는 실제 문법 오류가 있다.

- `server/experiments/stt/benchmark_live_stream_runtime.py:26`
- `server/experiments/stt/benchmark_live_stream_runtime_actual.py:80`

두 파일 모두 현재 file coverage에는 `indexed`, gap 없음으로 저장됐다. 일부 정의도 provider 복구 결과로 남아 있다. 부분 복구 자체는 가능하지만, parse 실패 사실을 file gap으로 남기지 않은 것은 정직성 결함이다.

### 4.5 import와 호출 관계

현재 공식 Python 경로의 project-local import를 독립 resolver로 대조한 하한은 다음과 같다.

- `server/app`: 1,097/1,148 = 95.56%
- tests: 313/328 = 95.43%
- server tools: 52/54 = 96.30%

반면 project-local call/construct는 크게 부족하다.

| 범위 | 독립적으로 확정 가능한 호출 | static match | 보수적 recall 하한 |
|---|---:|---:|---:|
| 현재 `server/app` | 761 | 323 | 42.44% |
| `legacy/backend` | 657 | 129 | 19.63% |
| tests | 459 | 41 | 8.93% |
| server tools | 565 | 323 | 57.17% |
| 합계 | 2,442 | 816 | 33.42% |

이 resolver는 같은 파일·명시 import·`self`·명시 type local처럼 안전하게 확정 가능한 호출만 세었다. 그러므로 33.42%는 전체 Python의 절대 recall이 아니라, **추측 없이도 찾을 수 있었던 관계 중 이미 놓친 비율을 보여주는 하한**이다.

Python unit에는 다음 명시적 gap이 저장돼 있다.

> large-workspace semantic enrichment limited for 884 source files; declarations and imports retained, non-boundary per-symbol reference and lexical queries skipped

대형 프로젝트 budget 때문에 non-boundary per-symbol query를 생략한 결과이며, 현재 제품 목표인 실행 흐름과 직접 충돌한다.

### 4.6 API·framework·DB·test

| capability | 결과 | 판정 |
|---|---|---|
| 현재 FastAPI REST | 53/53 route+handler | 합격 |
| 현재 WebSocket | 0/2 | 미지원 |
| DB/table/query facts | 0 | 불합격 |
| reads/writes/executes-query | 0 | 불합격 |
| Service/Repository node role | 0 | 불합격 |
| handler role | 69 | 동작 |
| Python test 연결 | 14/461 test cases | 매우 부족 |
| JavaScript test 연결 | 3/4 test cases | 부분 |

SQL 6개가 manifest에 포함돼도 DB adapter가 실행되지 않으므로 UI mockup의 `API → controller → service → repository → table` 중 마지막 table 층은 현재 데이터로 만들 수 없다.

## 5. TracePath 실사

### 5.1 대표 trace

- representative traces: 59
- 상태: `gap` 59, complete 0, partial 0
- 고유 HTTP 진입점: 37/53
- 길이: 2~4 facts
- 대표 trace가 없는 HTTP route: 16개

모든 Python direct-call capability에 analysis-unit gap이 적용되므로, 관계가 일부 이어져도 현재 trace 상태는 정직하게 `gap`이 된다.

### 5.2 선택 상세에서 다시 계산하는 trace

선택 상세는 overview의 59개만 재사용하지 않는다. SQLite에서 선택한 진입점을 다시 읽어 다음 한도로 계산한다.

- area당 후보 진입점 최대 4개
- path 최대 8개
- depth 최대 16
- 진입점당 expansion 최대 8,192

현재 저장 graph로 같은 규칙을 독립 재현한 결과:

- 53/53 HTTP route는 최소 한 경로를 반환한다.
- 49/53은 첫 8개 후보 중 `server.app.services` 경로에 한 번 이상 도달한다.
- 11/53은 repository 또는 persistence 경로에 한 번 이상 도달한다.
- DB/table/query terminal에 도달하는 route는 0/53이다.
- 최장 후보도 7 facts다.

즉 프론트 제한 때문에만 짧은 것이 아니다. graph 자체가 중간에서 끊긴다.

### 5.3 `POST /api/v1/sessions` 원본 대조

원본의 핵심 실행 순서는 다음과 같다.

```text
POST /api/v1/sessions
→ routes.session.lifecycle.create_session
→ SessionService.create_session_draft
→ SessionCoordinator.create_session_draft
→ PostgreSQLSessionRepository.save
→ upsert_session / replace_session_participants
→ PostgreSQL transaction
```

현재 Fact Graph는 다음까지만 이어진다.

```text
POST /api/v1/sessions
→ create_session
→ SessionService.create_session_draft
→ 끝(gap)
```

`SessionCoordinator.create_session_draft` node는 canonical relevance gate에서 사라졌고, `PostgreSQLSessionRepository.save` node는 존재하지만 호출 간선이 없다. `upsert_session`도 존재하지만 저장소에서 이어지는 호출 간선이 없다.

따라서 AI나 프론트가 표시 방법을 바꿔도 이 경로를 사실로 복원할 수 없다.

## 6. AI 의미 분석 감사

### 6.1 AI가 실제로 받은 것

AI는 원본 930개 production source 파일 전체를 그대로 읽지 않는다.

| 입력 | 수 |
|---|---:|
| structural regions | 164 |
| anchors | 1,212 |
| boundary bundles | 484 |
| representative traces | 59 |
| source excerpts | 48개 파일, 233줄 |
| tests | 입력 제외 |

region 합계는 production source 930개 파일과 68,074 effective LOC를 구조적으로 덮는다. 그러나 347/930 source 파일은 anchor가 0개다. 이 파일은 주로 path root와 region aggregate를 통해서만 의미 분류에 영향을 준다.

source excerpt 233줄은 production effective LOC의 약 0.34%다. 따라서 “AI가 모든 코드를 읽고 의미를 이해했다”는 표현은 틀리다. 정확한 표현은 다음과 같다.

> AI가 정적 그래프의 압축 요약과 제한된 근거 조각을 보고 영역 이름과 묶음을 만들었다.

### 6.2 AI 출력 품질

| 항목 | 결과 |
|---|---:|
| areas | 58 (L0 16, L1 42) |
| semantic labels | 54 |
| structural fallback | 4 (`app` 2, `assistant`, `controllers`) |
| region assignments | 164/164, 정확히 한 번씩 |
| invalid fact/evidence/trace/parent reference | 0 |
| warnings | 0 |

이름과 요약은 배정된 path·anchor 책임과 대체로 맞는다. 다만 이는 영역 분류 품질이지 실행 관계 정확도가 아니다.

AI prompt는 정적 facts가 code identity, relation, count, dispatch, TracePath order의 주인이라고 명시하며, AI가 코드 객체·간선·실행 단계·DB 객체를 만드는 것을 금지한다. 이 원칙은 환각 방지에는 맞지만, 정적 관계 누락을 AI가 보완하는 구조는 아직 구현되지 않았다는 뜻이다.

### 6.3 current와 legacy 혼합

README는 `server/client/shared/deploy`를 공식 경로, `legacy/backend`와 `legacy/frontend`를 과거 참조본으로 명시한다. 그러나 docs는 Source Census에서 제외되고 `legacy/` 자체도 분석 scope에서 제외·격리되지 않는다.

- production regions에 들어간 legacy 파일: 198/930 = 21.29%
- current와 legacy가 함께 들어간 area: 26/58
- 그중 L0: 11개, L1: 15개

예: 회의 세션 관리, 보고서 처리, 데이터 영속화, 회의 오버레이, HTTP API 제공 등이 current와 legacy 구현을 같은 책임으로 합친다.

이는 AI naming 오류라기보다 **Source Census에 source lifecycle/currentness 계약이 없는 구조 오류**다. AI에게 이름만 더 잘 지으라고 해서는 해결되지 않는다.

## 7. 프론트로 실제 서빙되는 범위

### 7.1 overview MapView

| 항목 | 저장 원본 | overview 전달 |
|---|---:|---:|
| canonical nodes | 4,806 | 50 visible node instances |
| AI anchors | 1,212 | 47 anchor IDs visible |
| areas | 58 | 58, 그중 L0 16개를 box로 렌더 |
| top-level area relations | - | 90 directed pairs, count 합계 1,095 |
| overview trace | 59 candidates | 1개 area에 3-step gap trace |

MapView projection은 area마다 대표 trace 하나가 있으면 그 trace steps만, 없으면 정렬된 anchor 첫 하나만 보낸다. 나머지는 `hiddenNodeCount` 숫자로만 남는다.

현재 화면은 모든 사실을 처음부터 그리지 않는다는 점에서는 맞지만, 사용자가 필요할 때 숨은 symbol 전체를 찾는 read model이 없다.

### 7.2 검색

현재 global search는 다음만 순회한다.

- 58개 area의 name/originalName/summary
- area에 실제 전달된 50개 node의 name/kind
- 결과 최대 8개

canonical의 나머지 symbol, AI의 나머지 anchor, file path를 SQLite에서 검색하지 않는다. UI 문구는 `영역 · API · 심벌`이지만 실제로는 overview projection에 실린 심벌만 검색한다.

### 7.3 선택 상세와 근거

좋은 부분:

- area/anchor/fact ID 선택 시 SQLite에서 필요한 node, edge, evidence를 query한다.
- 관계 count는 화면에 그린 선이 아니라 전체 boundary bundle 집계를 사용한다.
- evidence는 실제 file:line을 열 수 있다.
- trace hop은 truth, dispatch, call-site, lexical order, guard/repeat/deferred/awaited 정보를 전달할 수 있다.

한계:

- 사용자가 hidden fact ID를 발견할 검색/탐색 경로가 없다.
- area trace는 최대 4개 진입점·8개 path로 제한된다.
- graph에 빠진 method/call/DB terminal은 상세 query로도 복구되지 않는다.
- Service/Repository role fact가 없어서 대부분 node role은 일반 `code`로 전달된다.

## 8. 기능별 최종 판정

| 사용자가 기대하는 정보 | 현재 상태 | 판정 |
|---|---|---|
| 프로젝트 파일을 빠짐없이 세기 | 지원 형식 1,037/1,037 | 합격 |
| 분석한 코드가 현재 원본인지 증명 | digest 0 mismatch | 합격 |
| 영역 이름과 큰 구조 | 58개 영역, 54개 semantic label | 부분 합격 |
| current와 legacy 구분 | 26개 area에서 혼합 | 불합격 |
| 함수·class 목록 | 대체로 양호 | 부분 합격 |
| method·constructor 목록 | Python 대량 누락 | 불합격 |
| import 구조 | 현재 Python 약 95.5% 하한 | 합격에 가까움 |
| 호출 관계 | Python 보수적 하한 33.42% | 불합격 |
| REST API → handler | 53/53 | 합격 |
| WebSocket → handler | 0/2 | 불합격 |
| handler → service → repository | 일부만 연결 | 불합격 |
| repository → SQL/table | 0 | 불합격 |
| 외부 API·queue·cache | facts 0 | 미구현/불합격 |
| test → 실제 코드 | 17/465 cases | 불합격 |
| 모든 symbol 검색 | overview 50개만 | 불합격 |
| 근거 file:line | 저장된 evidence 전수 유효 | 합격 |
| AI가 누락 관계 보완 | 계약상 하지 않음 | 미구현 |

## 9. 근본 원인과 수정 우선순위

### P0 — 실행 지도 성립 조건

1. **Canonical relevance gate 수정**
   - public/internal product method와 constructor를 관계 endpoint 여부와 무관하게 보존한다.
   - size 문제는 삭제가 아니라 SQLite lazy query와 visibility policy로 푼다.

2. **Python call graph의 대형 workspace 축약 제거**
   - 모든 파일의 AST inventory는 유지하고, call-site resolution을 file shard로 병렬 실행한다.
   - `self`, instance field, constructor injection, builder return type, protocol/interface dispatch를 증거 기반으로 연결한다.
   - 확정 불가 대상은 candidate 또는 typed gap으로 남긴다.

3. **API-to-persistence closed corpus gate 추가**
   - 최소한 현재 공식 route별로 handler→service→repository까지 수동 정답표를 만든다.
   - 완성 경로와 명시 gap을 분리 측정한다.

4. **DB facts 구현**
   - SQL DDL에서 table/column/index/constraint를 정적으로 추출한다.
   - repository SQL call-site를 query/table read/write에 연결한다.
   - row data는 읽지 않는다.

5. **Source lifecycle 계약**
   - `current`, `legacy/reference`, generated, vendor를 Source Census 단계에서 결정적으로 분리한다.
   - 기본 지도에는 current만 넣고 legacy는 별도 layer 또는 명시 선택으로 제공한다.

### P1 — 제품 탐색 완성

6. WebSocket/framework binding을 REST와 같은 계약으로 추출한다.
7. frontend action→API, job/event/external/cache capability를 실제 source evidence가 있을 때 생성한다.
8. Service/Repository/Controller 역할을 path 이름이 아니라 interface·constructor·framework evidence로 분류한다.
9. global search를 SQLite FTS/read model로 바꿔 retained symbol 전체를 검색한다.
10. area drill-down은 `hiddenNodeCount` 숫자가 아니라 페이지된 symbol·flow query를 제공한다.
11. parse 실패와 provider 부분 복구를 file-level gap으로 표시한다.
12. test 관계는 test case inventory와 target 연결을 분리하고, 미연결 분모를 UI에 보인다.

### P2 — 범위 선택

13. PowerShell·CSS·JSON·HTML을 제품 핵심 코드 지도에 넣을지 명시적으로 결정한다.
14. 넣지 않는 형식도 unsupported count와 이유를 사용자에게 보인다.

## 10. 재발 방지 acceptance gate

다음 gate가 모두 통과하기 전에는 “API부터 DB까지 실행 흐름을 제공한다”고 표현하지 않는다.

1. 모든 열거 파일이 included/excluded/unsupported 중 정확히 하나이며 digest가 재현된다.
2. parse 실패는 `indexed`만 남기지 않고 file gap을 가진다.
3. current/legacy 파일이 기본 semantic area에서 섞이지 않는다.
4. 고정 ground-truth corpus의 class/function/method/constructor를 종류별로 별도 측정한다.
5. 대표 API 시나리오는 route→handler→service→repository→terminal을 모두 file:line evidence로 검증한다.
6. 정적으로 확정할 수 없는 동적 dispatch는 AI가 exact confirmed로 승격하지 않는다.
7. frontend search가 retained public symbol 전체를 찾을 수 있다.
8. overview 축약 수치와 상세 query의 전체 분모가 서로 일치한다.
9. AI output의 모든 ID·parent·assignment·evidence reference가 verifier를 통과한다.
10. 같은 Fact Graph digest는 같은 승인 semantic revision을 재사용한다.

## 11. 측정 해석 주의

- independent AST 결과는 정의 recall 측정이다. 실행 시점의 동적 behavior 전체를 증명하지 않는다.
- Python call 33.42%는 보수적으로 확정 가능한 local call 집합에 대한 하한이다. 임의 저장소 전체 recall로 일반화하지 않는다.
- call-site token 3,122/3,122 일치는 evidence alignment다. 같은 이름의 잘못된 target 가능성까지 전수 증명한 것은 아니다.
- AI label의 의미 적합성은 path·anchor·evidence를 사람이 대조한 제품 감사 결과이며 수학적 정확도 점수가 아니다.
- Rust 100%는 4파일 corpus 결과다. 일반 Rust 프로젝트 전체 품질을 뜻하지 않는다.

이 보고서의 핵심은 하나의 종합 점수로 좋은 수치와 나쁜 수치를 섞지 않는 것이다. **inventory, definition, relation, trace, semantic grouping, frontend serving을 각각 별도 분모로 계속 측정해야 한다.**
