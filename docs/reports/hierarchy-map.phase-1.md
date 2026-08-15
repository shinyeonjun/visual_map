# hierarchy-map Phase 1 보고

Status: Done  
Date: 2026-08-15  
Plan: `docs/plans/hierarchy-map.md`

## Goal

카드 밖에 떠 있는 동작(orphan feature)이 없어지게 한다.

## 변경 요약

### 핵심 수정

`assign_features_to_domains`가 도메인 그룹의 **대표 키(`group.key`)** 가 아니라, 클러스터 형성 시 이미 모아 둔 **진입점·자원 ID 멤버십**으로 기능을 배정한다.

이전:
- 기능 진입점 경로 → `capability_key_from_path` → `group.key` 일치 시에만 `domain_ids` 부여
- 한 클러스터에 auth·sessions capability가 같이 들어가면 winner key만 도메인으로 남아, 나머지 키의 기능은 `domain_ids`가 비었다

이후:
- `group.entrypoint_ids` / `group.resource_ids`에 들어 있는 ID로 역인덱스를 만들고, 기능의 `entrypoint_ids`·`resource_ids`가 그 도메인에 붙는다
- 클러스터에 함께 묶인 capability의 기능도 같은 도메인 카드에 올라간다

### 파일 구조

`src/domain/formation.rs`(639줄)를 모듈로 분할했다.

| 파일 | 역할 |
|------|------|
| `formation/mod.rs` | 파이프라인 오케스트레이션 |
| `formation/capability_data.rs` | capability → 클러스터 입력 벡터 |
| `formation/cluster_groups.rs` | 클러스터 → 도메인 그룹·멤버십 |
| `formation/constraints.rs` | 병합 제약·미배정 유닛 |
| `formation/feature_assignment.rs` | 기능 → 도메인 배정 (Phase 1 핵심) |

### 제거

- `formation.rs` 단일 파일 (모듈로 대체)
- `assign_features_to_domains`의 `capability_key_from_path` 경로 조회 및 `FactStore` 의존

## 테스트

### 단위

`src/domain/formation/feature_assignment.rs`
- `진입점이_들어있는_도메인에_기능을_붙인다`

### 통합

`tests/capability_domain.rs`
- `로그인과_일정_기능은_자신이_속한_도메인_묶음에_붙는다`

## 검증 명령

```powershell
cd d:\visual_map
cargo test --test capability_domain --test domain_pipeline
cargo test formation::feature_assignment
```

## 검증된 것

- 코드 구조상 orphan 원인(대표 키 단일 조회) 제거
- 도메인 그룹에 이미 수집된 entrypoint/resource 멤버십으로 배정 경로 단일화

## 아직 검증되지 않은 것

- `meeting-overlay-assistant` 기준 저장소에서 orphan feature = 0 (사용자 실행 필요)
- Phase 2: auth와 sessions가 서로 다른 카드로 유지되는지 (클러스터 병합 제약은 이번 단계 범위 밖)

## Rollback

`formation/feature_assignment.rs`의 배정 로직과 이번에 추가한 테스트만 되돌리면 된다. 클러스터 형성 코드는 동작 변경 없음.
