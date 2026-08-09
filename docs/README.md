# Current product documents

이 디렉터리에는 현재 제품과 구현을 설명하는 문서만 둔다. Atlas,
Leiden grouping, mode-based UI, POC screenshot처럼 2026-08-07 hard cut 이전의
설계는 현재 계약이 아니다.

- [프로젝트 개요와 실행 방법](../README.md)
- [Runtime architecture](architecture.md)
- [Engineering cleanup roadmap](engineering-cleanup-roadmap.md)
- [Security and privacy boundary](security-privacy.md)

## Engine contracts

- 코드 추출기, 지원 언어, evidence와 Language IR 계약:
  [`code_memory/docs`](../code_memory/docs/README.md)
- 공용 정적 데이터와 저장 schema: [`crates/fact-model`](../crates/fact-model/README.md)
- AI 의미 출력과 검증 계약: [`crates/semantic-model`](../crates/semantic-model/README.md)
- DB metadata 분석 계약: [`db_memory/docs`](../db_memory/docs/README.md)

엔진 문서는 extractor 동작과 데이터 계약을 정의한다. 데스크톱 사용자 경험은
runtime architecture가 정의한다. 현재 Fluent 2 workbench, provider/model/reasoning
설정, 정확한 CLI hand-off도 별도 목업이 아니라 runtime 계약에 포함된다.

현재 코드 엔진에는 검증된 donor-to-Language-IR migration bridge가 남아 있다.
이는 canonical 경로로 이동하기 위한 shadow gate이며, 제거 순서와 완료 조건은
engineering cleanup roadmap의 P1을 따른다.
