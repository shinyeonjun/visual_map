# hierarchy-map Phase 3 보고

Status: Done  
Date: 2026-08-15  
Plan: `docs/plans/hierarchy-map.md`

## Goal

짧은 주소·긴 주소, production·archived처럼 **같은 바깥 계약**이면 기능 하나로 묶는다.  
레거시 코드는 별도 기능이 아니라 같은 기능의 증거로 남긴다.

## 변경 요약

### 1. 계약 동치 묶음 (`feature_contracts.rs`)

entrypoint를 `contract_identity`와 경로 정규화 결과로 동치 판정해 union-find로 묶는다.

- `/health` + `/api/v1/health` → 같은 기능
- 메서드 없음(`*`) + `GET` → 같은 기능
- `GET` + `POST` 같은 경로 → **다른** 기능 (의도적)

하드코딩 없음. 모든 판정은 `normalize_contract_path` / `contract_identity` 규칙을 따른다.

### 2. 대표 entrypoint 선택

묶음 안에서 production 경로·명시 HTTP 메서드·안정 id 순으로 대표를 고른다.  
archived route는 `entrypoint_ids`에 남지만 라벨·키는 production 쪽이 우선한다.

### 3. 파일 분할

`features.rs`(550줄)를 역할별로 나눴다.

| 파일 | 역할 |
|------|------|
| `features.rs` | 기능 빌드 오케스트레이션 |
| `feature_contracts.rs` | 계약 동치·묶음 |
| `feature_build.rs` | `FeatureGroup` 조립 |

## 테스트

### 단위

`src/views/overview/feature_contracts.rs`
- `짧은_경로와_api_접두_경로는_같은_계약이다`
- `메서드가_없는_경로는_명시_메서드와_같은_계약으로_묶인다`
- `다른_http_메서드는_같은_계약이_아니다`
- `동치_묶음이_하나의_그룹이_된다`

### 통합

`tests/capability_domain.rs`
- `짧은_경로와_긴_경로는_같은_기능으로_묶인다`
- `같은_http_계약은_production과_archived를_기능_하나로_묶는다` (기존)

## 검증 명령

```powershell
cd d:\visual_map
cargo test --test capability_domain 짧은_경로와_긴_경로는_같은_기능으로_묶인다
cargo test --test capability_domain 같은_http_계약은_production과_archived를_기능_하나로_묶는다
cargo test features::feature_contracts
cargo test --test capability_domain --test domain_pipeline
```

## Rollback

`feature_contracts.rs` 묶음 로직과 `features.rs`의 그룹 빌드 경로만 되돌리면 된다.

## 다음

Phase 4: 기능 진입점에서 이어진 실행 길만 화면에 올리기.
