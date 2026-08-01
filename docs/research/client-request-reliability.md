# 다언어 클라이언트 요청 연결 신뢰성 연구

- 상태: Phase 1 구현 시작
- 규모: Large
- 목적: 프론트·백엔드·RPC 클라이언트 호출을 공통 관계로 연결하되, 확정할 수 없는 동적 호출을 확정 관계로 위장하지 않는다.

## 판정

정적 literal URL, 안전한 문자열 결합, 알려진 설정값, framework prefix/mount는 자동 연결할 수 있다. 환경·reflection·generated runtime에 의존하는 호출은 정적 결과만으로 확정할 수 없으며 `candidate` 또는 `unknown`으로 보존해야 한다. 선택적 runtime 관찰은 정적 분석을 보강할 수 있지만, 기본 인덱싱을 임의 코드 실행으로 바꾸지 않는다.

## 현재 구조의 근본 경계

- `FrameworkFact`에는 서버 `HTTP_ROUTE`가 있지만 클라이언트 HTTP 요청을 표현하는 공통 fact가 없다.
- `CodeCall`은 호출자·호출 대상은 표현하지만 method, URL, base URL, 해석 상태를 표현하지 못한다.
- snapshot link에는 `code_call`과 `code_handle`은 있지만 `client_request`가 없다.
- 따라서 현재 `axios/fetch/HttpClient/requests`를 기존 `CALLS`로 표시하면 서버 endpoint 연결을 보장하지 못한다.

## 공통 진실 모델

```text
static-confirmed  : method/path와 서버 endpoint가 정적으로 유일하게 일치
runtime-confirmed : 실행 중 method/path 관찰과 서버 endpoint가 일치
candidate          : 일부 값 또는 endpoint가 모호하지만 연결 후보가 있음
unknown            : 호출은 발견했으나 URL/대상을 해석하지 못함
excluded           : 테스트 코드·생성물·지원 범위 밖 등 명시적 제외
```

호출이 실행되지 않았다는 사실은 연결이 없다는 증거가 아니다. runtime 결과는 정적 결과를 덮어쓰지 않고 별도 evidence로 병합한다.

## 안전한 구현 범위

1. 공통 `ClientRequest` IR과 stable ID를 추가한다.
2. 12개 active 언어에서 공통적으로 발견 가능한 HTTP client literal 패턴을 추출한다.
3. URL template·문자열 결합은 안전한 상수 전파 범위만 확정하고 나머지는 후보/unknown으로 둔다.
4. method/path를 서버 route의 canonical identity와 비교한다.
5. client request → server route link의 evidence와 source range를 보존한다.
6. 동적 호출, 별도 저장소, 외부 설정은 자동 확정하지 않는다.
7. runtime 관찰은 별도 opt-in phase로 둔다.

## 살충제 패러독스 방지

언어별 대표 fixture만 맞추지 않고, 같은 의미를 다음 변형으로 반복한다.

- literal URL과 single/double quote
- template/string interpolation
- local constant concatenation
- method variable 또는 dynamic URL
- 같은 path의 다른 method
- test-only 호출
- 주석·문자열 속 가짜 호출
- 동명이인 서버 route
- base URL이 환경변수인 호출
- 파일 이동·함수명 변경·선언 순서 변경

확정 결과는 positive fixture, 확정하지 않는 결과는 negative fixture로 함께 검증한다.

## 보안·개인정보

runtime phase에서도 body, header, token, cookie, secret, row data를 저장하지 않는다. method, redacted URL, source location, trace identity만 보관한다. 기본 정적 분석은 프로젝트 코드를 실행하지 않는다.

