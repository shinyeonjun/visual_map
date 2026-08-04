use std::collections::{HashMap, HashSet};

use super::model::{
    Evidence, InventoryItem, InventorySnapshot, OverviewAxis, RepresentativePath, SnapshotLink,
    VisualEdge, VisualMap, VisualNode, VisualNodeCoverage, VisualNodeMetrics,
};
use super::visual_map::visual_node;

const ROLE_AXIS_THRESHOLD: f64 = 0.10;

pub(super) fn atlas_overview(snapshot: &InventorySnapshot, mode: String) -> VisualMap {
    let (groups, item_group, _) = atlas_groups(snapshot);
    let depth_analysis = confirmed_call_depths(snapshot);
    let group_depths = group_depths(&groups, &depth_analysis.item_depths);
    let axis = choose_overview_axis(snapshot, &groups, &depth_analysis);
    let representative = representative_paths(snapshot, 5);
    let hidden = groups.len().saturating_sub(40);
    let visible_groups = groups.into_iter().take(40).collect::<Vec<_>>();
    let visible_ids = visible_groups
        .iter()
        .map(|group| group.id.clone())
        .collect::<HashSet<_>>();
    let nodes = visible_groups
        .iter()
        .map(|group| atlas_group_node(group, group_depths.get(&group.id).copied()))
        .collect::<Vec<_>>();
    let (edges, hidden_edges) = atlas_group_edges(snapshot, &item_group, &visible_ids);
    let mut warnings = vec![if architecture_package_names(snapshot).is_empty() {
        format!(
            "구조 메타데이터가 없어 원본 항목 {}개를 경로·이름 기반 보조 그룹 {}개로 축약했습니다",
            snapshot.items.len(),
            nodes.len()
        )
    } else {
        format!(
            "원본 항목 {}개를 코드 엔진 패키지와 DB 스키마 기준 구조 영역 {}개로 축약했습니다",
            snapshot.items.len(),
            nodes.len()
        )
    }];
    let omitted_code_symbols = snapshot
        .items
        .iter()
        .filter(|item| item.is_code() && item.layer == "code")
        .filter(|item| !architecture_member(item))
        .count();
    if omitted_code_symbols > 0 {
        warnings.push(format!(
            "필드·변수·데코레이터 등 하위 코드 심벌 {omitted_code_symbols}개는 구조 순위에서 제외하고 코드 검색에 보존했습니다"
        ));
    }
    if hidden > 0 {
        warnings.push(format!(
            "구조 영역 +{hidden}개는 중요도 순위 밖이라 접었습니다"
        ));
    }
    if hidden_edges > 0 {
        warnings.push(format!(
            "구조 영역 간 관계 +{hidden_edges}개는 우선순위 밖이라 접었습니다"
        ));
    }
    if representative.is_empty() {
        warnings.push("확정 진입점이 없어 대표 경로를 산출하지 못했습니다".to_string());
    }
    VisualMap {
        id: format!("map:{}:atlas", snapshot.workspace_id),
        workspace_id: snapshot.workspace_id.clone(),
        mode,
        focus: "overview".to_string(),
        nodes,
        edges,
        overview_axis: Some(axis),
        warnings,
        review_board: None,
        api_reading: None,
        representative_paths: Some(representative),
    }
}

fn choose_overview_axis(
    snapshot: &InventorySnapshot,
    groups: &[AtlasGroup],
    depth: &DepthAnalysis,
) -> OverviewAxis {
    let total_members = groups
        .iter()
        .map(|group| group.member_ids.len())
        .sum::<usize>();
    let role_members = groups
        .iter()
        .map(|group| group.handler_count + group.service_count + group.repository_count)
        .sum::<usize>();
    let role_types = [
        groups.iter().any(|group| group.handler_count > 0),
        groups.iter().any(|group| group.service_count > 0),
        groups.iter().any(|group| group.repository_count > 0),
    ]
    .into_iter()
    .filter(|present| *present)
    .count();
    let role_ratio = if total_members == 0 {
        0.0
    } else {
        role_members as f64 / total_members as f64
    };
    if role_ratio >= ROLE_AXIS_THRESHOLD && role_types >= 2 {
        return OverviewAxis {
            kind: "role".to_string(),
            lanes: ["entry", "handler", "service", "repository", "other", "data"]
                .into_iter()
                .map(str::to_string)
                .collect(),
            reason: format!(
                "핸들러·서비스·저장소 역할이 {}%의 구조 항목에서 확인되어 역할 기준으로 배치했습니다",
                (role_ratio * 100.0).round()
            ),
        };
    }

    let non_column_items = snapshot
        .items
        .iter()
        .filter(|item| item.kind != "column")
        .count();
    let roots_over_half = non_column_items > 0 && depth.root_count * 2 > non_column_items;
    if !depth.item_depths.is_empty() && !roots_over_half {
        return OverviewAxis {
            kind: "depth".to_string(),
            lanes: ["depth-0", "depth-1", "depth-2", "depth-3-plus", "data"]
                .into_iter()
                .map(str::to_string)
                .collect(),
            reason: "역할 이름 근거가 부족해 진입점에서의 확정 호출 거리 기준으로 배치했습니다"
                .to_string(),
        };
    }

    OverviewAxis {
        kind: "depth".to_string(),
        lanes: ["api", "code", "db"].into_iter().map(str::to_string).collect(),
        reason: if roots_over_half {
            "진입점 후보가 전체 항목의 절반을 넘어 깊이 축을 과다 생성하므로 기본 3계층으로 표시했습니다"
                .to_string()
        } else {
            "역할 근거와 확정 호출 진입점이 부족해 기본 3계층으로 표시했습니다".to_string()
        },
    }
}

pub(super) fn atlas_group_detail(
    snapshot: &InventorySnapshot,
    group_id: &str,
    mode: String,
) -> VisualMap {
    let (groups, _, item_evidence) = atlas_groups(snapshot);
    let Some(group) = groups.iter().find(|group| group.id == group_id) else {
        let mut map = atlas_overview(snapshot, mode);
        map.warnings
            .push("선택한 구조 영역을 찾지 못해 전체 구조를 표시합니다".to_string());
        return map;
    };

    let item_by_id = snapshot
        .items
        .iter()
        .map(|item| (item.id.as_str(), item))
        .collect::<HashMap<_, _>>();
    let members = select_atlas_detail_members(&group.member_ids, &item_by_id, 35);
    let hidden = group.member_ids.len().saturating_sub(members.len());
    let visible_member_ids = members
        .iter()
        .map(|item| item.id.as_str())
        .collect::<HashSet<_>>();
    let mut nodes = Vec::with_capacity(members.len() + groups.len());
    nodes.push(atlas_group_node(group, None));
    nodes.extend(members.iter().map(|item| visual_node(item)));
    // Siblings travel with the detail so the reader keeps their bearings when a
    // group opens. Expanding one area used to blank the rest of the map, which
    // turned an in-place expansion into a screen change.
    nodes.extend(
        groups
            .iter()
            .filter(|sibling| sibling.id != group.id)
            .take(mode_node_cap(&mode))
            .map(|sibling| atlas_group_node(sibling, None)),
    );

    let mut edges = members
        .iter()
        .map(|item| VisualEdge {
            id: format!("group-contains:{group_id}->{}", item.id),
            from: group_id.to_string(),
            to: item.id.clone(),
            kind: "group_contains".to_string(),
            confidence: None,
            evidence: item_evidence
                .get(item.id.as_str())
                .map(|text| {
                    vec![Evidence {
                        kind: "group-evidence".to_string(),
                        text: text.clone(),
                    }]
                })
                .unwrap_or_default(),
            weight: None,
        })
        .collect::<Vec<_>>();
    let (member_edges, hidden_edges) =
        atlas_member_edges(snapshot, &item_by_id, &visible_member_ids);
    edges.extend(member_edges);
    edges.sort_by(|left, right| left.id.cmp(&right.id));

    let mut warnings = vec![format!(
        "{} 구조 영역 · API {} → 코드 {} → DB {} 순서로 표시합니다",
        group.title, group.api_count, group.code_count, group.db_count
    )];
    if hidden > 0 {
        warnings.push(format!(
            "구조 영역 항목 +{hidden}개는 상세 화면에서 접었습니다"
        ));
    }
    if hidden_edges > 0 {
        warnings.push(format!(
            "구조 영역 관계 +{hidden_edges}개는 우선순위 밖이라 접었습니다"
        ));
    }
    VisualMap {
        id: format!("map:{}:atlas:{group_id}", snapshot.workspace_id),
        workspace_id: snapshot.workspace_id.clone(),
        mode,
        focus: group_id.to_string(),
        nodes,
        edges,
        overview_axis: None,
        warnings,
        review_board: None,
        api_reading: None,
        representative_paths: None,
    }
}

pub(super) fn narrow_focus_map(snapshot: &InventorySnapshot, mode: String) -> VisualMap {
    VisualMap {
        id: format!("map:{}:narrow-focus:{mode}", snapshot.workspace_id),
        workspace_id: snapshot.workspace_id.clone(),
        mode,
        focus: "narrow-focus".to_string(),
        nodes: Vec::new(),
        edges: Vec::new(),
        overview_axis: None,
        warnings: vec![
            "결과가 너무 넓습니다. 왼쪽 목록에서 API/테이블 대상을 선택하거나 검색어를 좁히세요."
                .to_string(),
        ],
        review_board: None,
        api_reading: None,
        representative_paths: None,
    }
}

pub(super) fn mode_node_cap(mode: &str) -> usize {
    match mode {
        "atlas" | "explore" => 40,
        "api-flow" | "search-focus" => 32,
        "table-usage" | "column-impact" => 36,
        _ => 30,
    }
}
