# hierarchy-map Phase 6 보고

Status: Done  
Date: 2026-08-15  
Plan: `docs/plans/hierarchy-map.md`

## Goal

“확실하지 않음”이 카드·기능·실행 길에 보인다. 사람이 기준 웹앱에서 세 층을 눈으로 검증할 수 있다.

## 변경 요약

### 1. clean bundle status 투영 (`map.rs`, `models.rs`)

- **도메인**: `confirmed` → `verified`, `ambiguous` → `shared`, 그 외 → `candidate`
- **기능**: `FeatureStatus`를 `verified` / `candidate`로 매핑
- **실행 길**: `dynamicBoundaryIds`, edge `status`, 단계 노드 종류로 flow/step 신뢰도 계산
  - `dynamicBoundary` 노드 또는 `confirmed`가 아닌 edge → `candidate`
  - 그 외 → `verified`

### 2. 앱 신뢰 표시 (`App.tsx`, `domain-card.tsx`, `domain.ts`, `styles.css`)

- 도메인 카드: 신뢰도 % + `trustLabel` 상태
- 인스펙터 기능 목록: 기능별 `확인됨` / `후보` 배지
- 실행 길: flow 단위 배지, 단계별 점(후보는 강조색)
- 지도 범례: 확인됨 / 후보 점 추가

## 기준 저장소 수동 확인 목록

기준 웹앱 한 번 분석 후 아래를 눈으로 확인한다.

| 항목 | 확인 내용 |
|------|-----------|
| 섞임 | 서로 다른 계약(예: 로그인 vs 일정)이 한 기능/도메인에 섞이지 않는가 |
| 빠짐 | 알려진 API·핵심 동작이 도메인/기능 목록에서 빠지지 않는가 |
| 중복 | 같은 계약이 기능 두 개로 쪼개지지 않는가 |
| 길에 없는 일 | 실행 길에 없는 호출·파일 로직이 기능에 과하게 붙지 않는가 |

후보로 표시된 항목은 “틀렸다”가 아니라 “정적 근거가 약하다”는 뜻이다.

## 테스트

### Tauri `map.rs`

- `후보_기능과_미해결_flow_단계는_candidate로_표시한다` (신규)
- 기존 4개 테스트 유지

## 검증 명령

```powershell
cd d:\visual_map\frontend\src-tauri
cargo test map::tests

cd d:\visual_map
cargo test --test capability_domain --test domain_pipeline
```

## 롤백

- `MapFeature.status`, `MapFlow.status`, `MapFlowStep.status` 및 UI 배지/범례만 제거
- 엔진·clean bundle 계약은 유지
