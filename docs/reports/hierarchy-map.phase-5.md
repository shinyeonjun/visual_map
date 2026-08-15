# hierarchy-map Phase 5 보고

Status: Done  
Date: 2026-08-15  
Plan: `docs/plans/hierarchy-map.md`

## Goal

앱에서 도메인 → 기능 → 실행 길을 펼쳐 볼 수 있게 한다. AI 의미 분석이 비어 있어도 clean bundle 정적 지도가 열린다.

## 변경 요약

### 1. semantic 빈 결과 허용 (`analysis.rs`)

`semantic.domains`가 비어 있어도 오류로 중단하지 않고 `build_map(clean, &[])`를 호출한다. 도메인·기능 이름은 clean bundle의 정적 `label`을 사용한다.

### 2. clean bundle flow 투영 (`map.rs`, `models.rs`)

- `flows`, `units` 데이터셋을 읽는다.
- `FeatureJson.flow_ids`로 기능에 연결된 flow만 `MapFeature.flows`에 넣는다.
- 각 flow는 owner 유닛 `qualifiedName`과 entry에서 BFS로 순회한 단계 목록(`MapFlowStep`)을 포함한다.
- `entry`/`exit` 노드는 단계 목록에서 제외한다.

### 3. 앱 인스펙터 펼치기 (`App.tsx`, `domain.ts`, `styles.css`)

- 기능 8개 `slice(0, 8)` 제한 제거.
- 기능 행 클릭 시 해당 기능의 실행 길 목록을 인스펙터에 표시.
- 단계는 라벨 + 종류(호출, 분기, 반환 등)로 순서대로 나열.

## 테스트

### Tauri `map.rs`

- `semantic이_비어_있어도_정적_라벨로_지도를_만든다`
- `기능에_연결된_flow_단계를_순서대로_투영한다`

## 검증 명령

```powershell
cd d:\visual_map\frontend\src-tauri
cargo test map::tests

cd d:\visual_map
cargo test --test capability_domain --test domain_pipeline
```

앱 수동 확인:

1. AI 없이(또는 semantic 실패 후) 분석 → 도메인 카드가 보이는지
2. 도메인 선택 → 기능 전체 목록(9개 이상도 잘리지 않음)
3. 기능 클릭 → 실행 길 단계가 순서대로 표시되는지

## 롤백

- `analysis.rs` empty semantic 가드 복원
- `MapFeature.flows` 및 UI 펼치기 제거
- `slice(0, 8)` 복원
