# hierarchy-map Phase 2 보고

Status: Done  
Date: 2026-08-15  
Plan: `docs/plans/hierarchy-map.md`

## Goal

로그인과 일정처럼 **서로 다른 바깥 계약**이 한 도메인 카드로 합쳐지지 않게 한다.  
개수 상한(`max_count`) 때문에 force-merge가 가드를 끄지 않게 한다.

## 변경 요약

### 1. 계약 접두 cannot-link (`formation/constraints.rs`)

capability 쌍마다 `key`가 다르면 병합 금지 쌍으로 추가한다.

- `key`는 이미 `capability_key_from_path`로 경로 첫 유효 세그먼트에서 추출된 값이다.
- auth/sessions를 하드코딩하지 않는다. `/auth/...`와 `/sessions/...`는 경로 정규화 결과로 자연히 다른 key가 된다.
- production vs test 경로 금지 규칙은 기존과 동일하게 유지한다.

### 2. force-merge 가드 유지 (`clustering.rs`)

`max_count` 초과 시 돌던 `best_merge_pair_force`가 `can_merge`를 건너뛰던 문제를 수정했다.

- force 단계에서도 `forbidden_pairs` + `can_merge`(http/call/flow 구조 연결)를 모두 통과한 쌍만 병합한다.
- 병합 가능한 쌍이 없으면 루프를 종료하고 클러스터 수가 `max_count`를 넘을 수 있다. (의도된 동작)

## 테스트

### 단위

- `formation/constraints`: `서로_다른_계약_접두는_병합_금지`, `같은_계약_접두는_병합_금지가_아니다`
- `clustering`: `force_merge도_병합_금지와_구조_가드를_지킨다`

### 통합

- `capability_domain`: `서로_다른_계약_도메인은_개수_상한_때문에_한_카드로_합쳐지지_않는다`

## 검증 명령

```powershell
cd d:\visual_map
cargo test --test capability_domain 서로_다른_계약_도메인은_개수_상한_때문에_한_카드로_합쳐지지_않는다
cargo test domain::formation::constraints
cargo test domain::clustering
cargo test --test capability_domain --test domain_pipeline
```

## 검증된 것 (코드 기준)

- 다른 계약 접두 → `MergeConstraints.forbidden_pairs`
- force-merge도 동일 제약 + 구조 가드 적용
- 도메인 수가 많아져도 무관한 계약을 억지 병합하지 않음

## 한계 / Phase 3 이후

- 같은 계약의 중복 기능 collapse는 Phase 3 범위.
- `authentication` vs `auth`처럼 경로 표기만 다른 동일 업무는 아직 별도 key로 남을 수 있다.

## Rollback

`constraints.rs`의 key 비교 추가와 `clustering.rs` force-merge 가드만 되돌리면 된다.
