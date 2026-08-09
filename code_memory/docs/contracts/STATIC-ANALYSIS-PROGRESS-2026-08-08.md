# 정적 분석 전체 진행도 — 2026-08-08

> 역사적 감사 스냅샷입니다. 2026-08-09 canonical hard cut 이후의 현재
> 계약과 실행 방법은 `../README.md`, `LANGUAGE-SEMANTICS.md`,
> `CANONICAL-FACT-BUNDLE.md`를 따릅니다. 아래의 compatibility 출력과 삭제된
> PowerShell gate 설명은 당시 결함을 재현하기 위한 기록이지 현재 경로가 아닙니다.

마지막 코드·설치본 재검증: **2026-08-08**

## 결론

현재 구현은 **정적 원재료 수집 기반은 완성**, **10언어 핵심 의미 원재료는 검토 corpus 기준 통과**,
**Language IR을 하나의 canonical Fact Graph로 조립하는 코드 후반부와 백엔드 HTTP 경로·핸들러 baseline은
실제 실행 경로에 연결**된 상태다. canonical 단계는 provider-native identity를 먼저 등록하고 관계를 나중에
exact resolution하는 2-pass linker다. 이름 유사도나 경로 추측으로 endpoint를 만들지 않으며, 미등록·모호한
target은 typed gap으로 남기고 edge를 만들지 않는다. source/file/definition/relation/evidence/coverage/
capability를 deterministic relevance gate로 줄인 뒤, 저장소 밖 cache에 immutable SQLite bundle을 쓰고
SHA-256 manifest를 마지막에 게시한다.

snapshot identity는 Source Manifest, Analysis Plan, 실제 provider set과 실제 execution-context set을 같은
공식으로 사용한다. canonical release gate는 Language IR content digest·record count, definition/relation
회계, dangling endpoint 0, 근거 없는 confirmed 0, duplicate logical edge 0, 실제 SQLite byte digest,
외부 manifest를 매 실행 검증한다. 동일 입력 2회에서 semantic digest와 SQLite bundle digest가 같고,
사람용 diagnostic/evidence 문구만 바뀌면 semantic identity는 유지되되 payload identity는 달라진다.

정량 검증은 10언어 정의 117/117, 직접 CALLS/CONSTRUCTS 35/35, import/export site 39/39, type relation
90/90과 negative 22, test→production 10/10과 name-only negative 10/10을 유지한다. 정상 execution context는 10/10 exact이고 config 제거 variant는 false
exact 0이다. 언어별 provider-visible source를 1.1MB 이상으로 늘린 2회 gate에서도 정의 117/117,
CALLS/CONSTRUCTS 35/35와 canonical semantic/bundle digest 결정성이 함께 통과했다. 이 수치는 닫힌 검토
corpus의 회귀 정확도이며 임의의 실제 저장소 전체 정확도 100%를 뜻하지 않는다.

## 실제 제품 저장소 재검증 — nested TypeScript root 오류 수정

`D:\meeting-overlay-assistant`를 desktop과 같은 managed-provider 조건으로 다시 분석했다. 저장소 root의
`tsconfig.json`을 참고하는 `legacy/frontend` package에서 config 소유 폴더를 provider 실행 root로 잘못
기록하던 결함을 수정한 뒤의 결과다. root/config mismatch 검사를 끄지 않고 실제 7개 AnalysisPlan unit
전체가 reconciliation을 통과했다.

| 측정 항목 | 실제 결과 |
| --- | ---: |
| included source | 1,065 files |
| scheduled language-file | 1,037 / 1,037 |
| AnalysisPlan / canonical unit receipt | 7 / 7 |
| compatibility documents / relations | 1,035 / 9,485 |
| provider 실행 | 703,743ms |
| execution-context reconciliation | 7ms, root mismatch 0 |
| direct Language IR stream | 34,687ms |
| canonical normalizer/linker | 10,026ms |
| canonical node / edge / evidence | 5,276 / 12,011 / 13,112 |
| canonical file coverage / gap / issue | 1,037 / 1,028 / 2 |
| 최종 artifact | language 19,577,485B, architecture 5,081,492B, SQLite 44,744,704B |

provider 703초는 이번 정확성 오류와 별개인 cold full-analysis 성능 수치다. 성공을 뜻하는 근거는 시간이
아니라 source manifest·plan·execution-context·canonical manifest가 모두 같은 digest chain으로 닫히고
immutable SQLite와 두 compatibility output이 실제 생성된 것이다.

Framework donor의 정적 HTTP registration은 별도 typed Framework IR을 거쳐 `HttpRoute` node와
`Exposes`/`Handles` edge로 canonical bundle에 들어간다. 정적 method/path와 source evidence가 없으면 route를
만들지 않고, 정확한 provider handler identity가 없으면 route만 남기고 `Handles`는 만들지 않는다. 9개 HTTP
언어 사례와 C의 GTK/GLib event donor 사례를 합친 flow gate는 10/10을 통과했지만, C event는 아직 canonical
event adapter가 아니라 donor 흐름 검증이다.

아직 최종 정적 제품은 아니다. middleware/RPC/GraphQL/frontend route·action·API client, ORM/static SQL,
event/queue/cache/external/config typed adapter, 독립 DB catalog와의 Code+DB bundle·reconciliation,
canonical clean=incremental, 수백~수천 파일 frozen/OSS holdout이 남았다.

- Batch 기준: 완료 2/8, 부분 완료 5/8, 미구현 1/8.
- 주관적인 단일 완료 퍼센트는 더 이상 갱신하지 않는다. 아래 batch별 산출물과 release gate를 판단 근거로 쓴다.
- 이 수치는 정확도 점수가 아니다. 남은 제품 경계와 release gate를 기준으로 한 구현 완성도다.
- 검토한 닫힌 corpus 안에서는 정의 117개와 실행 관계 35개가 각각 TP 100%, FP/FN 0을 통과한다.
  이를 임의의 실제 저장소 전체 정확도 100%로 표현하지 않는다.

## 최종 파이프라인 기준 판정

| Batch | 판정 | 현재 증거 | 완료하려면 필요한 것 |
| --- | --- | --- | --- |
| A. 공용 데이터 계약 | 완료 | shared `fact-model`에 source manifest, analysis plan, Language IR, canonical fact, evidence, coverage, stable ID 계약과 fail-closed validation이 있다. | schema 변경 시 migration/golden 유지 |
| B. Source Census·Analysis Plan·scheduler | 완료 | `index`의 첫 file authority이며 10언어 파일을 정확히 한 unit에 배정한다. provider shard는 plan 소유권을 바꾸지 못하고 누락은 typed omission으로 남는다. 정상 대형 파일도 전체 hash/line을 읽는다. | 새 언어·새 build system 추가 시 계약 확장 |
| C. 10언어 provider·Language IR | 부분 완료 | 10언어 provider 실행, 단일 atomic JSONL stream authority, source evidence, capability/gap receipt가 작동한다. 정의 117/117, owner 55/55, callable signature 63/63, known visibility 117/117, 호출·생성 35/35, import/export 39/39, type relation 90/90을 통과한다. configured execution context는 10/10 exact, config-removed variant는 false exact 0이다. | frozen unseen syntax와 실제 수백~수천 파일 workspace/TU/target holdout을 닫아야 한다. annotation은 typed adapter가 요구할 때만 transient evidence로 쓴다. |
| D. canonical normalizer/linker | 부분 완료 | 실제 `index`가 Language IR authority 직후 exact 2-pass identity/linker, dedup, deterministic relevance, invariant 검사, immutable SQLite bundle과 completion manifest를 생성한다. 10언어 소형·언어별 1.1MB 입력에서 2회 semantic/bundle digest가 같고 release gate가 bundle bytes까지 검증한다. | framework/DB typed record가 들어온 뒤 같은 linker/manifest 계약으로 통합하고, frozen multi-unit/수백~수천 파일 scale·crash-recovery·retention gate를 닫는다. |
| E. framework·API·ORM·asset 통합 | 부분 완료 | backend HTTP route/handler baseline과 test→production baseline이 각각 typed IR을 거쳐 canonical graph로 통합됐다. test gate는 10언어 exact relation 10/10과 name-only negative 10/10을 2회 검증한다. | middleware/RPC/GraphQL/frontend, ORM/static SQL, event/queue/cache/external/config typed adapter와 DB exact reconciliation |
| F. Tauri staging import·publish | 부분 완료 | code engine의 completion manifest·SQLite digest/schema/count를 검증하고 workspace generation에 원자 publish한다. analysis job lock, progress event, 이전 pointer 복원, static query/semantic map read model이 실제 desktop 경로에 연결됐다. | DB canonical bundle과 Code+DB reconciliation, large bundle streaming/memory gate, crash·cancel·rollback end-to-end 확대 |
| G. parity·hard cut | 부분 완료 | 구 UI/Atlas와 구 planner 일부는 제거됐고 Language IR donor 재변환도 삭제됐다. `language-index.v2`, `architecture-index.v4`, framework/collect 호환 output은 아직 active하다. | canonical parity 뒤 호환 output과 dead code 제거 |
| H. canonical incremental | 미구현 | provider cache invalidation은 있으나 clean canonical graph와 incremental graph의 동일 digest gate가 없다. 이번 감사에서 coordinator diagnostic이 direct/donor 한쪽에만 들어가 기본 실행을 막던 문제는 수정했지만, 이는 canonical incremental 완성이 아니다. | reverse dependency invalidation, fact reuse, clean=incremental 증명 |

## 최종 표시 데이터별 현재 상태

| 최종 사용자 정보 | 판정 | 현재 상태 |
| --- | --- | --- |
| 파일·언어·분석 범위·제외 이유 | 완료 | Source Census와 manifest hash가 실제 실행 authority다. |
| package/build/config 기반 분석 단위 | 완료 | Analysis Plan과 provider schedule이 exact accounting한다. |
| class/function/method/constructor/field와 소유자 | core·최소 metadata·canonical relevance 구현 | 10언어 117개 정의, owner 55개, callable declaration signature 63개, known visibility 117개를 사람 정답표와 대조했다. canonical linker는 top-level/type/callable, relation endpoint와 ancestor를 유지하고 사용되지 않는 nested member를 결정적으로 제거한다. provider identity 수, 병합 전 canonical definition 수, 유지·제거 수를 서로 다른 분모로 기록한다. |
| project-local direct call/construct | core 완료, 제품 범위 부분 | 25 CALLS + 10 CONSTRUCTS가 strict gate를 통과한다. dynamic dispatch는 gap이며 negative/missing-context/real-repository 확대가 남았다. |
| explicit export/re-export | **조건부 독립 baseline 인증** | export 문법이 제품 경계인 TypeScript·Dart의 2개 site를 전체 import/export 39-site 정답표 안에서 exact target으로 검증했다. export 개념이 없는 언어에 가짜 관계를 강제하지 않는다. |
| 일반 symbol reference | **제품 관계에서 제외** | raw provider occurrence는 typed API/test/DB/config 관계를 해석하는 transient 입력으로만 사용한다. 호출·타입·경계 의미로 승격되지 않은 `references` edge와 capability receipt는 Language IR/canonical 계약에서 제거했다. |
| internal import/package resolution | **독립 baseline·edge-case 인증** | 단일 authority가 `ProjectImportIndex`와 10언어 syntax site를 exact resolver에 통과시킨다. `imports.v1`은 45개 source/config와 39개 site를 SHA-256으로 고정하고 exact target·UTF-8/UTF-16 evidence·typed gap·negative를 대조한다. 결과는 internal 15, known external 7, unresolved 14, ambiguous 3이며 원본 mutation 0과 동일 manifest·plan·IR·artifact content digest를 통과했다. 다중 후보가 언어상 성립하지 않는 7개 언어에는 가짜 ambiguity 대신 missing-context/unresolved를 유지한다. |
| extends/implements/override/type relation | **10언어 독립 baseline 인증** | 17개 source/config를 SHA-256으로 고정하고 관계 90/90과 negative 22건을 두 번 실행했다. 결과는 extends 11, implements 7, mixes-in 1, overrides 13, declaration-bound uses-type 58이며 FP/FN 0, exact evidence 100%, source mutation 0이다. Python override는 확인된 local inheritance owner 안에서만 보완하고 private name-mangled member는 제외한다. 실제 대규모 저장소·missing-context 확대는 남았다. |
| Backend route/API/handler/middleware | **정적 HTTP route·handler baseline canonical 통합** | 9개 HTTP 언어에서 typed method/path/evidence route, file→route `Exposes`, exact handler→route `Handles`가 canonical bundle에 들어간다. handler가 없거나 모호하면 route는 유지하되 `Handles`는 만들지 않는다. middleware·RPC·GraphQL과 실제 framework/version holdout은 남았다. |
| Frontend route/page/action/store/API client | 자산 일부, 최종 미통합 | 일부 framework/project-model 정보만 있고 최종 typed adapter와 cross-stack binding이 없다. |
| DB schema/table/column/constraint | 별도 엔진 있음, 최종 미통합 | metadata-only DB engine은 존재하지만 code fact와 합친 canonical bundle이 없다. |
| ORM·static SQL read/write | 자산 일부, 최종 미통합 | collector/DB 자산은 있으나 code symbol↔query↔table exact reconciliation과 strict gate가 없다. |
| queue/cache/external/config boundary | 자산 일부, 최종 미통합 | messaging/deployment collector는 donor이며 canonical capability는 아직 unsupported다. |
| test case→production relation | **10언어 독립 baseline 인증** | exact runner/annotation/registration으로 TestCase를 만들고, 그 body 안의 provider-resolved direct call이 기존 project-local production definition을 가리킬 때만 confirmed `Tests` edge를 만든다. 언어별 positive 1개와 이름만 비슷한 negative 1개를 고정해 10/10 연결·10/10 거부·2회 결정성·source mutation 0을 통과했다. 미연결 test는 edge 대신 typed gap으로 남긴다. |
| 정적 TracePath | **desktop query·제품 read model 연결 완료** | published canonical node/edge/receipt/gap에서 confirmed exact 실행 관계만 따르는 bounded ordered path를 계산한다. complete/partial/gap/cycle/depth-limit, stable identity, evidence, representative/selection query를 MapView까지 연결했다. |
| 경계 관계 수·순환·공유 resource·inbound 없음 | 기반 구현, 집계 미구현 | canonical exact edge는 보존되지만 상위 boundary로 접는 RelationBundle과 고유 상대 영역/종류/순환/공유 resource 집계는 아직 없다. |
| 앱 지도에서 사용할 snapshot | **code-only vertical slice + AI 분할 경로 연결** | code engine immutable canonical bundle을 Tauri가 검증·원자 publish하고, static region·relation·TracePath·AI Base revision을 거쳐 typed MapView/Selection으로 제공한다. 큰 Base 입력은 결정적 local partition→개별 verifier/cache→source-free global reconciliation→전체 verifier로 연결됐다. 실제 대형 provider 품질·시간 gate와 DB는 남았다. |

## 현재 검증으로 실제 증명된 범위

- 지원 언어: TypeScript, JavaScript, Python, Java, C#, C, C++, Go, Rust, Dart.
- 정의: TP 117, FP 0, FN 0, kind/owner/coverage/determinism 100%, owner 55/55.
- 실행 관계: CALLS 25 + CONSTRUCTS 10 = 35, FP 0, FN 0.
- import/package: reviewed site 39/39, internal 15, known external 7, unresolved 14,
  ambiguous 3, invalid evidence 0, 10/10 언어 결정성.
- type relation: reviewed relation 90/90, extends 11, implements 7, mixes-in 1,
  overrides 13, uses-type 58, reviewed negative 22, invalid evidence 0, source mutation 0,
  10/10 언어 2회 결정성.
- 크기: 언어마다 provider-visible source 하나를 1.1MB 이상으로 늘려도 동일 digest.
- 대형 파일 성능: 중복 donor 재변환 삭제 뒤에도 Python syntax inventory 반복 parsing이 남아 있었다.
  파일당 syntax tree를 한 번만 만들고 definition/import/type가 공유하도록 고친 동일 1.1MB 2회 gate에서
  direct IR 총시간은 semantic 46,198ms→8,280ms, definition 47,050ms→8,283ms, 최대 단일 실행은
  23,015ms→3,708ms로 감소했다. 결과 digest와 117/117·35/35 정확도는 유지됐다.
- 배포 provider: signed catalog에서 8개 pack을 임시 경로에 새로 추출한 뒤 10언어 uniform core,
  definition 117/117, CALLS/CONSTRUCTS 35/35, import 39/39, type relation 90/90을 한 출시 gate에서 통과.
- definition metadata: reviewed definition 117/117 known visibility, callable 63/63 source-backed declaration
  signature, 독립 metadata 사례 37/37, 10/10 언어, 2회 metadata digest 결정성을 통과한다. C++ 생성자
  initializer를 signature에 섞던 결함과 Rust public trait 구현 member를 private로 낮추던 결함도 별도
  회귀로 고정했다. compatibility raw provider signature 60/162는 이 수정 전 원인을 찾은 역사적 진단값이며
  제품 정의 coverage나 현재 정확도로 사용하지 않는다.
- provider execution-context: 정상 설정 9개 project가 10언어를 덮고 exact 10/10이다. 같은 fixture에서
  핵심 config를 제거한 9개 variant는 false exact 0이며 partial/not-executed와 누락 dimension을 정직하게
  남긴다. 실제 monorepo의 여러 target/TU와 build variant 확대는 여전히 필요하다.
- framework flow: JavaScript/TypeScript Express, Go Gin, Rust Axum, C++ Crow, C# ASP.NET Core, Dart Shelf,
  Python FastAPI, Java Spring MVC의 HTTP route→handler→service와 C GTK/GLib event→callback→service를
  실제 provider로 실행해 10/10 통과했다. HTTP 9건은 canonical route/node/edge까지 확인한다.
- 기계 회귀: Code Memory Rust **286/286**, shared fact-model **17/17**, semantic-model **3/3**,
  semantic-compiler **21/21 + 1 provider opt-in**, Tauri **66 passed / 4 environment-only ignored**,
  frontend **6/6**, fmt, typecheck, lint, production frontend build, internal Tauri release-profile build,
  `clippy --all-targets -D warnings`, locked code-engine release build가 모두 통과한다.
  import staging 미사용 경고 63건은 실제 IR 연결로 해소했으며 suppression을 추가하지 않았다.

## 이번 재감사에서 찾고 바로 고친 결함

1. **coordinator diagnostic direct/donor 분리**
   - compiler project model이 실패한 실행에서는 donor만 경고를 받아 record가 70/71로 갈라졌고,
     정상적인 CLI 분석이 parity 오류로 중단됐다.
   - provider merge 전에 생긴 diagnostic을 `DirectLanguageIrInput`에도 전달하고 회귀 테스트를 추가했다.
     같은 실패 조건에서 direct/donor record 71/71과 exact parity가 확인됐다.
2. **imports 거짓 완료**
   - provider import relation이 0인데 SCIP/CompilerApi imports receipt는 `Complete`가 될 수 있었다.
   - 실제 import-site 분모와 resolver가 연결되기 전에는 모든 protocol에서 imports를 필수·부분 완료로
     고정하는 회귀 테스트를 추가했다. 이 수정은 import를 구현한 것이 아니라 거짓 성공을 제거한 것이다.
3. **호출 관계 정답 gate의 release 경로 누락**
   - `run-semantic-ground-truth-gate.ps1`은 35개 CALLS/CONSTRUCTS를 통과했지만 signed provider bundle
     gate는 uniform-core, definition, import만 실행해 호출 관계 회귀가 패키징을 차단하지 못했다.
   - release bundle gate가 definition 117, CALLS/CONSTRUCTS 35, import 39의 세 독립 정답표를 모두
     실행하도록 연결했다. 개별 gate 통과와 출시 차단 경로 연결을 같은 완료 조건으로 유지한다.
4. **Windows canonical path 표현 분리로 Java·C# provider가 조용히 빠짐**
   - 선택한 저장소 root는 `D:\\...` 형태였지만 provider 격리 workspace가 다시 canonicalize한 root는
     `\\?\\D:\\...` 형태여서, 같은 디렉터리가 `strip_prefix`에서 서로 다른 경로로 판정됐다.
   - 이 때문에 Java·C# semantic provider는 `outside the selected repository`로 실패했지만 import
     syntax resolver가 일부 결과를 대신 만들어 import gate만 보면 결함이 가려질 수 있었다.
   - repository ownership에 쓰는 모든 기존 경로를 하나의 `canonical_existing_path` 경계로 통일하고,
     선택 root·격리 root·복사할 원본 파일에 같은 표현을 적용했다. 최신 release binary에서 definition
   117/117, CALLS/CONSTRUCTS 35/35, import 39/39가 다시 통과했다.
5. **성능 타이머가 provider와 IR 후처리를 같은 값으로 기록**
   - 두 타이머가 같은 시작점을 공유해 대형 실행의 597초를 provider 비용처럼 보이게 했다.
   - provider, source stability, execution-context, direct IR, merge, donor IR, parity를 별도 경계로
     계측했다. 새 결과는 provider 106,027ms와 IR 507,408ms를 분리해 중복 변환 병목을 확정했다.
6. **10언어 hard cut 뒤 provider 로컬 메타데이터에 Ruby/PHP가 잔존**
   - 실제 manifest와 배포 pack은 10언어뿐이었지만 로컬 README가 12언어를 말하고 checksum 목록에
     이미 제거된 Ruby/PHP 파일 37,460개가 남아 있었다.
   - README를 10언어 계약으로 고치고 checksum을 현재 31,173개 파일에서 재생성해 Ruby/PHP 항목을
     0으로 만들었다. release pack에는 원래 이 두 디렉터리가 포함되지 않았음을 별도로 확인했다.
7. **provider `IMPLEMENTATION` 하나를 모든 언어에서 implements로 오해**
   - 실제로는 Java/C#/C++의 class inheritance, interface implementation, method override가 같은 raw flag에
     섞여 있었다. 독립 syntax inventory의 `extends`/`implements`/Dart `with`/Rust `impl` 위치와
     provider가 해결한 양 끝점을 동시에 확인해 canonical kind를 분리했다.
   - C++ `DEFINITION`은 header 선언→구현 방향으로 오므로 cross-owner method일 때만 방향을 뒤집어
     `overrides`로 만들고, 일반 함수 prototype은 type relation으로 승격하지 않는다.
8. **Rust trait 구현의 method relation만 있고 type relation이 빠짐**
   - rust-analyzer는 `id` method override는 주지만 `User implements Entity` type pair를 hierarchy API로
     주지 않았다. `impl Entity for User`의 두 토큰에서 LSP definition을 조회하고, 둘 다 project-local
     type symbol로 유일하게 해결될 때만 relationship과 exact occurrence를 보존했다.
   - 현재 fresh cache에서 Rust는 implements 1 + overrides 1, explicit site match 1/1이다.
9. **C/C++ shared header를 path만으로 중복 판정한 gate**
   - 같은 `types.h`가 C와 C++ compile context에서 별도 semantic document가 되는 정상 동작을 기존
     semantic gate가 중복으로 오판했다. 문서 identity를 `(language, path)`로 고쳐 10/10 gate를 복구했다.
10. **C/C++ type occurrence를 모두 `uses_type`으로 승격한 과잉 수집**
   - 지역 변수, 생성식, 상속 토큰, receiver/self까지 화면 관계가 될 수 있었다.
   - field·parameter·return·generic constraint처럼 선언에 묶인 type site만 독립 inventory로 만들고,
     provider가 exact local type symbol을 해결한 경우만 관계로 보존했다.
11. **LSP definition name range와 declaration 범위를 같은 값으로 사용**
   - type evidence의 owner가 method가 아니라 바깥 class로 붙고, Dart abstract method가 누락됐다.
   - definition occurrence는 exact selection/name span, `enclosing_range`는 전체 declaration로 분리했다.
12. **Python language server의 method implementation 응답 공백**
   - `Service extends BaseService`는 확인됐지만 `Service.execute overrides BaseService.execute`가 빠졌다.
   - provider-resolved local inheritance pair와 exact owner/member definitions가 모두 있을 때만 같은 member를
     연결하고, Python name-mangled private member는 제외했다. 전역 이름 fallback은 만들지 않았다.
13. **C++ 템플릿 생성자 중복 label**
   - clangd가 같은 위치의 생성자를 `BoxValue`와 `BoxValue<T>` 두 이름으로 보내 definition FP 1이 생겼다.
   - LSP symbol 중복 identity를 표시 이름이 아니라 exact selection 위치+symbol kind로 바꾸고 lexical한
     짧은 이름과 넓은 declaration span을 보존해 definition 117/117을 복구했다.
14. **개별 gate는 통과하지만 release bundle에서만 PowerShell StrictMode가 실패**
   - semantic 정답의 선택 속성 `tokenOccurrence`가 없는 정상 항목을 dot access로 읽어, 상위 release
     script의 `Set-StrictMode -Version Latest`를 상속한 경우에만 실행이 중단됐다.
   - `PSObject.Properties`로 속성 존재를 먼저 확인하고 없으면 첫 occurrence를 쓰도록 고쳤다. 같은
     StrictMode 아래 semantic 35/35, import 39/39, type relation 90/90과 전체 provider bundle을 다시 통과했다.
15. **같은 provider batch를 direct와 donor 두 경로에서 다시 IR로 변환**
   - 1.1MB 언어 fixture에서 provider가 아니라 IR 변환만 507,408ms를 사용했다. 두 결과가 같다는 parity는
     정확도를 높인 것이 아니라 같은 입력과 adapter를 두 번 실행한 자기 비교였다.
   - `ProviderUnitBatch`를 유일 authority로 정하고 한 번의 merge에서 atomic JSONL IR과 임시
     `language-index.v2` 호환 projection을 함께 만든다. donor→IR 재변환과 parity receipt는 삭제했다.
   - unit test는 실제 JSONL 모든 record를 역직렬화하고 byte SHA-256을 대조한다. PowerShell 공용 helper는
     schema/snapshot/stream digest/record·byte count/complete/content digest를 검사하며 definition,
     semantic, import, type와 signed provider release gate가 2회 artifact digest 결정성을 차단한다.
16. **한 source를 capability마다 다시 parsing해 대형 Python IR이 22초 이상 걸림**
   - definition, import, type hierarchy, type use가 각각 tree-sitter tree를 만들었고 type use 내부에서도
     definition과 hierarchy를 다시 parsing했다. LSP/SCIP provider 경로도 type relation/use를 중복 parsing했다.
   - 파일당 syntax tree를 한 번 만들고 root-based inventory들이 공유하도록 바꿨다. provider는
     `SyntaxTypeInventory` 하나에서 relation/use를 함께 받는다. 정답 수나 digest를 바꾸는 최적화가 아니다.
   - 동일한 1.1MB·10언어·2회 gate에서 direct IR 총시간은 semantic 46,198→8,280ms,
     definition 47,050→8,283ms, 최악 파일 23,015→3,708ms로 줄었고 117/117·35/35·결정성은 유지됐다.

## 다음 작업 순서

1. 실제 provider execution-context의 exact/partial matrix와 독립 정답 gate 마감
2. frozen holdout으로 Batch C의 새 문법 과적합 여부와 실제 저장소 coverage 확인
3. canonical definition identity table, 2-pass normalizer/linker와 deterministic relevance gate
4. framework/API typed adapter
5. DB/ORM/static SQL 및 event/queue/cache/external typed adapter
6. canonical Fact Import Bundle과 Tauri staging/publish
7. canonical parity 후 legacy output·adapter 제거
8. clean=incremental 동일 digest와 실제 OSS·수백~수천 파일 scale/cancel/OOM gate

1~8은 모두 결정적 정적 분석·시스템 구현 작업이다. AI는 이 단계를 대신하지 않는다. AI는 canonical
Fact Graph가 완성된 뒤 의미 영역 이름, 요약, 설명, candidate 관계를 만드는 별도 계층이다.

## 2026-08-08 전체 진행도 재감사 — definition metadata와 실행 context

이번 재감사는 이미 통과한 definition 117/117을 다시 세지 않고, 그 숫자가 가리고 있던 두 빈칸을
실제 release binary 산출물과 코드로 확인했다.

| 항목 | 실측 | 판정 |
| --- | ---: | --- |
| reviewed definition name/kind/owner | 117/117, owner 55/55 | core 완료 |
| Language IR known visibility | **0/117** | 미구현. adapter가 모든 정의에 `Unknown`을 넣음 |
| raw provider symbol signature | **60/162 (37.0%)** | 불균일. 제품 정의 정답 분모를 새로 만들어야 함 |
| provider unit 실행 | **10/10** | 실행 자체는 통과 |
| exact execution context | **0/10** | 미완료 |
| partial execution context | **10/10** | 누락 dimension은 정직하게 기록됨 |

`relevance`는 이 단계에서 언어별로 추출할 새 의미 값이 아니다. provider가 “중요해 보인다”는 점수를
만들면 언어별 편향과 노이즈가 생긴다. source census의 test/generated/vendor/external flags, canonical
소유권, public surface, 실제 typed relation을 입력으로 Batch D가 결정적으로 계산한다. 따라서 Batch C의
남은 범위에서 relevance를 분리했고, 임의 annotation·documentation·local 변수 수집도 추가하지 않는다.

전체 공정률은 **약 45% (43~47%)**로 유지한다. 새 측정은 완료 기능을 늘린 것이 아니라 부분 완료의
정확한 경계를 드러낸 것이다.

## 삭제 원칙

현재 legacy provider/framework/collector는 보기 싫다는 이유만으로 먼저 삭제하지 않는다. Batch D/E의
canonical 결과가 같은 사실을 보존한다는 parity를 통과한 단위부터 제거한다. 반대로 parity가 끝난 donor,
중복 DTO, 구 output writer, dead reference는 같은 패치에서 제거한다.

## 2026-08-08 최신 체크포인트 — type relation 독립 인증 완료

현재 release binary와 실제 managed provider로 다음 네 개의 서로 다른 사람 정답표를 모두 다시
통과했다. 한 fixture의 성공을 다른 capability 정확도로 재사용하지 않는다.

| capability | 독립 분모 | 현재 결과 |
| --- | ---: | --- |
| definition/owner | 정의 117, owner 55 | TP 117, FP 0, FN 0, kind/owner/evidence/결정성 100% |
| direct call/construct | 실행 관계 35 | CALLS 25 + CONSTRUCTS 10, FP/FN 0 |
| import/package | import site 39 | internal 15, external 7, unresolved 14, ambiguous 3, invalid evidence 0 |
| type relation | 관계 90 + negative 22 | extends 11, implements 7, mixes-in 1, overrides 13, uses-type 58, FP/FN 0 |

type gate는 17개 source/config SHA-256, exact endpoint/range, 단일 stream authority의 content digest,
두 번의 동일 digest,
원본 변경 0건을 함께 검사한다. 연결 여부만 확인한 것이 아니라, signed catalog에서 8개 pack을 임시
경로에 추출한 실제 출시 gate도 10언어 uniform core와 definition 117/117, CALLS/CONSTRUCTS 35/35,
import 39/39, type relation 90/90을 연속 통과했다. 전체 회귀는 Code Memory 261/261, fact-model 15/15,
Tauri 38/38, fmt/check/clippy `-D warnings`/locked release build가 통과했다.

이로써 Batch C의 시각화 핵심 원재료 중 definitions, calls/constructs, imports/explicit exports, type
relations는 독립 baseline을 가졌다. generic references는 의도적으로 제품 관계에서 제거했다. 단일
atomic JSONL Language IR authority와 content-digest 결정성도 연결됐고 donor 재변환은 제거됐다. 그러나
signature/visibility·relevance, exact execution context, 실제 대규모 holdout이 남았고 뒤쪽
canonical linker·Code+DB adapter·Tauri importer/publish가 미구현이다. 따라서 최종 정적 제품 구현
완성도는 **약 45% (43~47%)**로 판정하며, 앞단 fixture 정확도 100%를 전체 제품 100%라고 부르지 않는다.

> 아래 체크포인트 절은 실패 원인과 수정 순서를 보존하는 시간순 기록이다. 현재 판정은 문서 맨 위 표와
> 가장 마지막 체크포인트를 우선한다. 과거 절의 36-site/ambiguity 0 수치를 현재 수치로 읽지 않는다.

## 2026-08-08 전체 재검증 체크포인트

이 절은 위 판정을 현재 checkout의 소스, release binary, 수동 정답표, 대형 파일 사본과 다시 대조한
결과다. import 런타임 연결 때문에 Batch C 내부 진행은 늘었지만 최종 제품 경계는 아직 그대로다.
**전체 구현 완성도는 약 40% (38~42%)이며 정확도 점수가 아니다.**

### 다시 통과한 실행 증거

| 검증 | 현재 결과 | 이 결과가 증명하지 않는 것 |
| --- | --- | --- |
| Code Memory Rust | 251/251 통과 | 아직 제품 경로에 연결하지 않은 import/linker/publish 기능 |
| shared fact-model | 15/15 통과 | 타입 정의만 있는 canonical graph의 실제 생성 |
| Tauri | 38/38 통과 | Code/DB canonical bundle ingestion과 analysis command |
| definition ground truth | TP 117, FP 0, FN 0, owner 55/55, 10/10 결정성 | 임의 저장소의 모든 문법과 symbol 종류 |
| CALLS/CONSTRUCTS ground truth | TP 35, FP 0, FN 0, evidence·owner·10/10 결정성 | import/type/framework/DB 관계의 정확도 |
| 1.1MB source gate | 위 두 정답표와 digest 모두 유지 | 수십~수백 MB 파일, 전체 workspace peak memory |
| uniform core provider gate | 10/10 언어 통과 | capability별 정답 분모가 모두 존재한다는 뜻 |
| Database Memory 로컬 테스트 | CLI 5 + core 149 = 154 통과, live DB 27개 환경 제외 | code symbol과 DB object가 최종 Fact Graph에서 연결됨 |
| 코드 품질 | fmt, check, locked release build, Code Memory clippy `-D warnings` 모두 통과 | 독립 import/type/framework 정확도 정답표를 대신하지 않음 |

생성된 최신 정량 보고서는 다음과 같다.

- `code_memory/build/definition-ground-truth/definition-quality-report.json`
- `code_memory/build/semantic-ground-truth/semantic-quality-report.json`
- `code_memory/build/large-source-semantic/large-source-semantic-report.json`
- `code_memory/build/import-ground-truth/import-quality-report.json`
- `code_memory/build/type-relation-ground-truth/type-relation-quality-report.json`

### 현재 결과를 쉬운 층으로 나눈 판정

1. **무엇을 읽을지 정하기 — 완료**: Source Census, 제외 이유, 전체 hash/line, 10언어 file ownership,
   package/config 기반 Analysis Plan과 provider schedule이 실제 실행 authority다.
2. **파일 안의 핵심 재료 뽑기 — core 완료**: 시각화에 필요한 explicit definition/owner와 검토 corpus의
   direct CALLS/CONSTRUCTS는 exact source evidence를 가진다.
3. **파일·package 사이를 정확히 잇기 — baseline 인증, edge case 인증 중**: 10언어 import syntax inventory와
   exact resolver가 새 IR에 연결됐다. 전용 실제 provider fixture에서 36개 site 전체를 사람이 고정한
   정답과 대조했고, internal 15·known external 7·unresolved 14를 cold/warm 동일 digest로 통과했다.
   다만 ambiguity 사례가 정답표에 아직 없고 import 전용 1.1MB clean/warm은 provider가 root에 만든
   제외 산출물로 manifest가 달라져 실패했으며 type/override도 독립 정답표 없이 partial이다.
4. **API·framework·ORM·queue·external·DB를 같은 어휘로 바꾸기 — 미완료**: 추출 donor와 독립 DB
   엔진은 있지만 canonical typed adapter와 code↔DB exact join이 없다.
5. **하나의 무손실 Fact Graph로 조립하기 — 미완료**: 공용 FactNode/FactEdge 타입만 있고 identity table,
   endpoint linker, dedup, relevance gate, bundle writer가 없다.
6. **앱에 안전하게 게시하기 — 미완료**: Tauri의 현재 SQLite store는 공용 fact-model과 다른 임시 DTO를
   받는 테스트 경계다. 실제 analysis job, staging validation, atomic generation publish가 없다.
7. **변경분만 재분석해도 동일함을 증명하기 — 미완료**: provider cache는 있으나
   `clean canonical digest == incremental canonical digest`를 아직 증명할 canonical graph 자체가 없다.

### 이번 감사의 제품 판정

현재 엔진을 “정적 분석이 거의 끝났다”고 부르면 안 된다. 더 정확한 표현은 다음이다.

> **10언어 source scope와 핵심 code symbol/direct-call 재료 수집기는 검증됐다. 그러나 최종 시각화가
> 필요로 하는 관계 완성, canonical graph 조립, Code+DB 결합, 앱 게시 파이프라인은 아직 남아 있다.**

따라서 다음 작업은 문서에 이미 적힌 순서대로 **internal import/package resolution 독립 인증**이다. 이 단계부터는
각 capability마다 positive, negative, ambiguous, missing-context, cold/warm, large-source 분모를 따로 두며,
기존 CALLS/CONSTRUCTS corpus를 반복 확장해 수치만 올리는 방식은 금지한다.

## 2026-08-08 import 제품 연결 후 전체 체크포인트

이번 체크는 파일이 생겼는지, 제품이 호출하는지, 사람이 고정한 독립 정답표가 있는지를 분리했다.
전체 Batch 판정은 **완료 2/8, 부분 완료 2/8, 미구현 4/8**, 의사결정용 구현 완성도는
**약 40% (38~42%)**다. import가 제품 Language IR과 v3 capability receipt까지 연결돼 Batch C 내부
진행은 늘었지만 canonical graph와 publish 경계가 그대로라 전체 제품을 크게 올려 잡지 않는다.

### 이번에 실제로 확인한 import 하위 단계

| 하위 단계 | 판정 | 확인 결과 |
| --- | --- | --- |
| 공용 grammar·UTF-8/UTF-16 range | 완료 | 10언어 grammar 선택과 비 ASCII column 변환 테스트 통과 |
| module/package IR endpoint | 완료 | 임의 파일을 구조 endpoint로 숨길 수 없도록 kind와 이름을 fail-closed 검증; shared contract 15/15 |
| import-site inventory | 단위 완료 | TS/JS, Python, Java, C#, C/C++, Go, Rust, Dart의 명시 구문을 세고 comment·비 literal·`mod` 오인을 막는 테스트 6개 통과 |
| project import index·resolver | baseline 인증, edge case 인증 중 | exact metadata/path 규칙이 direct/donor 공통 index로 실행된다. 전용 10언어 실제 provider fixture의 internal/external/unresolved 결과는 통과했지만 ambiguous branch의 독립 정답 분모는 아직 0이다. |
| direct/donor 입력과 `emit_unit` 연결 | 완료 | `file_relations`·`project_model_files`를 두 입력에 전달하고 `ProjectImportIndex::build`를 snapshot당 한 번 실행한다. 각 file의 `inventory_imports`·`resolve` 결과와 exact evidence를 IR에 넣고 direct/donor exact parity를 검사한다. |
| capability 분모·typed gap | 구현 | v3 receipt가 eligible/import/export/internal/external/unresolved/ambiguous/invalid evidence와 inventory/metadata failure를 언어별로 기록한다. 독립 정답표와 대조 전이므로 정확도 인증은 아님 |
| 독립 ground truth·대형 파일·결정성 | 부분 완료 | `imports.v1`과 전용 release gate가 33개 pinned file, 36개 site의 baseline 10/10 cold/warm 결과를 통과했다. 1.1MB 두 실행의 site/target/evidence 대조도 통과했지만 clean→warm `SourceManifestDigest`가 달라 최종 gate는 실패했다. ambiguity branch도 아직 0이다. |

### 최신 실행 결과

- Code Memory Rust: 251/251
- shared fact-model: 15/15
- Tauri: 38/38
- Database Memory: CLI 5 + core 149 = 154 통과, live DB 27개 환경 제외
- 정의: TP 117 / FP 0 / FN 0, owner 55/55, 10언어 2회 결정성
- CALLS·CONSTRUCTS: TP 35 / FP 0 / FN 0, 10언어 2회 결정성
- 언어별 1.1MB source: 위 정의와 관계 결과 유지
- fmt, `cargo check --locked`, locked release build: 통과
- Code Memory clippy `--all-targets -D warnings`: 통과. 이전 미사용 import 코드 63건은 실제 wiring으로 해소
- import 전용 10언어 ground truth: reviewed file 33, site 36, internal 15, known external 7,
  unresolved 14, invalid evidence 0, 10/10 cold/warm 결정성. ambiguity는 0건이므로 아직 branch 인증이 아니다.
- import 전용 언어별 1.1MB: 두 실행 모두 36개 site의 exact outcome/target/evidence 대조를 통과한 뒤
  `SourceManifestDigest` 불일치로 최종 실패. 분석 첫 실행이 source root에 provider 산출물을 생성하는
  clean-start 격리 결함을 발견했다.

### 다음 한 단계의 종료 조건

완료된 baseline 정답표를 유지하면서 먼저 C#/Java 등 provider가 source root에 build/IDE 산출물을 쓰지
않도록 provider workspace를 격리하거나, 동일한 수준으로 원본 불변성을 증명한다. 그 뒤 resolver가 실제로
여러 후보를 받을 수 있는 각 언어 family의 ambiguous branch와 언어별 1.1MB 변형을 별도 분모로 고정한다.
exact target·truth·evidence range·typed gap·digest가 clean/warm에서 같고 FP/FN 0이어야 이 단계를 완료로
올린다. 언어 문법상 후보 다중성이 발생하지 않는 branch에 가짜 ambiguity를 만들지 않고, 그 경우에는
missing-context fail-closed를 검증한다.

## 2026-08-08 현재 checkout 전체 재확인

현재 source와 최신 정량 보고서의 생성 시각을 대조하고 핵심 로컬 회귀를 다시 실행했다. Code Memory
source의 마지막 변경은 import 보고서보다 앞서 있으므로 현재 보고서는 stale하지 않다. 이번 확인으로
새 capability가 추가되거나 미구현 제품 경계가 닫히지는 않았으므로 전체 판정은 **완료 2/8, 부분 완료
2/8, 미구현 4/8, 약 40% (38~42%)**로 유지한다.

| 재검증 대상 | 결과 | 정직한 의미 |
| --- | --- | --- |
| Code Memory Rust | 251/251 통과 | Source Census·AnalysisPlan·provider·Language IR 앞단의 회귀 없음 |
| shared fact-model | 15/15 통과 | 계약과 fail-closed validation 정상; canonical producer가 생겼다는 뜻은 아님 |
| Tauri | 38/38 통과 | transitional store 안전 primitive 정상; canonical Code+DB importer는 여전히 없음 |
| Database Memory | CLI 5 + core 149 = 154 통과, live 27 환경 제외 | metadata-only DB 엔진 정상; code↔DB canonical join은 여전히 없음 |
| definition 정답표 | 117/117, FP/FN 0, owner 55/55 | 고정 corpus의 definition/owner capability 인증 |
| CALLS·CONSTRUCTS 정답표 | 35/35, FP/FN 0 | 고정 corpus의 project-local direct execution relation 인증 |
| import baseline | 36/36, internal 15·external 7·unresolved 14 | baseline exact resolver 인증; ambiguity와 clean-start는 미인증 |
| import 1.1MB clean/warm | site·target·evidence는 두 실행 통과, 최종 digest 실패 | provider가 원본 root를 변경하는 실행 격리 결함이 현재 Batch C의 첫 차단 요소 |

현재 상태를 제품 언어로 요약하면 다음과 같다.

> **분석할 원본과 10언어의 핵심 코드 재료는 신뢰할 수 있게 뽑는다. 그러나 그 재료를 API·DB·event와
> 합쳐 하나의 무손실 지도 snapshot으로 만드는 조립·게시 파이프라인은 아직 없다.**

따라서 다음 구현은 기능을 넓히는 작업이 아니라 **provider writable workspace 격리**다. C#/Java
provider가 사용자 source root 대신 manifest에서 만든 격리 사본에만 산출물을 쓰게 하고, 원본 mutation
0건과 clean/warm 동일 digest를 증명한 뒤 import ambiguity/missing-context와 type relation gate로 넘어간다.

## 2026-08-08 provider 격리 완료 후 전체 진행도 체크

현재 checkout의 실행 경로, 공용 계약, provider 실제 실행, DB 엔진, Tauri 저장 경계를 다시 대조했다.
이 절은 위 실패 기록을 지우지 않고 **현재 판정만 최신화**한다.

### 이번에 닫힌 차단 요소

- `SourceManifest`가 허용한 파일만 process cache 아래 writable workspace에 스트리밍 복사하며, 크기와
  SHA-256을 provider 실행 전에 다시 검사한다.
- 원본을 변경하던 Java/C# provider는 이 사본에서만 실행한다. evidence와 coverage는 원래 repository
  path로 되돌린다.
- 같은 언어의 여러 analysis unit은 결정적 순서로 같은 격리 사본을 사용해 source 복사 비용을 제한한다.
- provider 실행 뒤 Source Census를 다시 수행하고 최초 manifest digest와 다르면 결과를 게시하지 않는다.
- cache root의 repository 내부 fallback을 제거해 provider/cache 산출물이 선택한 source root 안으로
  들어갈 수 없게 했다.
- import gate는 저장소 fixture를 직접 실행하지 않고 SHA-256 pinned 33개 파일만 임시 root에 복사한다.
  따라서 과거 ignored 산출물이 남아 있어도 clean-start 분모를 오염시키지 않는다.

### 현재 재검증 숫자

| 검증 | 현재 결과 |
| --- | --- |
| Code Memory Rust | 254/254 통과 |
| shared fact-model | 15/15 통과 |
| Tauri | 38/38 통과 |
| Database Memory | core 149 + CLI 5 = 로컬 154 통과, live DB 27 환경 제외 |
| clean import baseline | 10/10 언어, 36/36 site, internal 15, known external 7, unresolved 14, cold 47,706ms / warm 436ms |
| 언어별 1.1MB import | 10개 큰 파일, 최소 1,100,048 bytes, 두 실행 545,387ms / 488,544ms, 36/36 site 통과 |
| 결정성 | 두 gate 모두 Source Manifest, Analysis Plan, IR stream set, semantic payload set digest 1종 |
| 코드 품질 | fmt check, locked check, clippy `-D warnings`, locked release build 통과 |

정량 보고서:

- `code_memory/build/import-ground-truth-clean/import-quality-report.json`
- `code_memory/build/large-source-import-isolated/import-quality-report.json`

### 전체 진행도 판정

Batch 수는 **완료 2/8, 부분 완료 2/8, 미구현 4/8**이며 최종 제품 구현 완성도는 여전히
**약 40% (38~42%)**다. provider 격리는 Batch C 안의 실제 release blocker를 닫았지만 canonical
normalizer/linker, framework/API/ORM/event/DB typed adapter, Code+DB bundle, Tauri staging/atomic publish,
clean=incremental을 새로 만들지는 않았기 때문이다.

쉬운 말로는 다음과 같다.

> **무엇을 읽고, 10언어 코드에서 핵심 정의·직접 호출·기본 import를 근거와 함께 뽑는 앞단은 강해졌다.
> 그러나 이 재료를 API·DB·queue까지 한 그래프로 조립해 앱 지도에 안전하게 올리는 뒤 절반은 아직 없다.**

다음 임계 순서는 import ambiguity/missing-context 독립 분모, type/extends/implements/override 독립 gate,
canonical 2-pass linker다. 격리 workspace의 Java/.NET 보조 cache 수명 정리는 정확도와 분리한 운영
후속 항목으로 유지한다.

## 2026-08-08 import edge-case 완료 후 전체 진행도 감사

이 체크포인트가 현재 최신 판정이다. 실행 경로를 다시 따라가 `파일 생성 여부`, `실제 소비 여부`,
`독립 정답표`, `release 차단 경로`를 각각 분리해 확인했다.

### 현재 수치

| 검증 | 현재 checkout 결과 | 정직한 의미 |
| --- | --- | --- |
| Code Memory Rust | 254/254 | 이미 구현된 census/planner/provider/IR 경계 회귀 없음 |
| shared fact-model | 15/15 | 계약·stable ID·fail-closed validation 정상 |
| Tauri | 38/38 | transitional store primitive 정상; canonical importer 증명 아님 |
| Database Memory | CLI 5 + core 149, live 27 환경 제외 | metadata-only 독립 DB 엔진 정상; Code+DB join 증명 아님 |
| 정의 | 117/117, owner 55/55, FP/FN 0 | 고정 10언어 corpus의 definition/owner 인증 |
| 직접 실행 관계 | CALLS 25 + CONSTRUCTS 10 = 35/35, FP/FN 0 | 고정 corpus의 exact target/range 인증 |
| import baseline | 45 pinned file, 39/39 site, internal 15, external 7, unresolved 14, ambiguous 3 | 다중 후보가 있는 Python·Java·C#은 간선을 만들지 않고 fail-closed |
| import 1.1MB | 10개 큰 파일, 최소 1,100,048 bytes, 2회 598,893ms / 494,665ms; 분리 계측 1회 615,563ms | 동일 39-site 결과와 manifest/plan/IR digest; 정확도·결정성은 통과했지만 성능은 미통과 |
| IR 성능 분리 | baseline IR 87ms, 1.1MB IR 507,408ms; 큰 입력 provider 106,027ms | direct 249,037ms + donor 258,363ms의 중복 전체 변환이 병목 |
| 코드 품질 | fmt, locked check, clippy `-D warnings` 통과 | Rust source 품질 회귀 없음 |

최신 import 보고서:

- `code_memory/build/import-edge-cases-current-fixed/import-quality-report.json`
- `code_memory/build/large-source-import-edge-cases-current-fixed/import-quality-report.json`
- `code_memory/build/source-timing-split/import-quality-report.json`
- `code_memory/build/large-source-timing-split/import-quality-report.json`

### 이번 감사에서 새로 닫은 것

1. Python의 독립 project roots, Java의 독립 modules, C#의 독립 projects에 동일한 정적 후보를 두어
   실제 candidate multiplicity 3건을 만들었다. C# partial type처럼 하나의 논리 타입인 사례를 모호성으로
   세지 않았다.
2. 언어 문법·project model상 같은 방식의 후보 다중성이 성립하지 않는 나머지 7개 언어에는 가짜
   ambiguity를 만들지 않고 missing-context/unresolved 분모를 유지했다.
3. baseline과 대형 gate 모두 사람이 SHA-256으로 고정한 45개 파일만 새 임시 root에 복사한다.
4. signed provider bundle gate가 definition뿐 아니라 CALLS/CONSTRUCTS와 import 독립 정답 gate도
   실행하도록 연결했다. 수동 통과와 출시 차단을 같은 완료 조건으로 만들었다.
5. Windows에서 선택 root는 `D:\\...`, provider workspace는 `\\?\\D:\\...`로 다시 canonicalize되어
   Java·C# provider가 repository 밖으로 오인되던 결함을 공용 path canonicalization 경계로 고쳤다.
   수정 후 definition 117/117, CALLS/CONSTRUCTS 35/35, import 39/39와 대형 run의 10/10 `indexed`를
   새 release binary로 다시 확인했다.

### 코드 경로로 재확인한 미완료 경계

- `index`는 Source Census와 AnalysisPlan으로 provider를 실행하고 direct/donor Language IR를 검증하지만,
  IR record는 아직 영속/stream되지 않고 digest receipt만 남는다.
- type/extends/implements/override/uses-type row는 일부 donor에 있으나 관계 종류별 독립 분모가 없다.
  현재 adapter는 provider 명칭을 좁은 IR 종류로 접으므로 새 정답표 없이 매핑을 고치지 않는다.
- framework/API/ORM/event/queue/cache/external 및 독립 DB snapshot은 donor 자산이다. 새 canonical typed
  adapter와 code↔DB exact reconciliation이 없다.
- canonical FactNode/FactEdge 계약은 있으나 실제 2-pass identity/linker, dedup/relevance gate, bundle writer가
  없다.
- Tauri는 공용 canonical bundle을 읽는 staging importer와 analysis/cancel/progress command가 없다.
  현재 `FactGraphStore::publish`는 다른 임시 DTO를 받는 테스트 primitive다.
- provider cache는 있으나 canonical graph가 없으므로 clean canonical digest와 incremental digest의
  동일성도 아직 증명할 수 없다.

### 대규모 프로젝트 판정

1.1MB 검증은 큰 단일 파일을 끝까지 hash/parse하고 동일 결과를 내는지만 증명한다. 수백~수천 파일
workspace의 peak memory, timeout, 필요한 map-boundary 관계 보존은 아직 별도 holdout이 없다. 현재 provider는
500개 파일/500개 query symbol 부근부터 세부 조회를 줄이고 Rust는 기본 1,500개 파일을 넘으면 deeper
semantic pass를 부분 처리한다. 이 동작은 gap을 숨기지 않는다는 점에서는 정직하지만 최종 대규모 제품
합격 증거는 아니다.

이번 대형 gate는 정확도와 결정성은 통과했지만 판매 제품 성능 기준은 통과하지 못했다. 기존
`language_ir_adapter_validation`과 `provider_and_scip_conversion` 타이머가 같은 시작점을 공유하던 계측
결함을 고쳐 provider, source stability, execution context, direct IR, merge, donor IR, parity를 분리했다.

| 동일 10언어 fixture | 전체 | provider | direct IR | donor IR | IR 합계 |
| --- | ---: | ---: | ---: | ---: | ---: |
| baseline source | 107,882ms | 106,295ms | 73ms | 8ms | 87ms |
| 언어별 1.1MB source | 615,563ms | 106,027ms | 249,037ms | 258,363ms | 507,408ms |

provider 비용은 거의 같고, 큰 입력에서 direct/donor가 같은 source 검증·syntax inventory·IR emission을
각각 수행하는 구간만 507초로 증가했다. 따라서 병목은 언어 도구가 아니라 **이행 기간의 중복 IR 변환
경계**다. 어느 inventory 함수가 얼마를 차지하는지는 다음 세부 계측에서 나누되, direct stream authority
승격 뒤 donor 전체 재변환을 제거해야 한다는 구조적 결론은 현재 수치로 충분히 확정된다.

### 현재 결론과 다음 순서

Batch 판정은 **완료 2/8, 부분 완료 2/8, 미구현 4/8**, 최종 제품 구현 완성도는 **약 40%
(38~42%)**로 유지한다. import는 Batch C 내부에서 완료 조건을 하나 닫았지만 canonical 조립·Code+DB·앱
게시가 생긴 것은 아니기 때문이다.

이 체크포인트 뒤 type relation gate는 완료됐다. explicit exports는 import/export 정답표에 포함됐고
generic references는 제품 관계에서 제외했다. 따라서 direct Language IR을 실제 stream authority로
승격하면서 donor 전체 재변환을 제거한다. 그 다음 canonical definition identity table과 2-pass normalizer/linker로
이동한다. 이미 통과한 definition/call/import fixture만 계속 늘려 전체 품질처럼 보이게 하는 방식은
금지한다.

## 2026-08-08 최신 체크포인트 — definition metadata 마감과 전체 release 재검증

이 절이 이 문서의 현재 최신 판정이다. 위의 과거 체크포인트에 남은 0/117 visibility, raw signature
60/162, Code Memory 261/261, 전체 45% 표기는 당시 문제를 발견한 역사적 수치다.

### 이번에 닫은 계약

- source declaration을 최소 definition metadata의 단일 권위로 사용한다.
- callable signature는 decorator·body·constructor initializer를 제외한 declaration header다.
- visibility는 명시 modifier와 언어별 정적 기본 규칙으로 계산한다.
- reviewed definition 117/117 known visibility, callable 63/63 signature, metadata 사례 37/37을 통과한다.
- migration receipt v5가 언어별 metadata count, digest, audit sample을 기록하고 2회 artifact digest를
  대조한다.
- C++ constructor initializer와 Rust trait implementation visibility edge case를 회귀로 고정했다.
- documentation·annotation·local variable·statement는 최종 시각화 소비자가 없어 수집하지 않는다.

### 전체 기계 검증

| 검증 | 최신 결과 |
| --- | ---: |
| Code Memory | 263/263 |
| shared fact-model | 15/15 |
| Tauri | 38/38 |
| Database Memory | CLI 5/5 + core 149, live-env 27 ignored |
| definition/owner/metadata | 117/117, owner 55/55, signature 63/63, visibility 117/117, 사례 37/37 |
| CALLS/CONSTRUCTS | 35/35 |
| import/export | 39/39 |
| type relation | 90/90 + negative 22 |
| source/build quality | fmt, locked check, clippy `-D warnings`, locked release build 통과 |
| 설치 경로 | signed provider 8 packs를 임시 추출한 뒤 10언어 네 독립 gate 통과 |

clippy 재검증에서 definition metadata helper의 인자가 8개로 늘어난 구조 문제가 발견됐다. lint 억제 대신
`DefinitionMetadataInput` context object로 묶고 다시 전체 회귀와 release를 통과했다.

### 현재 제품 진행도

- Batch A/B: 완료.
- Batch C/G: 부분 완료.
- Batch D/E/F/H: 미구현.
- 전체 정적 제품 구현 완성도: **약 47% (45~49%)**.

닫힌 fixture의 항목별 100%는 검토 corpus 회귀 정확도다. 임의 저장소 전체 정확도나 최종 제품 완성도를
뜻하지 않는다. 실제로는 execution context exact 0/10, partial 10/10이며 canonical linker, typed adapter,
Code+DB bundle, Tauri publish, canonical incremental이 아직 없다.

### 다음 임계 순서

1. provider execution-context exact/partial matrix와 독립 정답 gate
2. frozen·실제 저장소 holdout으로 새 문법 과적합과 coverage 확인
3. canonical definition identity table과 2-pass normalizer/linker·relevance
4. framework/API와 DB/ORM/static SQL/event/queue/cache/external typed adapter
5. canonical Fact Import Bundle과 Tauri staging validation/atomic publish
6. canonical parity 뒤 legacy 제거, clean=incremental·대규모 scale/cancel/OOM gate

## 2026-08-08 최신 체크포인트 — 실제 provider 실행 context 마감

이 절이 현재 최신 판정이다. 위 역사 절의 `exact 0/10`, `partial 10/10`, receipt v5, Code Memory
263/263, 전체 47% 표기는 당시 상태다.

### 이번에 닫은 계약

- Language IR header를 `codebase-workspace.language-ir.v2`로 올리고 실제
  `ProviderExecutionContext`를 포함했다.
- migration receipt는 v6, stream authority는 v2, context reconciliation receipt는 v3이다.
- 실제 실행 context fingerprint가 snapshot identity와 authoritative stream identity에 포함된다.
- 정상 설정 9개 project가 10개 언어를 모두 덮고 exact 10/10을 통과한다.
- 같은 fixture에서 build/config를 제거한 9개 variant는 거짓 exact가 0건이다. TypeScript/JavaScript,
  Python, Java, C#, C/C++, Go, Rust, Dart의 partial/not-executed와 missing dimension을 각각 고정했다.
- config artifact path·용도·SHA-256을 정답과 대조하며 동일 입력 2회에서 context set, snapshot,
  stream, authoritative content digest가 모두 같다.
- `not_executed`는 provider가 실행되지 않았다는 뜻이지 syntax/project-model 사실까지 없다는 뜻이
  아니다. 이 상태에서는 SCIP/LSP/compiler evidence와 provider/compiler resolution만 금지한다.

### 독립 검증 수치

| 검증 | 최신 결과 |
| --- | ---: |
| configured execution context | 9 projects, 10 languages, exact 10/10 |
| missing-context variants | 9 projects, false exact 0, 2회 결정성 |
| definition/owner/metadata | 117/117, owner 55/55, signature 63/63, visibility 117/117, 사례 37/37 |
| CALLS/CONSTRUCTS | 35/35, FP/FN 0 |
| import/export | 39/39, internal 15, external 7, unresolved 14, ambiguous 3 |
| type relation | 90/90, negative 22, 2회 결정성 |
| unit contract tests | Code Memory 275/275, shared fact-model 16/16 |

### 전체 진행도와 다음 구현

- Batch A/B는 완료, C/G는 부분 완료, D/E/F/H는 미구현이다.
- 최종 정적 제품 구현 완성도는 **약 50% (48~52%)**다. 이는 정확도 점수가 아니다.
- `코드에서 정직한 언어 원재료를 뽑는 앞단`은 검토 corpus 기준으로 상당 부분 닫혔다.
- 하지만 최종 시각화가 소비할 canonical definition identity, 2-pass normalizer/linker, deterministic
  relevance, Code+DB Fact Bundle, Tauri atomic publish, canonical incremental은 아직 없다.
- 다음 구현은 **canonical identity table과 normalizer/linker**다. AI가 필요한 단계가 아니며,
  provider symbol·source definition·typed relation을 같은 안정 ID로 접고 unresolved/ambiguous를
  fail-closed로 남기는 결정적 정적 작업이다.
- 동시에 frozen unseen syntax와 실제 OSS/대규모 holdout을 늘려 현재 fixture 100%가 전체 정확도로
  오해되지 않게 한다.

## 2026-08-08 최신 체크포인트 — canonical normalizer/linker와 immutable Fact Bundle

이 절과 문서 맨 위 결론이 현재 판정이다. 위의 `Batch D 미구현`, Code Memory 263/275,
전체 47~50% 표기는 당시 체크포인트의 역사 기록이다.

### 이번에 실제 제품 경로에 연결한 것

1. Language IR v2의 모든 unit stream을 content SHA-256과 record count로 다시 검증한다.
2. pass 1에서 `(analysis unit, provider symbol ID)`를 canonical stable node ID에 등록한다.
3. pass 2에서 등록된 native symbol, 정확한 file identity, 명시적 package/module/namespace만 resolve한다.
4. 같은 이름·비슷한 경로는 target 생성 근거로 사용하지 않는다. absent/ambiguous endpoint는 gap이고
   edge는 0개다.
5. 중복 logical edge는 truth/resolution/evidence를 보수적으로 병합하고, conflicting dispatch는
   `unknown`으로 낮춘다.
6. top-level/type/callable, relation endpoint와 ancestor만 유지하는 deterministic relevance gate를
   적용한다. test/generated/vendor flags와 원본 evidence는 그대로 보존한다.
7. 저장소 밖 cache에 fixed-schema SQLite를 staging으로 쓰고 close→VACUUM→fsync→SHA-256 뒤
   content-addressed 이름으로 rename한다. 외부 manifest가 마지막 complete marker다.
8. 정상 `index`가 이 단계를 반드시 실행한다. canonical 생성이 실패하면 뒤의 호환 output도 게시하지
   않는다.

### 이번 재감사에서 바로 고친 계약 결함

- snapshot을 구 manifest 공식과 새 Language IR 공식이 다르게 계산하던 문제를 하나의
  `SnapshotId::from_execution_inputs`로 통일했다.
- provider identity 수와 canonical node 수를 같은 분모로 빼서 pruning을 과장할 수 있던 receipt를
  `provider identities / canonical nodes / retained / pruned` 네 값으로 분리했다.
- evidence summary나 diagnostic message 같은 사람용 문구가 바뀌면 semantic map identity까지 바뀌던
  문제를 typed semantic projection으로 분리했다. full bundle digest는 byte payload 전체를 계속 보호한다.
- snapshot+semantic만 파일명에 써서 같은 의미·다른 운영 문구 payload가 충돌할 수 있던 문제를
  `canonical-<bundle SHA-256>` content address로 바꿨다.
- 대형 enum variant는 box 처리해 clippy 경고를 억제하지 않고 메모리 표현을 고쳤다.

### 독립 검증

| 검증 | 최신 결과 |
| --- | ---: |
| Code Memory unit | 280/280 |
| shared fact-model | 16/16 |
| 10언어 definition | 117/117, FP/FN 0 |
| 10언어 CALLS/CONSTRUCTS | 35/35, FP/FN 0 |
| import/export | 39/39, internal 15, external 7, unresolved 14, ambiguous 3 |
| type relation | 90/90 + reviewed negative 22 |
| canonical invariants | dangling 0, confirmed-without-evidence 0, duplicate logical edge 0 |
| canonical determinism | semantic digest 10/10, SQLite byte digest 10/10, 2회 동일 |
| 대형 source | 언어별 1.1MB 이상, definition 117/117 + call 35/35 + canonical digest 동일 |
| release install | signed provider 8 packs, 10언어 다섯 독립 gate 연속 통과 |
| build quality | fmt, check, clippy `-D warnings`, release build 통과 |

`100%`는 위 사람이 고정한 닫힌 정답표 안의 정확도다. 실제 임의 저장소, 모든 build variant,
framework/DB 관계, 앱 게시까지 100%라는 뜻이 아니다.

### 현재 진행도와 다음 순서

- 완료: A 공용 계약, B census/plan/scheduler.
- 부분 완료: C 10언어 provider/IR, D language canonical linker/bundle, G hard cut.
- 미구현: E framework/API/ORM/asset+DB 통합, F Tauri import/publish, H canonical incremental.
- 전체 정적 제품 구현 완성도: **약 60% (58~62%)**.

다음 임계 작업은 **Batch E typed adapter**다. 기존 framework/route/ORM/static SQL/test/event/
queue/cache/external donor 중 최종 시각화가 실제 소비하는 데이터만 shared canonical record로 바꾼다.
그 다음 독립 DB catalog를 합친 Code+DB bundle, Tauri atomic publish, canonical incremental 순서다.
AI는 이 정적 파이프라인이 끝난 뒤에만 의미 이름·그룹·설명을 만든다.

## 2026-08-08 최신 체크포인트 — backend HTTP route/handler canonical 통합과 전체 재감사

이 절과 문서 맨 위 결론이 현재 판정이다. 위의 `Batch E 미구현`, Code Memory 280/280,
fact-model 16/16, 전체 60% 표기는 당시 체크포인트의 역사 기록이다.

### 이번에 실제 제품 경로에 연결한 것

```text
framework pack detection + provider-backed donor facts
  → typed Framework IR (route/evidence/gap/accounting)
  → canonical HttpRoute node
  → source File --Exposes--> HttpRoute
  → exact provider handler --Handles--> HttpRoute
  → immutable canonical SQLite bundle
```

- pack signal은 analyzer 선택 근거일 뿐 route 사실이 아니다. 정적 method/path, census에 포함된 source,
  현재 source digest와 정확한 range가 모두 있어야 Framework IR route가 된다.
- `HttpRoute`는 표시 문자열을 다시 parsing하지 않고 typed `method`/`path` details를 가진다. method는
  대문자 normalization, path는 절대 route 형식, qualified identity는 `{METHOD} {path}`와 일치해야 한다.
- handler는 기존 provider symbol identity를 exact resolution할 때만 `Handles`를 만든다. 이름 유사도,
  suffix, 같은 파일 추정은 사용하지 않는다. handler를 못 찾은 route는 숨기지 않고 route와 typed gap만
  남긴다.
- 같은 route donor가 반복되어도 계획 분모를 부풀리지 않는다. receipt는 raw donor 후보 수와 exact duplicate
  제거 뒤 planned route 수를 분리하고 `planned = emitted + rejected`를 강제한다.
- FrameworkBindings capability는 language adapter와 framework adapter가 동시에 주장하지 않는다. canonical
  framework adapter가 unit별 receipt를 한 번만 소유한다.
- framework pack JSON bytes와 adapter version digest를 provider-set/snapshot identity에 넣었다. 규칙만
  바뀌었는데 source snapshot을 재사용하는 일이 없다.

### 재감사에서 찾은 근본 결함과 수정

1. Dart Shelf의 provider 결과는 정상이었지만 보조 Tree-sitter type enrichment가 일부 문법을 완전하게
   parse하지 못해 언어 전체가 실패했다. 선택적 type 분석 실패는 `TypeRelations`만 partial로 낮추고,
   이미 확인한 definition/call/import/framework fact는 보존하도록 LSP와 SCIP 경계를 분리했다.
2. Java Spring fixture의 Maven dependency에 version이 없어 유효하지 않은 project model이었고 JDT
   classpath가 비어 call edge가 사라졌다. Spring Web 6.1.8을 명시해 분석기 결함과 fixture 설정 결함을
   분리했다. 정답 fixture도 실제 build tool 관점에서 유효해야 한다.
3. raw framework fact 수를 capability 후보 분모로 써 exact duplicate가 coverage를 낮출 수 있었다.
   duplicate 제거 뒤 planned 수를 별도로 계산하고 unit audit을 canonicalized route/gap에서 다시 만들었다.

### 최신 독립 검증

| 검증 | 결과 |
| --- | ---: |
| Code Memory unit | 285/285 |
| shared fact-model | 17/17 |
| Tauri unit | 38/38 |
| Database Memory | CLI 5/5 + core 149, live 환경 의존 27 ignored |
| 10언어 definition | 117/117, FP/FN 0, owner 55/55, signature 63/63, visibility 117/117 |
| 10언어 CALLS/CONSTRUCTS | 35/35, FP/FN 0 |
| import/export | 39/39, internal 15, external 7, unresolved 14, ambiguous 3 |
| type relation | 90/90 + reviewed negative 22 |
| execution context | 정상 9 projects/10언어 exact 10/10, config 제거 9 variants false exact 0 |
| framework flow | 10/10: HTTP 9개는 typed Framework IR + canonical route/edge, C 1개는 event donor flow |
| large source | 언어별 provider-visible source 1.1MB 이상, definition 117/117 + call 35/35 유지 |
| canonical integrity | dangling 0, evidence-less confirmed 0, duplicate logical edge 0 |

위 100% 수치는 사람이 검토하고 고정한 닫힌 corpus의 회귀 결과다. 임의 저장소의 모든 framework 버전,
동적 registration, build variant, generated code를 100% 이해한다는 뜻이 아니다.

### 현재 판정과 남은 임계 경로

- 완료: A 공용 계약, B census/plan/scheduler.
- 부분 완료: C 10언어 provider/IR, D canonical linker/bundle, E typed 통합, G hard cut.
- 미구현: F Tauri import/publish, H canonical incremental.
- 전체 정적 제품 구현 완성도: **약 64% (62~66%)**. 정확도 점수가 아니다.

다음 순서는 **ORM/static SQL + 독립 DB catalog reconciliation**이다. 그 다음 test→production relation,
event/queue/cache/external/config typed boundary, Tauri validation/atomic publish, clean=incremental과 실제
OSS·수백~수천 파일 scale/cancel/OOM 순서로 닫는다. 현재 최종 화면이 직접 소비하지 않는 데이터는 새로
수집하지 않는다.

## 2026-08-08 최신 체크포인트 — test→production 정적 관계

이 절과 문서 맨 위 결론이 현재 판정이다. 위 문단의 `test→production relation` 미완료 표기는 당시
체크포인트의 역사 기록이다.

- exact test framework 문법으로 확인한 TestCase만 만든다.
- TestCase body 안의 exact provider call이 기존 project-local production definition으로 해결될 때만
  confirmed `Tests` edge를 만든다.
- 파일명, 테스트 이름, 함수 이름 유사도와 디렉터리 근접성은 연결 근거로 쓰지 않는다.
- 정적으로 연결하지 못한 테스트는 삭제하거나 추측하지 않고 `unresolved_target` gap으로 남긴다. 이 gap은
  향후 AI가 candidate를 제안할 입력이며, AI가 정적 confirmed 값을 덮어쓰지는 않는다.

독립 ground-truth gate 결과는 10언어 모두 `TestCase 2 / confirmed Tests 1 / static gap 1`이다. 사람 검토
positive 10개는 10/10 연결됐고, 이름만 비슷한 negative 10개는 10/10 거부됐다. exact marker/call range,
canonical endpoint·truth·evidence, source SHA-256 불변, semantic/bundle digest 2회 결정성을 함께 검증했다.
현재 전체 회귀는 Code Memory 285/285, shared fact-model 17/17, fmt와 clippy `-D warnings`가 통과한다.

사용자 지시에 따라 이 체크포인트에서 범위를 멈춘다. DB 분석, AI adapter, 다른 관계 종류는 이번 작업에
포함하지 않았다.

## 2026-08-08 최신 체크포인트 — desktop static TracePath

위의 “멈춘다”는 test→production 작업 범위에 대한 당시 기록이다. code engine의 canonical 사실을
변경하지 않고, desktop query 층에서 다음 정적 제품 경계를 추가했다.

- canonical node/edge/capability receipt/typed gap을 읽는 bounded `TracePath` query
- confirmed exact 실행 edge만 hop으로 채택, candidate·virtual/interface·구조/type/test edge 배제
- canonical `handler --Handles--> route`를 원본 변경 없이 runtime `route -> handler` 순서로 해석
- complete/partial/gap/cycle/depth-limit과 stable path identity/evidence union
- representative path를 static region과 Base semantic input에 연결하고 selected fact query 제공
- evidence가 특정 fact를 가리키는 gap은 그 경로에만 적용하고 evidence-less unit/workspace gap만 넓게 적용
- representative entry/path 예산을 static region별 round-robin으로 배분해 대형 한 영역의 독점 방지
- trace 전체 region이 area 안에 있을 때만 AI representative trace 선택 허용
- verified trace가 없으면 map에 node를 하나만 내보내 unordered anchor를 가짜 chain으로 만들지 않음

실제 TypeScript Express canonical bundle에서 `/health`는 route→handler 2단계, handler를 해결하지 못한
`/unknown`은 단일 route+gap으로 분리됐다. 가짜 handler/edge는 0개다. 이는 code engine의 10언어
정답표 수치를 바꾸는 작업이 아니며, 남은 정적 확장은 ORM/SQL/DB와 event/queue/cache/external의 exact
canonical producer, 대규모 representative omission receipt, clean=incremental이다.

## 2026-08-08 최신 체크포인트 — 정확도 보존 cold-path 성능 엔지니어링

이 절이 현재 성능 판정이다. 목표는 분석 항목, provider evidence, canonical truth 계약을 줄이지 않고
최초 분석의 wall time만 낮추는 것이었다. 이름·경로 유사도로 관계를 보충하거나 빈 결과를 성공으로
바꾸는 최적화는 허용하지 않았다.

### 적용한 구조 변경

- weighted provider scheduler를 work-conserving 방식으로 바꿨다. 선두의 무거운 job이 예산을 기다릴 때
  뒤의 가벼운 job을 실행하되, 최종 결과는 원래 ordinal로 다시 정렬해 결정성을 유지한다.
- LSP 요청을 최대 16개 bounded batch로 전송하고 JSON-RPC response id로 입력 순서에 재결합한다.
  `documentSymbol`, `prepareCallHierarchy`, `outgoingCalls`, `definition`의 사실 판정 규칙은 바꾸지 않았다.
- definition의 기존 3회 provider-only 조회를 position마다 `250ms`씩 기다리지 않고, 모든 exact source
  position에 대해 세 번의 전역 round로 실행한다. 각 round 뒤 같은 cache를 소비하므로 retry 횟수와
  abstention 계약은 이전과 같다.
- AI local partition은 최초 batch 기본 4개, 실패 partition 재시도 기본 2개로 분리했다. prompt, evidence,
  verifier, model과 reasoning effort는 그대로다. 각각
  `CODEBASE_WORKSPACE_AI_MAX_PARALLEL`, `CODEBASE_WORKSPACE_AI_RETRY_MAX_PARALLEL`로 1~8 범위 조정이 가능하다.
- `scip-typescript --max-file-byte-size`를 scheduler shard의 최대 파일이 아니라 Source Census가 승인한
  프로젝트 전체 최대 source 크기로 계산한다. indexer가 상위 tsconfig의 더 큰 member를 조용히 건너뛰지
  못한다.
- immutable Fact snapshot은 workspace별 최대 2개를 process memory에 보관한다. pointer와 bundle metadata가
  달라지면 전체 digest/integrity 검증을 다시 하며, 같으면 `Arc`를 재사용해 node 선택마다 SQLite 여섯
  테이블을 전량 materialize하지 않는다.
- provider, LSP method, AI partition마다 machine-readable performance receipt를 stderr에 남긴다. 따라서
  다음 병목은 추측이 아니라 job/method별 wall time으로 찾는다.

### 실제 저장소 cold/warm 영수증

측정 대상은 `D:\meeting-overlay-assistant`이며 Source Census는 파일 1,232개를 열거해 1,065개를 승인하고,
1,037개 file-language 항목을 7개 analysis unit으로 계획했다. 별도 임시 cache를 사용해 기존 사용자
snapshot을 바꾸지 않았다.

| 항목 | 변경 전 또는 중간 측정 | 최종 cold 측정 | 판정 |
| --- | ---: | ---: | --- |
| Python LSP provider, 884 files | 703,743ms | 56,316ms | 92.0% 감소 |
| Rust LSP job A | 189,847ms | 7,264ms | 96.2% 감소 |
| Rust LSP job B | 125,106ms | 7,236ms | 94.2% 감소 |
| provider+SCIP conversion 전체 | 550,315ms 중간 측정 | 72,911ms | 86.8% 감소 |
| canonical bundle까지 최초 전체 | 비교 가능한 기존 전체 영수증 없음 | **86,018ms** | 현재 cold 기준선 |
| 같은 입력 warm 전체 | 해당 없음 | **9,334ms** | provider cache hit |

최종 Python LSP는 15,041개 요청을 처리했다. 그중 definition 11,396개가 41,973ms로 남은 가장 큰
단일 병목이다. 이를 생략하거나 lexical 추정으로 바꾸지 않았으므로 다음 최적화도 compiler/LSP가 보장하는
동일 결과를 유지하는 범위에서만 허용한다.

추가 cold 실험에서 batch 크기를 16→64로 올리자 전체는 83,475ms였지만 Python provider는 반복 실행별
55,603~57,080ms로 batch 16의 56,316ms와 같은 변동 범위였다. canonical snapshot/digest/count도 완전히
같았다. 왕복 batch 수만 줄고 Pyright의 실제 definition 계산 시간은 줄지 않았으므로, 더 큰 burst의
메모리·서버 안정성 비용을 정당화하지 못한다고 판정해 기본값 16을 유지한다.

### 정확도·결정성 대조

| canonical 결과 | 기존 앱 bundle | 최적화 후 cold bundle | 변화 |
| --- | ---: | ---: | ---: |
| nodes | 5,276 | 5,298 | +22 |
| edges | 12,011 | 12,058 | +47 |
| evidence | 13,112 | 13,159 | +47 |
| file coverage | 1,037 | 1,037 | 0 |
| typed gaps | 1,028 | 1,024 | -4 |
| dangling endpoint | 0 | 0 | 0 |
| evidence 없는 confirmed | 0 | 0 | 0 |

감소가 아니라 대형 TypeScript source 누락을 찾아 복구해 fact가 늘었다. cold와 warm은 snapshot id,
semantic digest, bundle digest와 위 count가 전부 동일했다. 현재 회귀는 Code Memory **288/288**,
Tauri **68 passed / 4 environment-only ignored**, frontend **6/6**, 두 crate `cargo fmt --check`, frontend
lint와 production build를 통과한다.

AI 4-way 최초 batch는 단위·동시성 회귀로 검증했지만 실제 외부 모델 16-partition E2E 시간은 비용을
발생시키므로 이번 자동 측정에 포함하지 않았다. 따라서 **86초는 정적 canonical bundle 생성 시간**이며,
전체 제품 최초 분석 시간은 여기에 실제 AI 호출 시간이 더해진다.

### 후속 데이터 처리 기법 조사

현재 41,973ms의 Python definition 병목을 그래프 DB·GPU·무조건적인 batch 확대로 해결할 수 있는지와,
SCIP/Kythe/Glean/Salsa/Skyframe/Tree-sitter의 실제 처리 방식을 현재 파이프라인에 대조했다. 결론은
compiler semantic program을 한 번 만든 뒤 전체 occurrence를 배출하는 **batch semantic index를 Python
그림자 provider로 먼저 검증**하고, 이후 content-addressed analysis unit → fact ownership/reverse
invalidation → query DAG → persistent incremental parsing 순으로 진행하는 것이다.

상세 결정과 승격 gate는
[`STATIC-PIPELINE-PROCESSING-RESEARCH-2026-08-08.md`](./STATIC-PIPELINE-PROCESSING-RESEARCH-2026-08-08.md)에
기록했다. 공개 `scip-python@0.6.6`은 Windows path separator를 정규식으로 escape하지 않아 시작부터
실패했고, 전역 monkey patch 실험은 path 자체를 오염시켜 파일 0개짜리 빈 index를 만들었다. 따라서 이
실험의 3.172초는 성능 수치로 채택하지 않았다.

## 2026-08-08 최신 체크포인트 — 열 언어 provider shadow 완료

최소 Windows source patch의 Python 실험을 포함해 지원 중인 열 언어 전체를 같은 comparator로 조사했다.
정본은
[`PROVIDER-SHADOW-EVALUATION-2026-08-08.md`](./PROVIDER-SHADOW-EVALUATION-2026-08-08.md)다.

- Python candidate는 실제 884-file 저장소를 약 13.7초에 처리했지만 current definition 38개,
  occurrence 6,072개, relation 139개를 보존하지 못해 production 승격을 거부했다.
- TypeScript/JavaScript same-provider shadow는 current fact를 100% 보존했다.
- C#은 restore를 생략할 때 실제 call 한 건이 사라져 production의 `--skip-dotnet-restore`를 제거했다.
- Go/Rust/Dart candidate도 하나 이상의 current confirmed fact를 보존하지 못했다.
- Java와 C/C++ candidate는 공식 Windows packaging 경계를 통과하지 못했다.

공통 `compare-scip` CLI는 provider symbol 문자열을 exact definition/evidence locator로 rebasing하고,
candidate-only extension과 current-fact regression, raw SHA-256과 normalized semantic digest를 분리한다.
provider-to-provider 비교만으로 절대 정확도를 주장하지 못하도록 report의 `productionEligible`은 항상
false다. 따라서 production provider 선택은 C#의 정확도 수정 외에는 변경하지 않았다.
