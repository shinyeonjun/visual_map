# 지원 언어 공통 품질 Phase 1 보고서

Date: 2026-08-01  
Status: Complete for the current 12-language active set; overall objective remains in progress.

## Delivered

- 공통 핵심 품질 계약을 추가했다: `code_memory/docs/contracts/UNIFORM-CORE-QUALITY.md`
- 12개 활성 언어를 대상으로 동일한 strict gate를 추가했다:
  `code_memory/tests/gates/run-uniform-core-quality-gate.ps1`
- 게이트가 fixture 목록·bridge 목록·framework catalog 목록의 언어 집합 드리프트를 즉시 실패시킨다.
- CI가 release bridge를 빌드한 뒤 동일한 uniform gate를 실행하도록 연결했다.
- Rust provider가 소규모 workspace에서 `indexed` 상태를 반환하면서 `CALLS=0`을 내보내던 문제를 수정했다. Rust도 기본 LSP reference enrichment를 사용하도록 공통 provider 경로를 고쳤다.
- Rust 회귀 테스트를 추가했다.

## Verification

| Check | Result |
| --- | --- |
| `cargo fmt --manifest-path rust/Cargo.toml -- --check` | passed |
| `cargo test --manifest-path rust/Cargo.toml` | 138 passed, 0 failed |
| `cargo build --release --manifest-path rust/Cargo.toml` | passed |
| `run-language-semantic-gate.ps1` | 12/12 passed |
| `run-uniform-core-quality-gate.ps1` | 12/12 passed, 0 skipped |
| `git diff --check` | passed |

현재 공통 gate가 확인하는 것은 문서·심볼·직접 cross-file `CALLS`·endpoint·source range·중복 제거·오류 진단이다. 따라서 이 보고서는 “12개 언어의 공통 core baseline을 CI에서 강제한다”는 완료 보고서이지, 모든 framework/ORM의 route-to-DB 흐름이 동일하게 완성됐다는 보고서는 아니다.

## 발견 및 해결한 문제

Rust fixture는 provider 오류 없이 `indexed`로 끝났지만 semantic relation이 0개였다. 기존 설정은 Ruby 또는 환경변수가 있을 때만 reference enrichment를 켰기 때문에, Rust의 call hierarchy가 충분하지 않은 작은 workspace에서 직접 호출이 사라졌다. Rust를 기본 enrichment 대상에 포함하고 동일 gate로 재검증해 숨은 부분 성공을 차단했다.

## 남은 차이와 다음 단계

- 지원 스택과 제품 지원 문서에서 12개 active 언어와 Kotlin/Swift 2개 target 언어를 분리 표기했다. Kotlin/Swift는 provider와 동일 gate가 생기기 전까지 active supported가 아니다.
- framework pack gate는 pack fact와 adapter 실행을 확인하지만, 모든 pack의 route → middleware/handler → service → repository → DB 흐름을 아직 공통 fixture로 검증하지 않는다.
- Ruby provider는 Windows Bundler platform 경고를 출력하지만 현재 fixture 결과와 gate는 통과한다. release 전에는 이 경고를 provider 정상 상태와 분리해 진단해야 한다.
- 다음 구현 단계는 provider diagnostic부터 Tauri inventory gap, snapshot link, API answer/UI evidence까지 동일한 gap ID와 실패 상태를 보존하는 것이다.
