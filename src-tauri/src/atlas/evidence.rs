use std::{collections::BTreeSet, path::Path};

use crate::{
    engine::EngineRegistry,
    workspace::{self, Workspace},
};

use super::{
    apply_explicit_query_evidence, apply_explicit_query_evidence_for_code,
    apply_focused_code_evidence, record_code_search_gap, visual_map_with_change, ChangeIntent,
    InventorySnapshot,
};

pub(crate) fn enrich_integrated_snapshot_code_evidence(
    workspace: &Workspace,
    snapshot: &mut InventorySnapshot,
) {
    if snapshot.metadata.code.is_none() || snapshot.metadata.db.is_none() {
        return;
    }

    let code_ids = snapshot
        .items
        .iter()
        .filter(|item| {
            item.is_code()
                && item.kind != "file"
                && item.kind != "module"
                && (item.path.is_some() || item.location.is_some())
        })
        .map(|item| item.id.clone())
        .collect::<Vec<_>>();

    if !code_ids.is_empty() {
        apply_explicit_query_evidence_for_code(snapshot, workspace.repo_path.as_str(), &code_ids);
    }
}

pub(crate) fn enrich_composition_code_evidence(
    app_data_dir: &Path,
    workspace_id: &str,
    focus_ids: &[String],
    snapshot: &mut InventorySnapshot,
) {
    let code_ids = composition_code_evidence_source_ids(snapshot, focus_ids);
    if let Ok(workspace) = workspace::open_workspace(app_data_dir, workspace_id) {
        apply_explicit_query_evidence_for_code(snapshot, workspace.repo_path.as_str(), &code_ids);
    }
}

fn composition_code_evidence_source_ids(
    snapshot: &InventorySnapshot,
    focus_ids: &[String],
) -> Vec<String> {
    const MAX_CODE_ITEMS: usize = 32;
    const MAX_HOPS: usize = 4;

    let code_items = snapshot
        .items
        .iter()
        .filter(|item| item.is_code())
        .map(|item| item.id.as_str())
        .collect::<BTreeSet<_>>();
    let mut selected = focus_ids
        .iter()
        .filter(|id| code_items.contains(id.as_str()))
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut frontier = selected.iter().cloned().collect::<Vec<_>>();
    for _ in 0..MAX_HOPS {
        if frontier.is_empty() || selected.len() >= MAX_CODE_ITEMS {
            break;
        }
        let frontier_ids = frontier.iter().map(String::as_str).collect::<BTreeSet<_>>();
        let mut next = snapshot
            .links
            .iter()
            .filter(|link| {
                link.is_confirmed()
                    && matches!(link.kind.as_str(), "code_handle" | "code_call")
                    && frontier_ids.contains(link.from.as_str())
                    && code_items.contains(link.to.as_str())
            })
            .map(|link| link.to.clone())
            .collect::<Vec<_>>();
        next.sort();
        next.dedup();
        frontier.clear();
        for id in next {
            if selected.len() == MAX_CODE_ITEMS {
                break;
            }
            if selected.insert(id.clone()) {
                frontier.push(id);
            }
        }
    }
    selected.into_iter().collect()
}

pub(crate) fn normalized_change_intent(
    intent: Option<ChangeIntent>,
) -> Result<Option<ChangeIntent>, String> {
    let Some(mut intent) = intent else {
        return Ok(None);
    };
    if !matches!(
        intent.kind.as_str(),
        "rename" | "drop" | "type" | "nullability"
    ) {
        return Err("지원하는 변경 종류는 이름 변경, 삭제, 타입 변경, NULL 제약입니다".to_string());
    }
    intent.value = intent
        .value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    if intent
        .value
        .as_deref()
        .is_some_and(|value| value.len() > 128 || value.chars().any(char::is_control))
    {
        return Err("변경 대상 값은 제어 문자 없이 128자 이하여야 합니다".to_string());
    }
    if intent.kind == "nullability"
        && intent
            .value
            .as_deref()
            .is_some_and(|value| !matches!(value, "nullable" | "required"))
    {
        return Err("NULL 제약 값은 nullable 또는 required여야 합니다".to_string());
    }
    if intent.kind == "drop" {
        intent.value = None;
    }
    Ok(Some(intent))
}

pub(crate) fn enrich_snapshot_code_evidence(
    app_data_dir: &Path,
    registry: &EngineRegistry,
    workspace_id: &str,
    focus_id: Option<&str>,
    mode: &str,
    snapshot: &mut InventorySnapshot,
    operation_id: Option<&str>,
) {
    let Some(focus_id) = focus_id else {
        return;
    };
    if mode == "api-flow" {
        enrich_api_code_evidence(
            app_data_dir,
            registry,
            workspace_id,
            focus_id,
            snapshot,
            operation_id,
        );
        return;
    }
    let Some(focus) = snapshot
        .items
        .iter()
        .find(|item| item.id == focus_id)
        .cloned()
    else {
        return;
    };
    let (table, column) = match (mode, focus.kind.as_str()) {
        ("table-usage", "table") => (focus, None),
        ("column-impact", "column") => {
            let Some(table) = focus
                .parent_id
                .as_deref()
                .and_then(|parent_id| snapshot.items.iter().find(|item| item.id == parent_id))
                .cloned()
            else {
                record_code_search_gap(
                    snapshot,
                    focus.id.as_str(),
                    "code-search-scope-missing",
                    "상위 테이블이 없어 컬럼 코드 검색 범위를 만들 수 없습니다.",
                    Vec::new(),
                );
                return;
            };
            (table, Some(focus))
        }
        _ => return,
    };
    let evidence_target_id = column
        .as_ref()
        .map_or(table.id.as_str(), |column| column.id.as_str())
        .to_string();
    let Some((matched_files, schema_ambiguous)) = enrich_table_code_evidence(
        app_data_dir,
        registry,
        workspace_id,
        table.id.as_str(),
        evidence_target_id.as_str(),
        snapshot,
        operation_id,
    ) else {
        return;
    };
    let Some(column) = column else {
        return;
    };

    let (path_filter, omitted_files) = focused_code_path_filter(&matched_files);
    if omitted_files > 0 {
        record_code_search_gap(
            snapshot,
            column.id.as_str(),
            "code-search-scope-truncated",
            &format!(
                "테이블 검색 파일 중 {omitted_files}개를 컬럼 검색의 16개/512-byte 경로 범위에 포함하지 못했습니다."
            ),
            vec![table.id.clone()],
        );
    }
    let Some(path_filter) = path_filter else {
        record_code_search_gap(
            snapshot,
            column.id.as_str(),
            "code-search-scope-empty",
            "테이블 식별자가 확인된 파일이 없어 일반적인 컬럼명을 저장소 전체에서 검색하지 않았습니다.",
            vec![table.id.clone()],
        );
        return;
    };
    match workspace::focused_code_search_with_operation(
        app_data_dir,
        registry,
        workspace_id,
        column.name.as_str(),
        Some(path_filter.as_str()),
        32,
        operation_id,
    ) {
        Ok(search) => {
            apply_focused_code_evidence(snapshot, column.id.as_str(), &search, schema_ambiguous);
        }
        Err(_) => record_code_search_gap(
            snapshot,
            column.id.as_str(),
            "code-search-failure",
            "컬럼 식별자 코드 검색에 실패했습니다. 기본 snapshot 후보는 그대로 유지합니다.",
            vec![table.id.clone()],
        ),
    }
}

fn enrich_api_code_evidence(
    app_data_dir: &Path,
    registry: &EngineRegistry,
    workspace_id: &str,
    focus_id: &str,
    snapshot: &mut InventorySnapshot,
    operation_id: Option<&str>,
) {
    if let Ok(workspace) = workspace::open_workspace(app_data_dir, workspace_id) {
        let code_ids = api_code_evidence_source_ids(snapshot, focus_id);
        apply_explicit_query_evidence_for_code(snapshot, workspace.repo_path.as_str(), &code_ids);
    }
    for target_id in api_code_evidence_target_ids(snapshot, focus_id) {
        let _ = enrich_table_code_evidence(
            app_data_dir,
            registry,
            workspace_id,
            target_id.as_str(),
            target_id.as_str(),
            snapshot,
            operation_id,
        );
    }
}

fn api_code_evidence_source_ids(snapshot: &InventorySnapshot, focus_id: &str) -> Vec<String> {
    visual_map_with_change(
        snapshot,
        Some(focus_id.to_string()),
        "api-flow".to_string(),
        None,
    )
    .api_reading
    .map(|answer| {
        answer
            .steps
            .into_iter()
            .filter_map(|step| step.item.node_id)
            .collect()
    })
    .unwrap_or_default()
}

pub(crate) fn api_code_evidence_target_ids(
    snapshot: &InventorySnapshot,
    focus_id: &str,
) -> Vec<String> {
    let map = visual_map_with_change(
        snapshot,
        Some(focus_id.to_string()),
        "api-flow".to_string(),
        None,
    );
    let Some(answer) = map.api_reading else {
        return Vec::new();
    };
    answer
        .db_candidates
        .into_iter()
        .filter_map(|candidate| candidate.node_id)
        .filter(|target_id| {
            snapshot
                .items
                .iter()
                .any(|item| item.id == *target_id && item.is_db() && item.kind == "table")
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn enrich_table_code_evidence(
    app_data_dir: &Path,
    registry: &EngineRegistry,
    workspace_id: &str,
    table_id: &str,
    evidence_target_id: &str,
    snapshot: &mut InventorySnapshot,
    operation_id: Option<&str>,
) -> Option<(Vec<String>, bool)> {
    let table = snapshot
        .items
        .iter()
        .find(|item| item.id == table_id && item.is_db() && item.kind == "table")
        .cloned()?;
    let ambiguous_table_ids = snapshot
        .items
        .iter()
        .filter(|item| {
            item.kind == "table" && item.is_db() && item.name.eq_ignore_ascii_case(&table.name)
        })
        .map(|item| item.id.clone())
        .collect::<Vec<_>>();
    let schema_ambiguous = ambiguous_table_ids.len() > 1;
    if schema_ambiguous {
        record_code_search_gap(
            snapshot,
            evidence_target_id,
            "code-search-schema-ambiguous",
            "동일한 테이블명이 여러 스키마에 있어 텍스트 검색 후보의 신뢰도를 high로 표시하지 않습니다.",
            ambiguous_table_ids,
        );
    }

    let table_search = match workspace::focused_code_search_with_operation(
        app_data_dir,
        registry,
        workspace_id,
        table.name.as_str(),
        None,
        32,
        operation_id,
    ) {
        Ok(search) => search,
        Err(_) => {
            record_code_search_gap(
                snapshot,
                table.id.as_str(),
                "code-search-failure",
                "테이블 식별자 코드 검색에 실패했습니다. 기본 snapshot 후보는 그대로 유지합니다.",
                Vec::new(),
            );
            return None;
        }
    };
    let table_evidence =
        apply_focused_code_evidence(snapshot, table.id.as_str(), &table_search, schema_ambiguous);
    if let Ok(workspace) = workspace::open_workspace(app_data_dir, workspace_id) {
        apply_explicit_query_evidence(
            snapshot,
            table.id.as_str(),
            workspace.repo_path.as_str(),
            &table_evidence.matches,
            schema_ambiguous,
        );
    }
    Some((table_evidence.matched_files, schema_ambiguous))
}

pub(crate) fn focused_code_path_filter(paths: &[String]) -> (Option<String>, usize) {
    const MAX_FILES: usize = 16;
    const MAX_FILTER_BYTES: usize = 512;

    let paths = paths
        .iter()
        .map(|path| path.replace('\\', "/"))
        .collect::<BTreeSet<_>>();
    let total = paths.len();
    let mut escaped = Vec::new();
    for path in paths {
        if escaped.len() == MAX_FILES {
            break;
        }
        let path = escape_regex(path.as_str());
        let mut candidate_paths = escaped.clone();
        candidate_paths.push(path.clone());
        let candidate = format!("^({})$", candidate_paths.join("|"));
        if candidate.len() > MAX_FILTER_BYTES {
            continue;
        }
        escaped.push(path);
    }
    let selected = escaped.len();
    (
        (!escaped.is_empty()).then(|| format!("^({})$", escaped.join("|"))),
        total.saturating_sub(selected),
    )
}

fn escape_regex(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        if matches!(
            character,
            '\\' | '^' | '$' | '.' | '|' | '?' | '*' | '+' | '(' | ')' | '[' | ']' | '{' | '}'
        ) {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    escaped
}
