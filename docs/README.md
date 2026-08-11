# Current product documents

이 디렉터리에는 현재 제품과 구현을 설명하는 문서만 둔다. Atlas,
Leiden grouping, mode-based UI, POC screenshot처럼 2026-08-07 hard cut 이전의
설계는 현재 계약이 아니다.

- [프로젝트 개요와 실행 방법](../README.md)
- [분석 제품 경계 — 정적·DB·AI·서빙 책임](analysis-product-boundary.md)
- [Runtime architecture](architecture.md)
- [Engineering cleanup roadmap](engineering-cleanup-roadmap.md)
- [Security and privacy boundary](security-privacy.md)
- [meeting-overlay-assistant 분석 결과 감사 (2026-08-10)](meeting-overlay-analysis-audit-2026-08-10.md)

## Engine contracts

- 코드 추출기, 지원 언어, evidence와 Language IR 계약:
  [`code_memory/docs`](../code_memory/docs/README.md)
- 공용 정적 데이터와 저장 schema: [`crates/fact-model`](../crates/fact-model/README.md)
- AI 의미 출력과 검증 계약: [`crates/semantic-model`](../crates/semantic-model/README.md)
- DB metadata 엔진 개요: [`db_memory`](../db_memory/README.md)

엔진 문서는 extractor 동작과 데이터 계약을 정의한다. 데스크톱 사용자 경험은
runtime architecture가 정의한다. 현재 Fluent 2 workbench, provider/model/reasoning
설정, 정확한 CLI hand-off도 별도 목업이 아니라 runtime 계약에 포함된다.

새 분석 항목의 채택 여부와 정적·DB·AI 사이의 책임 충돌은
`analysis-product-boundary.md`를 우선한다. 실제 저장소 감사 문서는 그 계약을
검증하는 corpus 결과이지 범용 제품 규칙이 아니다.

현재 코드 엔진과 데스크톱 사이에는 canonical SQLite 경로 하나만 남아 있다.
이전 donor/architecture/collector 호환 출력은 제거됐으며, 남은 정리와 실측 완료
조건은 engineering cleanup roadmap을 따른다.
