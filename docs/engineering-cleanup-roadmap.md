# Engineering cleanup roadmap

Status: active baseline cleanup plan, 2026-08-09.

이 문서는 새 기능 목록이 아니다. 현재 end-to-end vertical slice를 보존하면서 구형 호환 경로와 대형 저장소 병목을 안전하게 제거하기 위한 순서를 고정한다. 각 단계는 독립적으로 검증되고, canonical Fact Graph의 의미 digest가 바뀌지 않아야 한다.

## 현재 기준선

- 로컬 폴더에서 Source Census와 Analysis Plan을 만든다.
- 10개 언어 provider가 Language IR 후보를 생성한다.
- canonical linker가 근거가 있는 사실만 Fact Graph로 확정한다.
- immutable SQLite bundle, 정적 TracePath, AI 의미 분석, L0/L1 지도 조회가 앱까지 연결되어 있다.
- AI 결과는 기존 Fact ID만 참조하고 정적 사실을 덮어쓸 수 없다.
- 아직 legacy donor projection, 대형 orchestrator, 전량 snapshot 읽기 경로가 남아 있다.

## P0. 재현 가능한 기준선 고정

목표: 이후 리팩터링의 비교 대상을 신뢰할 수 있게 만든다.

- 작업 트리에서 생성물, 로컬 provider, 임시 분석 DB, 로그와 캐시를 추적하지 않는다.
- Rust/TypeScript 포맷, lint, test, build를 모두 통과시킨다.
- 같은 fixture를 두 번 분석했을 때 semantic digest와 SQLite bundle digest가 동일해야 한다.
- 근거 없는 `confirmed` 관계는 0개여야 한다.

완료 조건: 깨끗한 `main`, 원격과 동일한 commit, CI가 같은 명령을 재현한다.

## P1. canonical-only 실행 경로

목표: desktop 분석의 주인을 새 Analysis Plan과 canonical pipeline 하나로 만든다.

1. Framework/Test IR이 legacy projection이 아니라 typed donor input을 직접 받게 한다.
2. 앱 분석 경로에 `canonical-only` gate를 둔다.
3. desktop staging에서 임시 `language-index.json`과 `architecture-index.json` 생성을 중단한다.
4. CLI 호환 출력은 parity 확인 기간에만 유지하고 소비자가 0개가 되면 삭제한다.

완료 조건:

- 전환 전후 canonical semantic digest가 같다.
- desktop 전체 테스트가 통과한다.
- 분석 staging에 legacy JSON이 생기지 않는다.
- 삭제 대상의 실제 read site가 0개임을 검색과 테스트로 증명한다.

## P2. 대형 orchestrator 분리

목표: 동작을 바꾸지 않고 책임과 테스트 경계를 작게 만든다.

- `index_project`를 census, planning, provider execution, linking, publish 단계 coordinator로 분리한다.
- Language IR adapter를 source inventory, definition mapping, relation mapping, receipt/artifact 모듈로 분리한다.
- provider별 예외 규칙은 공용 linker에 섞지 않고 해당 adapter에 둔다.
- 파일명과 디렉터리는 파이프라인 단계와 같은 용어를 사용한다.

완료 조건: fixture별 노드·관계·gap·coverage와 digest가 분리 전후 동일하다. 이 단계에서 새 추출 규칙을 추가하지 않는다.

## P3. SQLite query-backed read model

목표: 선택 한 번에 Fact Graph 전체를 메모리에 올리는 경로를 없앤다.

- SQLite bundle을 권위 저장소로 유지한다.
- map overview, selection detail, evidence, TracePath를 고정된 typed query로 읽는다.
- 목록은 pagination과 명시적 상한을 사용한다.
- 전체 `nodes`, `edges`, `evidence` 벡터를 snapshot cache에 보관하지 않는다.
- revision digest별 immutable connection/read-model cache만 허용한다.

완료 조건: 대형 fixture에서 선택 지연시간과 peak memory를 수치로 기록하고, 결과 parity 테스트를 통과한다.

## P4. 영수증과 진단 데이터 분리

목표: 제품 기본 경로에는 판단에 필요한 최소 데이터만 남긴다.

- production receipt에는 provider, 버전, 입력 digest, 상태, coverage, gap 요약만 저장한다.
- 상세 raw payload와 비교 로그는 명시적 diagnostic mode에서만 만든다.
- 사용자 소스와 AI 입력 임시 파일은 성공·실패·취소 모두에서 정리한다.

## P5. 남은 언어 품질 감사

목표: 지원 언어마다 같은 증거·실패·coverage 계약을 적용한다.

- ground-truth fixture를 언어 문법 특성별로 유지한다.
- C/C++ compile context, C# semantic 호출, Go identity, Rust trait/impl, Dart package resolution을 각각 검증한다.
- SCIP 위치의 UTF-16/UTF-8 변환과 다중 바이트 source range를 교차 검증한다.
- 동적으로만 결정되는 관계는 AI로 확정하지 않고 typed gap 또는 candidate 설명으로 남긴다.

## 삭제하면 안 되는 것

정리 과정에서도 다음은 제품의 안전장치이므로 유지한다.

- `crates/fact-model`의 공용 데이터 계약과 schema version
- Source Census와 Analysis Plan
- source evidence, truth class, typed gap, capability receipt
- canonical linker와 immutable SQLite publish
- atomic publish와 이전 revision 보존
- AI가 기존 Fact ID만 참조하도록 하는 출력 검증

## 단계별 검증 순서

1. 대상 consumer와 read/write site를 확인한다.
2. characterization test로 현재 canonical 결과를 고정한다.
3. 한 단계만 패치한다.
4. unit → crate → desktop → frontend 순으로 검증한다.
5. digest, 관계 수, gap, coverage, 메모리·시간을 전후 비교한다.
6. parity가 확인된 뒤에만 구형 코드를 삭제한다.

DB 분석, 앱 내 대화, 협업 기능은 이 정리 계획과 별도다. 현재 우선순위는 코드 분석 vertical slice를 하나의 canonical 경로로 단순화하고 대형 저장소에서도 같은 사실을 더 적은 비용으로 제공하는 것이다.
