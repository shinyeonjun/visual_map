# 지원 언어 공통 품질 Phase 2 보고서

Date: 2026-08-01  
Status: Foundation complete; broader capability/failure matrix remains in progress.

## Delivered

- \`CodeInventoryGap\`에 내용 기반의 결정적 ID를 추가했다.
- legacy gap에 ID가 없어도 동일한 입력으로 안정적인 ID를 재생성한다.
- code inventory → snapshot 변환에서 실제 존재하는 \`code:<id>\` endpoint만
  \`relatedIds\`로 연결한다.
- 알려진 endpoint가 없는 provider 진단은 global gap으로 유지해 API 읽기 결과에서
  숨기지 않는다.
- 코드 adapter 버전을 5로 올려 이전 snapshot의 관계 근거를 자동 stale/reindex
  대상으로 만들었다.
- runtime contract가 새 gap ID를 선택적으로 검증한다.

## Root cause fixed

이전 snapshot gap ID는 배열 index에 의존했고, 한 endpoint는 \`code:\` 정규화를
빠뜨렸다. provider 결과 순서가 달라지면 같은 문제의 ID가 바뀌었고, API flow는
실제 관련 코드 노드 또는 global provider 진단을 찾지 못할 수 있었다.

## Verification

- Tauri full test: \`250 passed, 0 failed, 5 ignored\`
- stable gap ID regression: passed
- global provider gap API visibility regression: passed
- Clippy with \`-D warnings\`: passed
- Rust format check: passed
- frontend runtime contract test: \`5 passed\`
- \`git diff --check\`: passed

## Remaining

provider 상태와 gap은 이제 하나의 stable ID로 snapshot/API까지 전달되지만,
모든 provider diagnostic을 \`indexed-partial\`, \`provider-failed\`, \`missing-tool\`,
\`unsupported\`, \`stale\`의 공통 상태로 완전히 분류해 UI 문구까지 동일하게 만드는
작업은 남아 있다. 다음 단계에서 language/framework capability matrix와 이
상태 분류를 실제 Project/API/Code/DB projection에 연결한다.
