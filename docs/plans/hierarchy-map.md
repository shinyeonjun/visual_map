# 도메인 → 기능 → 실행 길 Implementation Plan

Status: Proposed  
Scale: Large  
Research: `docs/research/hierarchy-map.md`

## Goal

화면+서버 웹앱에서, 사람이 이런 순서로 코드를 이해한다.

1. 큰 일 덩어리가 보인다. (일정 관리, 로그인)
2. 덩어리를 누르면 동작이 보인다. (일정 등록, 일정 삭제)
3. 동작을 누르면 **코드에서 찾은 일만**, 그 기능이 도는 순서가 보인다.
4. 코드에 없는 일은 그리지 않는다. 확신 없으면 “확실하지 않음”만 붙인다.
5. AI가 꺼져 있어도 위 지도는 나온다. AI는 이름만.

끝난 상태의 한 문장: **빠진 동작 없이, 다른 일이 한 카드에 섞이지 않고, 길을 따라가면 실제 코드와 만난다.**

## Current Facts

- 스캔·언어 분석·프레임워크 보강·함수 그래프·도메인 묶기·clean 저장은 있다.
- 기능을 도메인에 붙일 때 묶음의 대표 이름만 본다. (`src/domain/formation.rs` `assign_features_to_domains`)
- 다른 일을 합치지 말라는 강한 제약 없음. 개수가 많으면 가드 없이 합친다. (`src/domain/clustering.rs`)
- 화면의 길은 모든 함수 그래프다. (`src/flow/builder.rs`) 기능 범위가 아니다.
- 앱은 동작 8개만 보여 주고, 길은 숫자만. AI 결과가 비면 지도를 버린다.

## Proposed Behavior

- 동작은 자신이 속한 묶음에 붙는다. 카드 밖에 안 떠 있는다.
- 로그인과 일정 관리가 한 카드가 되지 않는다.
- 같은 바깥 동작은 기능 하나다.
- 화면에 보이는 길은 그 기능의 진입점에서 실제로 이어진 코드 일이다.
- 첫 화면은 큰 덩어리. 누르면 동작, 누르면 길. 캔버스 세부 배치는 화면 단계에서.
- 도구는 정보를 보여 준다. 가치는 사용자가 만든다.

## Success Criteria

기준: 화면+서버가 있는 웹앱 하나, 그리고 등록/삭제/로그인이 있는 작은 fixture.

- 로그인 동작이 일정 카드 안에 없다.
- 일정 등록·삭제가 일정 카드 안에 있다. 밖에 떠 있지 않다.
- 같은 등록이 두 줄로 안 나온다.
- 일정 등록 길에 “알림”처럼 **코드에 없는 단계가 없다.**
- 길에 있는 단계는 실제 함수/호출과 이어진다.
- AI 없이 위가 화면에 나온다.

## Non-Goals

- 언어 10개를 같은 깊이로 만들기
- 4번째 분석 층(대분류)
- 코드 한 줄씩 재생
- 화면/서버/DB 층만 그린 그림
- 없는 단계를 이해용으로 창작
- 새 언어·새 프레임워크 추가
- 클러스터 알고리즘 교체
- 전체 UI 리디자인 (펼치기만)

## Architecture

```text
바깥 동작(요청, 버튼에 해당하는 진입점)
  → 기능
  → 관련 기능을 도메인으로 묶음
  → 기능마다 길 = 진입점에서 이어진 실제 코드 일
  → 화면은 도메인 → 기능 → 길 순으로 펼침
  → AI는 이름만
```

엔진이 층을 맞고, 앱은 펼친다. 앱이 층을 새로 만들지 않는다.

## Implementation Phases

단계 보고는 `docs/reports/hierarchy-map.phase-N.md`에 남긴다.

### Phase 1: 동작이 덩어리에 붙는다

Goal:
- 카드 밖에 떠 있는 동작이 없어진다.

Deliverables:
- `assign_features_to_domains`가 대표 이름이 아니라 **그 묶음에 들어간 동작 집합**으로 붙인다.
- 기능의 `domain_ids`가 비지 않는 테스트를 작은 등록/로그인 fixture로 추가한다.

Verification:
- 로그인은 로그인 쪽, 일정 등록은 일정 쪽에 붙는다.
- `cargo test` 해당 테스트.

Rollback:
- 해당 함수와 테스트만 되돌린다.

### Phase 2: 다른 일이 한 카드로 안 합쳐진다

Goal:
- 로그인과 일정이 한 덩어리가 되지 않는다.

Deliverables:
- 서로 다른 바깥 경로 앞부분(auth vs sessions 같은)은 함부로 합치지 않는다.
- 개수가 많다고 가드를 끄고 합치지 않는다. (`clustering.rs` force merge)

Verification:
- fixture에서 인증 기능과 일정 기능이 다른 도메인.
- 동작이 많은 fixture에서 무관한 두 일이 한 id로 안 합쳐짐.

Rollback:
- 제약·병합 가드만 되돌린다.

### Phase 3: 같은 동작은 하나

Goal:
- 짧은 주소와 긴 주소, 예전 폴더와 지금 폴더가 같은 일이면 기능 하나.

Deliverables:
- 계약 맞추기(`contract_identity` 등)로 중복 기능을 줄인다.
- 예전 코드는 증거로만 남기고 기능을 하나 더 만들지 않는다.

Verification:
- 같은 등록이 feature 하나. 중복 라벨 없음.

Rollback:
- 계약 키/collapse만 되돌린다.

### Phase 4: 길은 그 기능의 실제 일 순서

Goal:
- 화면에 올리는 길이 “모든 함수”가 아니라 “이 동작에서 이어진 코드 일”이 된다.
- 코드에 없는 단계를 추가하지 않는다.

Deliverables:
- 기능의 진입점에서 도달하는 흐름만 그 기능의 길로 붙인다.
- 단계 = 실제 유닛/호출. 이름은 읽기 쉽게 해도 되고, 없는 일(알림 등)은 안 넣는다.
- 확신이 낮은 호출은 빼지 않고 표시만.

Verification:
- 일정 등록 길에 등록과 무관한 함수 전체가 안 실림.
- 코드에 없는 단계 문자열이 없음.

Rollback:
- 기능-길 연결만 되돌린다. 내부 함수 그래프 생성은 남겨도 된다.

### Phase 5: 앱에서 펼치기

Goal:
- 첫 화면 큰 덩어리 → 누르면 동작 전부 → 누르면 길.
- AI 없어도 지도가 나온다.

Deliverables:
- 동작 8개 절단 제거.
- 길을 숫자만이 아니라 목록/순서로 표시. 캔버스 vs 옆 칸은 이 단계에서 고른다.
- semantic 빈 결과여도 clean 지도를 연다. AI는 이름만 덮어씀.

Verification:
- AI 설정 없이 도메인·기능·길이 보임.
- 기능이 9개여도 잘리지 않음.

Rollback:
- 앱 표시·성공 조건만 되돌린다.

### Phase 6: 신뢰 표시와 굳히기

Goal:
- “확실하지 않음”이 보인다.
- 기준 웹앱에서 사람 확인. 다른 언어는 세 층이 안 깨지는지 스모크.

Deliverables:
- 상태(확실함/후보)를 카드·기능·길에 표시.
- 기준 저장소 확인 목록: 섞임, 빠짐, 중복, 길에 없는 일.

Verification:
- 사람 확인 + 기존 `cargo test`.
- 단계 보고 `docs/reports/hierarchy-map.phase-6.md`.

Rollback:
- 표시만 되돌리면 엔진 층은 유지.

## Test Plan

```powershell
cd d:\visual_map
cargo test --test capability_domain --test domain_pipeline
cargo test
```

앱 단계는 엔진 release 후 기준 웹앱 한 번 분석.  
성공은 테스트 통과만이 아니다. 기준 앱에서 세 층이 눈으로 맞아야 한다.

## Risks And Assumptions

- 가정: 1.0 기준은 화면+서버 웹앱. 데스크톱만 있는 프로그램은 나중.
- 가정: 바깥 동작이 거의 없는 라이브러리는 동작 층이 얇다. 그때만 내부 일을 기능으로 쓴다. 바깥 동작이 있는데 내부 일로 바꾸지 않는다.
- 위험: 길을 기능 범위로 자르면 “정보가 줄어 보인다”. 원칙은 줄 단위를 줄이는 것이지, 없는 일을 채우는 것이 아니다.
- 위험: 도메인 이름 다수결을 너무 세게 나누면 카드가 너무 많아진다. Phase 2는 “다른 일”만 막고, 같은 일의 세분은 한 카드에 둔다.
- UI 캔버스 배치는 제품 층을 바꾸지 않는다.

## Codex/Claude Prompt

```text
Read docs/research/hierarchy-map.md and docs/plans/hierarchy-map.md.
Implement Phase 1 only: features must belong to the domain cluster they were grouped into, not only the winner key.
Do not add languages, UI, or clustering algorithm changes.
Add a fixture-level test for login vs schedule features landing on the matching domains.
Run the smallest relevant cargo tests. Write docs/reports/hierarchy-map.phase-1.md.
```
