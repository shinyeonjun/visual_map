# hierarchy-map Phase 4 보고

Status: Done  
Date: 2026-08-15  
Plan: `docs/plans/hierarchy-map.md`

## Goal

기능의 실행 길이 **전역 함수 CFG 전체**가 아니라, 그 기능 진입점에서 실제로 이어진 flow만 포함하게 한다.

## 변경 요약

### 1. `feature_flows.rs` 추가

`FeatureFlowIndex`가 진입 유닛의 flow에서 시작해 `FlowLink.target_flow_id`만 따라 BFS한다.

- 같은 파일의 무관한 handler flow는 붙지 않음
- 호출 그래프로 이어진 callee flow만 포함
- 하드코딩 없음. 링크는 `flow/builder.rs`가 만든 정적 call link를 그대로 사용

### 2. 기능 빌드 경로 변경

- `feature_build.rs`: `scope_units` 전체 flow 수집 제거 → `flow_ids`를 입력으로 받음
- `features/mod.rs`: entrypoint 유닛 기준으로 `collect_for_units` 호출
- `assign_orphan_flows_to_nearest_endpoint` **삭제** (같은 파일이면 무조건 붙이던 Phase 4 역행 로직)

### 3. 제거한 코드

- `flows_by_owner` 기반 flow 수집 (`feature_build`)
- 파일 공유 기준 orphan flow 배정 (`assign_orphan_flows_to_nearest_endpoint`, `nearest_endpoint_feature_index`)

내부 `ExecutionFlowGraph` 생성(`flow/builder.rs`)은 그대로 둠.

## 테스트

### 단위

`src/views/overview/features/feature_flows.rs`
- `진입점에서_호출_링크로_이어진_flow만_포함한다`
- `같은_파일의_무관한_flow는_포함하지_않는다`

### 통합

`tests/capability_domain.rs`
- `기능_실행_길은_진입점에서_이어진_flow만_포함한다`

## 검증 명령

```powershell
cd d:\visual_map
cargo test --test capability_domain 기능_실행_길은_진입점에서_이어진_flow만_포함한다
cargo test features::feature_flows
cargo test --test capability_domain --test domain_pipeline
```

## Rollback

`feature_flows.rs`와 `features/mod.rs`의 flow 배정 경로, 삭제한 orphan 배정만 되돌리면 된다.

## 다음

Phase 5: 앱 UI에서 도메인 → 기능 → 길 펼치기.
