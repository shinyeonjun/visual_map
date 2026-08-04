# TASK-0 현재 엔진 재측정 보고서

- 측정일: 2026-08-03 (KST)
- 대상 작업 트리: `D:\project\visual_map`
- 기준 HEAD: `f794752`
- 엔진: `code_memory\rust\target\debug\code-memory-language.exe`
- 원시 산출물: `D:\visual_map_reliability_lab\_results\task0-20260803\`
- 재현 스크립트: `D:\visual_map_reliability_lab\_runs\measure-task0.ps1`

## 측정 방법

세 레포를 모두 새 호환 CLI 캐시에서 `index_repository`로 인덱싱한 뒤 같은 캐시에서
`get_architecture`와 `query_graph`를 실행했다. 기존 `e2e_runs` 산출물은 사용하지 않았다.

`CodeInventory.summary`는 현재 Tauri 정규화 규칙과 동일하게 엔진의 전체 코드 노드
쿼리 결과를 집계했다. 라우트는 UI navigation·테스트·URL 리소스를 제외했고, 함수/클래스류/
모듈 분류와 handler·service·repository 이름 규칙은
`src-tauri/src/workspace/code/inventory.rs`의 규칙을 따랐다.

실제 API 수는 엔진 결과와 별도로 소스 선언을 확인했다.

- Spring Petclinic: 메서드 매핑 15개 + `RouterFunctions.resources("/")` 1개 = 16개
- FastAPI template: 백엔드 `@router`/`@app` route decorator 23개 = 23개
- cURL: HTTP 서버 route를 제공하는 레포가 아니므로 HTTP ENDPOINT 기대값은 N/A이며 0개가 정상

## 1. 언어별 커버리지와 제외 사유

| 프로젝트 | 언어 | provider | files_found | files_indexed | files_excluded | files_missing | status |
|---|---|---:|---:|---:|---:|---:|---|
| java-spring-petclinic | Java | native-lsp | 62 | 62 | 0 | 0 | indexed |
| java-spring-petclinic | JavaScript | scip | 22 | 22 | 0 | 0 | indexed |
| fastapi-full-stack-fastapi-template | Python | native-lsp | 47 | 47 | 0 | 0 | indexed |
| fastapi-full-stack-fastapi-template | TypeScript | scip | 95 | 95 | 0 | 0 | indexed |
| c-curl | C | native-lsp | 892 | 377 | 515 | 0 | indexed-partial |
| c-curl | C++ | native-lsp | 265 | 182 | 83 | 0 | indexed-partial |
| c-curl | Python | native-lsp | 52 | 52 | 0 | 0 | indexed |

cURL의 C/C++ 제외 파일은 모두 `provider-excluded`로 기록됐다. 또한 C와 C++ 각각에
다음 부분 분석 진단이 기록됐다.

> large-workspace semantic enrichment limited for 378 source files; declarations and
> imports retained, non-boundary per-symbol reference and lexical queries skipped

즉 누락은 조용히 완료 처리되지 않았고, 선언·import는 남기되 일부 심볼 참조가 제한된
`indexed-partial`로 표시됐다. Java의 7개·Python의 114개·cURL의 C/C++/Python
provider 진단은 각각 컴파일 컨텍스트 또는 provider 경고이며 파일 누락(`files_missing`)
은 세 레포 모두 0이다.

## 2. CodeInventory 요약

| 프로젝트 | routes | handlers | services | repositories | functions | classes | modules | files | unknown |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| java-spring-petclinic | 16 | 3 | 0 | 4 | 176 | 63 | 125 | 84 | 280 |
| fastapi-full-stack-fastapi-template | 23 | 0 | 5 | 0 | 333 | 140 | 132 | 142 | 2,751 |
| c-curl | 0 | 4 | 1 | 0 | 4,213 | 2,177 | 29 | 945 | 7,838 |

`unknown`은 확정 역할로 분류할 수 없는 노드이며 누락을 의미하지 않는다. C/C++처럼
역할 명명 규칙이 약한 언어에서는 이 값이 커지고 handler/service/repository 값이
낮아지는 것이 현재 분류 계약의 의도된 결과다.

## 3. Architecture 노드 종류

| 프로젝트 | 노드 종류별 개수 |
|---|---|
| java-spring-petclinic | FILE 84, MODULE 31, EXTERNAL_LIBRARY 27, ENDPOINT 16, SERVICE 20, PACKAGE 9, DATA_RESOURCE 1, PROJECT 1 |
| fastapi-full-stack-fastapi-template | FILE 142, COMPONENT 167, EXTERNAL_LIBRARY 71, ENDPOINT 23, MODULE 30, EVENT 28, DYNAMIC_BOUNDARY 18, PACKAGE 4, DATA_RESOURCE 2, PROJECT 1 |
| c-curl | FILE 945, EXTERNAL_LIBRARY 228, MODULE 25, DYNAMIC_BOUNDARY 13, PACKAGE 3, EVENT 2, DATA_RESOURCE 1, PROJECT 1 |

## 4. Architecture 엣지 종류

| 프로젝트 | 엣지 종류별 개수 |
|---|---|
| java-spring-petclinic | IMPORTS 43, CALLS 46, CONTAINS 124, ENTRYPOINT_TO 36, WRITES 1, USES_LIBRARY 94 |
| fastapi-full-stack-fastapi-template | IMPORTS 125, CALLS 180, CONTAINS 176, ENTRYPOINT_TO 173, READS 4, WRITES 5, USES_LIBRARY 175, DYNAMIC_CALL 18 |
| c-curl | IMPORTS 2,187, CALLS 563, CONTAINS 973, READS 14, USES_LIBRARY 392, DYNAMIC_CALL 13 |

## 5. ENDPOINT 대조

| 프로젝트 | 독립 소스 기준 실제 API | architecture ENDPOINT | 차이 | 판단 |
|---|---:|---:|---:|---|
| java-spring-petclinic | 16 | 16 | 0 | 일치. MVC 매핑 15개와 WebFlux resource route 1개를 모두 포함 |
| fastapi-full-stack-fastapi-template | 23 | 23 | 0 | 일치. router decorator 23개와 일치 |
| c-curl | N/A (HTTP 서버 아님) | 0 | N/A | 정상. C 라이브러리 함수는 HTTP ENDPOINT로 오인되지 않음 |

## 6. 단계별 소요 시간

| 프로젝트 | 총 인덱싱 | file discovery/cache lookup | provider/SCIP conversion | framework analysis |
|---|---:|---:|---:|---:|
| java-spring-petclinic | 443 ms | 245 ms | 13 ms | 12 ms |
| fastapi-full-stack-fastapi-template | 533 ms | 318 ms | 36 ms | 15 ms |
| c-curl | 143,079 ms | 217 ms | 137,710 ms | 3,138 ms |

c/curl의 총 시간은 C/C++ `clangd`와 혼합 레포의 Python provider 대기 시간이
지배한다. 첫 무캐시 실행에서 공용 JSON 실행기의 120초 감시 제한에 걸렸으나, 제품
엔진을 바꾸지 않고 TASK-0 측정 스크립트가 CLI를 직접 실행하도록 하여 143.079초의
완료 결과를 확보했다.

## 7. 관찰점과 다음 TASK에 대한 경계

1. Java와 FastAPI는 현재 엔진에서 소스 기준 API 수와 ENDPOINT 수가 정확히 일치한다.
2. C/curl은 정적 C/C++ 프로젝트에서 역할 레인이 아닌 다른 구조 축이 필요하다는
   TASK-2의 실측 근거를 제공한다. C/C++의 역할 분류는 각각 handler 4/service 1,
   repository 0으로 매우 희소하다.
3. `language-index.languages[]`의 C++ 집계는 182 indexed/83 excluded인데,
   같은 산출물의 `analysis_units[]`에는 `root` C++ unit이 1 indexed/0 excluded로
   기록되어 있다. 이 두 표현은 동일한 범위를 설명하지 않는다. 이번 TASK-0에서는
   측정값을 보존하고 임의로 보정하지 않는다. 이후 커버리지 표시를 수정할 때
   `languages[]`를 사용자-facing 언어 커버리지의 기준으로 삼고, `analysis_units[]`
   의미를 별도로 정리해야 한다.
4. 다음 작업은 이 측정 결과를 전제로 TASK-1 `VisualNode` 구조화부터 시작한다.

## 원시 산출물

- `D:\visual_map_reliability_lab\_results\task0-20260803\task0-measurement.json`
- 각 프로젝트의 `*.language-index.json`
- 각 프로젝트의 `*.architecture-index.json`
