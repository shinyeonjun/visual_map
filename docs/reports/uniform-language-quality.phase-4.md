# 지원 언어 공통 품질 Phase 4 보고서

Status: Complete for the common active-language baseline — 2026-08-01

## 범위

framework pack이 감지되는지만 보지 않고, 대표적인 각 언어 흐름에서 다음을 실제로
검증했다.

`HTTP_ROUTE 또는 EVENT_HANDLER → HANDLES/HANDLES_EVENT → handler → CALLS → service`

모든 확정 관계는 endpoint, source path/range, service document를 가져야 하며, 같은
호출을 중복 출력하면 실패한다.

## 결과

`code_memory/tests/gates/run-framework-flow-gate.ps1`에 12개 active language fixture를
추가했고 release bridge 기준 `12/12`를 통과했다.

검증 언어/pack:

- JavaScript/TypeScript — Express
- Python — FastAPI
- Java — Spring MVC
- C# — ASP.NET Core Minimal API
- C/C++ — GTK/GLib event, Crow
- Go — Gin
- Rust — Axum
- PHP — Laravel
- Ruby — Rails
- Dart — Shelf

## 실제로 발견하고 고친 문제

1. Java Spring fixture가 Maven project metadata와 compiler release를 갖추지 않아
   route는 잡혀도 cross-file service CALLS가 나오지 않았다. 표준 `.project`,
   `.classpath`, Maven compiler release를 fixture에 넣어 provider가 실제 project
   context에서 동작하도록 고쳤다.
2. Rust LSP가 같은 호출에 넓은 range와 좁은 range를 함께 반환했다. 공통 relation
   merge에서 동일 endpoint/path의 겹치는 CALLS는 가장 정밀한 range 하나만 남기고,
   서로 다른 call site는 보존하도록 수정했다.
3. PHP fixture가 instance constructor 심볼만 반환하는 형태여서 의도한 service
   method 호출을 검증하지 못했다. 실제 static service call 형태로 바꿔 provider가
   올바른 method endpoint를 내도록 했다.
4. Ruby의 `OrderService.new.create_order`처럼 괄호 없는 member call은 기존 공통
   source heuristic에서 `REFERENCES`로 떨어졌다. Ruby 문법을 공통 SCIP 변환 계층의
   bounded call 판별에 추가하고 negative/positive unit test를 고쳤다.

## CI 고정

framework provider gate 다음에 framework flow gate를 실행하도록 `.github/workflows/ci.yml`
에 연결했다. 따라서 pack이 감지만 되고 실제 handler→service 관계가 끊긴 변경은 release
CI에서 통과하지 않는다.

## 범위의 한계

이번 게이트는 84개 pack 각각의 모든 고급 route/DI/event 조합을 인증한 것이 아니라,
12개 active 언어에서 하나씩 대표 flow를 공통 계약으로 인증한 것이다. 나머지 pack은
기존 provider gate의 fact/ownership 범위를 유지하며, 고급 capability는 별도 fixture로
승격해야 한다.

## 다음 단계

Phase 5에서는 같은 기준을 `handler → service/repository → static query → exact DB
snapshot`으로 확장한다. DB snapshot이 없거나 table/schema/column이 모호하면 code-side
candidate만 남기고 confirmed 관계를 만들지 않는다.

## 공통 상태의 UI 전달

이후 architecture payload를 `v2`로 올려 language/provider coverage와 framework adapter
상태를 `languages`/`frameworks`로 함께 전달했다. 코드 화면은 이 값을 그대로 사용해
언어별 `정상/부분/확인 필요`, framework별 `정상/확인 필요`, fact 개수를 표시한다.
따라서 provider가 실패했는데 화면에서 빈 성공처럼 보이는 경로를 줄이고, 기존
`partial`/gap 상태를 숨기지 않는다.
