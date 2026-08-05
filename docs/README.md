# Visual Map 문서

이 디렉터리는 현재 제품 동작을 설명하는 문서만 유지한다. 완료된 계획과 중간 보고서는
Git 이력에서 확인하고, 제품 상태는 계약 문서와 최신 POC 보고서를 기준으로 판단한다.

## 먼저 읽을 문서

| 목적                              | 문서                                                                    |
| --------------------------------- | ----------------------------------------------------------------------- |
| 현재 구현 상태와 검증 결과        | [2026-08-05 POC 검증 보고서](reports/poc-validation-2026-08-05.md)      |
| 제품이 공식적으로 책임지는 범위   | [제품 지원 경계](product-support.md)                                    |
| 언어·프레임워크·DB 지원 목록      | [공식 지원 스택 계약](contracts/visual-map-supported-stack-contract.md) |
| 코드·DB 통합 데이터 형식          | [엔진 인덱스 데이터 계약](contracts/engine-index-data-contract.md)      |
| 코드 관계의 확정·후보·미확인 규칙 | [코드 지능 계약](contracts/visual-map-code-intelligence-contract.md)    |
| 설치 앱 사용 중 문제 해결         | [문제 해결](troubleshooting.md)                                         |
| 엔진 개발·운영 장애 기록          | [엔진 트러블슈팅](troubleshooting/code-memory-engine.md)                |
| 보안·개인정보 경계                | [보안과 개인정보](security-privacy.md)                                  |

## 디렉터리 기준

- `contracts/`: 구현과 UI가 지켜야 하는 현재 계약
- `reports/`: 재현 가능한 최신 검증 결과
- `troubleshooting/`: 증상, 로그, 재현 조건, 원인, 수정 코드, 검증 결과
- `demo/`: 데모 절차
- `design/`: 현재 UI 설계 자료
- `assets/`: 문서에 직접 사용되는 캡처

## 문서 작성 규칙

완료나 지원을 주장할 때는 다음을 함께 남긴다.

1. 검증한 Git commit과 외부 저장소 commit
2. 실행 명령과 입력 조건
3. 원문 로그의 핵심 줄
4. 성공·실패 판정 기준
5. 결과 파일 위치
6. 남은 한계

추정, 기억, 스크린샷만으로 `완료`라고 쓰지 않는다. 오래된 결과와 최신 결과가
충돌하면 최신 POC 보고서가 우선하며, 제품 계약 변경은 별도 코드·테스트 변경과 함께
진행한다.
