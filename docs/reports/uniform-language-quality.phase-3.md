# 지원 언어 공통 품질 Phase 3 보고서

Date: 2026-08-01  
Status: Core and framework-provider gates complete; full route-to-DB conformance remains in progress.

## Delivered

- active 12개 언어를 동일한 core quality gate로 검증한다.
- framework catalog의 84개 pack을 provider와 함께 실제 index하여 검증한다.
- 각 pack에 대해 framework detection, declared fact, source file/range, resolved symbol,
  route method/path, HANDLES ownership을 검사한다.
- 게이트 실패를 허용 목록으로 숨기지 않고 CI 병합 차단 조건으로 연결했다.

## Root causes fixed during the gate

- C++ Crow signal이 source masking 때문에 감지되지 않던 문제: `include:` signal로 교정.
- Dart analyzer가 package metadata 없이는 의도적으로 제외되는 문제: fixture에 최소
  `.dart_tool/package_config.json`을 제공.
- Go framework import signal이 source masking과 맞지 않던 문제: `package:` signal로 교정.
- Java Spring/Spring Boot 중복 fact와 fixture annotation 줄 배치 문제: canonical fact와
  실제 source 범위를 함께 검증하도록 교정.
- Angular component decorator를 SERVICE로 오인하던 문제.
- Vue `defineComponent`를 RENDERS로 오인하던 문제.
- Fastify instance receiver와 pack signal이 어긋나던 문제.
- API Platform의 일반 PHP Entity를 schema로 오인하던 문제.
- Django/Starlette 등록형 route에서 경로만 읽고 두 번째 handler 인자를 놓치던 문제.

## Verification

- framework provider gate: `84 passed, 0 failed`
- Rust engine tests: `139 passed, 0 failed`
- provider gate는 `run-framework-provider-gate.ps1`로 CI에서 매번 실행한다.
- Ruby LSP Bundler 플랫폼 문제는 provider 결과를 폐기하지 않고 `warning` 진단으로
  보존한다. 즉 Ruby도 성공으로 위장하지 않으며 gem 해석 불완전 가능성이 UI/진단에
  남는다.

## Boundary

이 결과는 84개 pack의 공통 provider/fact/route ownership 품질을 보장한다. 모든
프로젝트에서 route → middleware → handler → service → repository → DB까지 동일하게
확정한다는 뜻은 아니다. 그 단계는 framework/ORM별 cross-file flow와 DB exact-join
conformance를 별도 게이트로 확장해야 한다.
