use std::collections::{BTreeMap, HashMap, HashSet};

use super::architecture::narrow_focus_map;
use super::linker::{candidate_links, MAX_CANDIDATES_PER_CODE_ITEM};
use super::model::{
    ApiReadingAnswer, ApiReadingStep, CandidateLink, Evidence, ImpactReviewItem, InventoryItem,
    InventorySnapshot, SnapshotLink, SourceLocation, VisualEdge, VisualMap,
};
use super::projection_support::{assign_review_ranks, confidence_rank, safe_evidence, safe_text};
use super::visual_map::{confirmed_link_edge, focus_neighborhood_map, visual_node};

pub(super) fn api_flow_map(
    snapshot: &InventorySnapshot,
    focus_id: String,
    mode: String,
) -> VisualMap {
    let item_by_id: HashMap<&str, &InventoryItem> = snapshot
        .items
        .iter()
        .map(|item| (item.id.as_str(), item))
        .collect();
    let Some(route) = item_by_id.get(focus_id.as_str()).copied() else {
        return narrow_focus_map(snapshot, mode);
    };
    if route.source != "code" || route.layer != "api" {
        return focus_neighborhood_map(snapshot, Some(focus_id), mode);
    }

    let traversal = reachable_api_flow_links(
        snapshot,
        route.id.as_str(),
        API_CALL_HOP_LIMIT,
        API_CODE_NODE_LIMIT,
        API_EDGE_LIMIT,
    );
    let has_confirmed_handler = traversal
        .links
        .iter()
        .any(|link| link.kind == "code_handle");
    let reachable_code_ids = traversal
        .node_order
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    let mut db_relations = snapshot
        .links
        .iter()
        .filter(|link| {
            link.truth_class == "confirmed"
                && matches!(link.kind.as_str(), "code_db_read" | "code_db_write")
                && reachable_code_ids.contains(link.from.as_str())
                && item_by_id
                    .get(link.to.as_str())
                    .is_some_and(|item| item.source == "db" && item.kind == "table")
        })
        .collect::<Vec<_>>();
    db_relations.sort_by(|left, right| {
        left.from
            .cmp(&right.from)
            .then_with(|| left.to.cmp(&right.to))
            .then_with(|| left.id.cmp(&right.id))
    });
    let confirmed_db_targets = db_relations
        .iter()
        .map(|link| link.to.as_str())
        .collect::<HashSet<_>>();
    let hidden_db_relations = db_relations.len().saturating_sub(API_DB_RELATION_LIMIT);
    db_relations.truncate(API_DB_RELATION_LIMIT);
    let all_candidates = has_confirmed_handler.then(|| candidate_links(snapshot));
    let candidate_linker_cap_reached = all_candidates.as_ref().is_some_and(|links| {
        let mut counts = HashMap::<&str, usize>::new();
        links
            .iter()
            .filter(|link| reachable_code_ids.contains(link.from.as_str()))
            .filter(|link| !confirmed_db_targets.contains(link.to.as_str()))
            .any(|link| {
                let count = counts.entry(link.from.as_str()).or_default();
                *count += 1;
                *count == MAX_CANDIDATES_PER_CODE_ITEM
            })
    });
    let mut candidates = if let Some(all_candidates) = all_candidates {
        all_candidates
            .iter()
            .filter(|link| reachable_code_ids.contains(link.from.as_str()))
            .filter(|link| !confirmed_db_targets.contains(link.to.as_str()))
            .filter(|link| {
                item_by_id
                    .get(link.to.as_str())
                    .is_some_and(|item| item.source == "db" && item.kind == "table")
            })
            .cloned()
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    candidates.sort_by(|left, right| {
        confidence_rank(left.confidence.as_str())
            .cmp(&confidence_rank(right.confidence.as_str()))
            .then_with(|| left.from.cmp(&right.from))
            .then_with(|| left.to.cmp(&right.to))
            .then_with(|| left.id.cmp(&right.id))
    });

    let mut unique_targets = HashMap::<String, usize>::new();
    let mut merged_candidates = Vec::<CandidateLink>::new();
    for mut candidate in candidates {
        if let (Some(source), Some(target)) = (
            item_by_id.get(candidate.from.as_str()),
            item_by_id.get(candidate.to.as_str()),
        ) {
            candidate.evidence.push(Evidence {
                kind: "candidate-source".to_string(),
                text: format!(
                    "{} 코드에서 {} 테이블 후보를 찾았습니다.",
                    source.name, target.name
                ),
            });
        }
        if let Some(index) = unique_targets.get(candidate.to.as_str()).copied() {
            merged_candidates[index].evidence.extend(candidate.evidence);
        } else {
            unique_targets.insert(candidate.to.clone(), merged_candidates.len());
            merged_candidates.push(candidate);
        }
    }
    for candidate in &mut merged_candidates {
        let mut seen = HashSet::new();
        candidate
            .evidence
            .retain(|entry| seen.insert((entry.kind.clone(), entry.text.clone())));
    }
    let mut candidates = merged_candidates;
    let hidden_candidates = candidates.len().saturating_sub(API_DB_CANDIDATE_LIMIT);
    candidates.truncate(API_DB_CANDIDATE_LIMIT);

    let mut included_ids = vec![route.id.clone()];
    included_ids.extend(traversal.node_order.iter().cloned());
    included_ids.extend(db_relations.iter().map(|link| link.to.clone()));
    included_ids.extend(candidates.iter().map(|link| link.to.clone()));
    let mut seen_nodes = HashSet::new();
    let nodes = included_ids
        .into_iter()
        .filter(|id| seen_nodes.insert(id.clone()))
        .filter_map(|id| item_by_id.get(id.as_str()))
        .map(|item| visual_node(item))
        .collect::<Vec<_>>();
    let visible_ids = nodes
        .iter()
        .map(|node| node.id.as_str())
        .collect::<HashSet<_>>();
    let mut edges = traversal
        .links
        .iter()
        .copied()
        .filter(|link| {
            visible_ids.contains(link.from.as_str()) && visible_ids.contains(link.to.as_str())
        })
        .map(|link| confirmed_link_edge(link, &item_by_id))
        .collect::<Vec<_>>();
    edges.extend(
        db_relations
            .iter()
            .map(|link| confirmed_link_edge(link, &item_by_id)),
    );
    edges.extend(candidates.iter().filter_map(|link| {
        if visible_ids.contains(link.from.as_str()) && visible_ids.contains(link.to.as_str()) {
            Some(VisualEdge {
                id: link.id.clone(),
                from: link.from.clone(),
                to: link.to.clone(),
                kind: "candidate_uses".to_string(),
                confidence: Some(link.confidence.clone()),
                evidence: link.evidence.clone(),
            })
        } else {
            None
        }
    }));
    let api_reading = api_reading_answer(
        snapshot,
        route,
        &traversal,
        ApiDatabaseProjection {
            relations: &db_relations,
            candidates: &candidates,
            hidden_relations: hidden_db_relations,
            hidden_candidates,
            candidate_cap_reached: candidate_linker_cap_reached,
        },
        &item_by_id,
    );

    VisualMap {
        id: format!("map:{}:{}", snapshot.workspace_id, route.id),
        workspace_id: snapshot.workspace_id.clone(),
        mode,
        focus: route.id.clone(),
        nodes,
        edges,
        warnings: {
            let mut warnings = Vec::new();
            if !has_confirmed_handler {
                warnings.push("확정 HANDLES 없음: handler 이후 구간은 알 수 없습니다.".to_string());
            }
            if api_reading.truncated {
                warnings.push(format!(
                    "API 읽기 경로 일부를 접었습니다: {}",
                    api_reading
                        .truncation_reason
                        .as_deref()
                        .unwrap_or("표시 한도 도달")
                ));
            }
            warnings
        },
        review_board: None,
        api_reading: Some(api_reading),
    }
}

const API_CALL_HOP_LIMIT: usize = 4;
const API_CODE_NODE_LIMIT: usize = 24;
const API_EDGE_LIMIT: usize = 32;
const API_DB_RELATION_LIMIT: usize = 8;
const API_DB_CANDIDATE_LIMIT: usize = 8;

struct ApiFlowTraversal<'a> {
    links: Vec<&'a SnapshotLink>,
    node_order: Vec<String>,
    depths: HashMap<String, usize>,
    incoming: HashMap<String, &'a SnapshotLink>,
    hidden_branches: usize,
    truncation_reasons: Vec<String>,
}

struct ApiDatabaseProjection<'a> {
    relations: &'a [&'a SnapshotLink],
    candidates: &'a [CandidateLink],
    hidden_relations: usize,
    hidden_candidates: usize,
    candidate_cap_reached: bool,
}

fn reachable_api_flow_links<'a>(
    snapshot: &'a InventorySnapshot,
    route_id: &str,
    max_call_depth: usize,
    node_limit: usize,
    edge_limit: usize,
) -> ApiFlowTraversal<'a> {
    let code_ids = snapshot
        .items
        .iter()
        .filter(|item| item.source == "code")
        .map(|item| item.id.as_str())
        .collect::<HashSet<_>>();
    let mut handles = snapshot
        .links
        .iter()
        .filter(|link| trusted_api_edge(link, "code_handle", "HANDLES"))
        .filter(|link| link.from == route_id)
        .filter(|link| code_ids.contains(link.from.as_str()) && code_ids.contains(link.to.as_str()))
        .collect::<Vec<_>>();
    handles.sort_by(|left, right| left.to.cmp(&right.to).then_with(|| left.id.cmp(&right.id)));

    let mut links = Vec::new();
    let mut node_order = Vec::new();
    let mut depths = HashMap::new();
    let mut incoming = HashMap::new();
    let mut seen_edges = HashSet::new();
    let mut seen_nodes = HashSet::from([route_id.to_string()]);
    let mut hidden_edges = HashSet::new();
    let mut truncation_reasons = Vec::new();
    let mut handler_branches = HashMap::<String, usize>::new();
    let mut frontier = Vec::<(String, usize)>::new();

    for handle in handles {
        if seen_edges.contains(handle.id.as_str()) {
            continue;
        }
        let is_new_node = !seen_nodes.contains(handle.to.as_str());
        if links.len() >= edge_limit {
            record_hidden_api_edge(
                handle,
                &mut hidden_edges,
                &mut truncation_reasons,
                format!("관계 최대 {edge_limit}개에 도달했습니다."),
            );
            continue;
        }
        if is_new_node && seen_nodes.len() >= node_limit {
            record_hidden_api_edge(
                handle,
                &mut hidden_edges,
                &mut truncation_reasons,
                format!("코드 노드 최대 {node_limit}개에 도달했습니다."),
            );
            continue;
        }

        seen_edges.insert(handle.id.clone());
        links.push(handle);
        if is_new_node {
            let branch = handler_branches.len();
            handler_branches.insert(handle.to.clone(), branch);
            seen_nodes.insert(handle.to.clone());
            node_order.push(handle.to.clone());
            depths.insert(handle.to.clone(), 1);
            incoming.insert(handle.to.clone(), handle);
            frontier.push((handle.to.clone(), branch));
        }
    }

    let mut outgoing = HashMap::<&str, Vec<&SnapshotLink>>::new();
    for link in snapshot
        .links
        .iter()
        .filter(|link| trusted_api_edge(link, "code_call", "CALLS"))
        .filter(|link| code_ids.contains(link.from.as_str()) && code_ids.contains(link.to.as_str()))
    {
        outgoing.entry(link.from.as_str()).or_default().push(link);
    }
    for next_links in outgoing.values_mut() {
        next_links
            .sort_by(|left, right| left.to.cmp(&right.to).then_with(|| left.id.cmp(&right.id)));
    }

    for call_depth in 1..=max_call_depth {
        if frontier.is_empty() {
            break;
        }
        let mut by_branch = BTreeMap::<usize, Vec<&SnapshotLink>>::new();
        for (node_id, branch) in &frontier {
            if let Some(next_links) = outgoing.get(node_id.as_str()) {
                by_branch
                    .entry(*branch)
                    .or_default()
                    .extend(next_links.iter().copied());
            }
        }
        for branch_links in by_branch.values_mut() {
            branch_links.sort_by(|left, right| {
                left.from
                    .cmp(&right.from)
                    .then_with(|| left.to.cmp(&right.to))
                    .then_with(|| left.id.cmp(&right.id))
            });
            branch_links.dedup_by(|left, right| left.id == right.id);
        }

        let rounds = by_branch.values().map(Vec::len).max().unwrap_or(0);
        let mut next_frontier = Vec::new();
        for round in 0..rounds {
            for (branch, branch_links) in &by_branch {
                let Some(link) = branch_links.get(round).copied() else {
                    continue;
                };
                if seen_edges.contains(link.id.as_str()) {
                    continue;
                }
                let is_new_node = !seen_nodes.contains(link.to.as_str());
                if links.len() >= edge_limit {
                    record_hidden_api_edge(
                        link,
                        &mut hidden_edges,
                        &mut truncation_reasons,
                        format!("관계 최대 {edge_limit}개에 도달했습니다."),
                    );
                    continue;
                }
                if is_new_node && seen_nodes.len() >= node_limit {
                    record_hidden_api_edge(
                        link,
                        &mut hidden_edges,
                        &mut truncation_reasons,
                        format!("코드 노드 최대 {node_limit}개에 도달했습니다."),
                    );
                    continue;
                }

                seen_edges.insert(link.id.clone());
                links.push(link);
                if is_new_node {
                    seen_nodes.insert(link.to.clone());
                    node_order.push(link.to.clone());
                    depths.insert(link.to.clone(), call_depth + 1);
                    incoming.insert(link.to.clone(), link);
                    next_frontier.push((link.to.clone(), *branch));
                }
            }
        }
        frontier = next_frontier;
    }

    for (node_id, _) in &frontier {
        for link in outgoing.get(node_id.as_str()).into_iter().flatten() {
            if !seen_edges.contains(link.id.as_str()) {
                record_hidden_api_edge(
                    link,
                    &mut hidden_edges,
                    &mut truncation_reasons,
                    format!("CALLS 최대 {max_call_depth} hop에 도달했습니다."),
                );
            }
        }
    }

    ApiFlowTraversal {
        links,
        node_order,
        depths,
        incoming,
        hidden_branches: hidden_edges.len(),
        truncation_reasons,
    }
}

fn trusted_api_edge(link: &SnapshotLink, kind: &str, engine_edge_type: &str) -> bool {
    link.truth_class == "confirmed"
        && link.kind == kind
        && link.engine_edge_type.as_deref() == Some(engine_edge_type)
}

fn record_hidden_api_edge(
    link: &SnapshotLink,
    hidden_edges: &mut HashSet<String>,
    reasons: &mut Vec<String>,
    reason: String,
) {
    if hidden_edges.insert(link.id.clone()) && !reasons.contains(&reason) {
        reasons.push(reason);
    }
}

fn api_reading_answer(
    snapshot: &InventorySnapshot,
    route: &InventoryItem,
    traversal: &ApiFlowTraversal<'_>,
    db_projection: ApiDatabaseProjection<'_>,
    item_by_id: &HashMap<&str, &InventoryItem>,
) -> ApiReadingAnswer {
    let ApiDatabaseProjection {
        relations: db_relation_links,
        candidates,
        hidden_relations: hidden_db_relations,
        hidden_candidates,
        candidate_cap_reached: candidate_linker_cap_reached,
    } = db_projection;
    let mut steps = vec![api_reading_step(route, None, 0, 1, item_by_id)];
    for node_id in &traversal.node_order {
        let Some(item) = item_by_id.get(node_id.as_str()).copied() else {
            continue;
        };
        let incoming = traversal.incoming.get(node_id).copied();
        let depth = traversal.depths.get(node_id).copied().unwrap_or(1);
        steps.push(api_reading_step(
            item,
            incoming,
            depth,
            steps.len() + 1,
            item_by_id,
        ));
    }

    let mut db_relations = db_relation_items(db_relation_links, item_by_id);
    assign_review_ranks(&mut db_relations);
    let mut db_candidates = candidates
        .iter()
        .filter_map(|link| {
            let source = item_by_id.get(link.from.as_str()).copied()?;
            let target = item_by_id.get(link.to.as_str()).copied()?;
            Some(ImpactReviewItem {
                id: format!("api-db-candidate:{}", link.id),
                node_id: Some(target.id.clone()),
                kind: "db-candidate".to_string(),
                title: target.name.clone(),
                detail: safe_text(&format!(
                    "{} 코드에서 {} 테이블 사용 가능성을 확인해야 합니다.",
                    source.name, target.name
                )),
                truth_class: "candidate".to_string(),
                confidence: Some(link.confidence.clone()),
                rank: 0,
                evidence: safe_evidence(&link.evidence),
                location: source.location.clone(),
            })
        })
        .collect::<Vec<_>>();
    assign_review_ranks(&mut db_candidates);

    let has_handler = traversal
        .links
        .iter()
        .any(|link| link.kind == "code_handle");
    let mut unknowns = Vec::new();
    let mut reachable_sources = traversal
        .node_order
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    reachable_sources.insert(route.id.as_str());
    let mut rejected_edges = snapshot
        .links
        .iter()
        .filter_map(|link| {
            let handles = (link.kind == "code_handle"
                || link.engine_edge_type.as_deref() == Some("HANDLES"))
                && link.from == route.id
                && !trusted_api_edge(link, "code_handle", "HANDLES");
            let calls = (link.kind == "code_call"
                || link.engine_edge_type.as_deref() == Some("CALLS"))
                && reachable_sources.contains(link.from.as_str())
                && !trusted_api_edge(link, "code_call", "CALLS");
            (handles || calls).then_some((link, handles))
        })
        .collect::<Vec<_>>();
    rejected_edges.sort_by(|(left, _), (right, _)| left.id.cmp(&right.id));
    let rejected_handles = rejected_edges
        .iter()
        .filter(|(_, handles)| *handles)
        .count();
    for (link, handles) in rejected_edges {
        let relationship = if handles { "HANDLES" } else { "CALLS" };
        let title = if handles && !has_handler {
            "확정 HANDLES 없음"
        } else if handles {
            "비확정 HANDLES 제외"
        } else {
            "비확정 CALLS 제외"
        };
        let mut evidence = safe_evidence(&link.evidence);
        evidence.push(Evidence {
            kind: "excluded-engine-edge".to_string(),
            text: safe_text(&format!(
                "{} → {} 관계의 kind={}, engineEdgeType={}, truthClass={}를 확정 경로에서 제외했습니다.",
                link.from,
                link.to,
                link.kind,
                link.engine_edge_type.as_deref().unwrap_or("없음"),
                link.truth_class
            )),
        });
        unknowns.push(ImpactReviewItem {
            id: format!("api-unknown:edge:{}", link.id),
            node_id: Some(link.to.clone()),
            kind: if handles { "handler-gap" } else { "call-gap" }.to_string(),
            title: title.to_string(),
            detail: safe_text(&format!(
                "{title}: {relationship} 관계가 확정 조건을 충족하지 않아 읽기 순서와 지도에서 제외했습니다."
            )),
            truth_class: link.truth_class.clone(),
            confidence: link
                .evidence
                .iter()
                .find(|entry| entry.kind == "engine-confidence")
                .map(|entry| entry.text.clone())
                .filter(|confidence| confidence != "unknown"),
            rank: 0,
            evidence,
            location: item_by_id
                .get(link.to.as_str())
                .or_else(|| item_by_id.get(link.from.as_str()))
                .and_then(|item| item.location.clone()),
        });
    }

    if !has_handler && rejected_handles == 0 {
        unknowns.push(api_answer_item(
            "api-unknown:handler",
            Some(route.id.clone()),
            "handler-gap",
            "확정 HANDLES 없음",
            "확정 HANDLES 없음: 코드 엔진에서 이 Route의 handler 관계를 찾지 못해 이후 구간은 알 수 없습니다.",
            "unknown",
            route.location.clone(),
        ));
    } else if db_relations.is_empty() && db_candidates.is_empty() {
        unknowns.push(api_answer_item(
            "api-unknown:db",
            Some(route.id.clone()),
            "db-gap",
            "DB 사용 구간을 확인할 수 없음",
            "확정 CALLS로 도달한 코드에서 이름 기반 DB 후보를 찾지 못했습니다.",
            "unknown",
            None,
        ));
    }

    for (index, reason) in snapshot.stale_reasons.iter().enumerate() {
        unknowns.push(api_answer_item(
            &format!("api-unknown:stale:{index}"),
            Some(route.id.clone()),
            "stale",
            "Snapshot 재확인 필요",
            reason,
            "unknown",
            route.location.clone(),
        ));
    }
    if snapshot.metadata.migration.reindex_required {
        let detail = if snapshot.metadata.migration.notes.is_empty() {
            "현재 snapshot은 최신 계약으로 완전히 검증되지 않아 재인덱싱이 필요합니다.".to_string()
        } else {
            format!(
                "현재 snapshot은 재인덱싱이 필요합니다. {}",
                snapshot.metadata.migration.notes.join(" ")
            )
        };
        unknowns.push(api_answer_item(
            "api-unknown:reindex",
            Some(route.id.clone()),
            "reindex",
            "재인덱싱 필요",
            &detail,
            "unknown",
            route.location.clone(),
        ));
    }

    let mut relevant_ids = reachable_sources;
    relevant_ids.extend(db_relation_links.iter().map(|link| link.to.as_str()));
    relevant_ids.extend(candidates.iter().map(|candidate| candidate.to.as_str()));
    let relevant_gaps = snapshot
        .metadata
        .gaps
        .iter()
        .filter(|gap| {
            gap.related_ids.is_empty()
                || gap
                    .related_ids
                    .iter()
                    .any(|id| relevant_ids.contains(id.as_str()))
        })
        .collect::<Vec<_>>();
    let capability_gaps = relevant_gaps
        .iter()
        .filter(|gap| gap.kind == "db-capability")
        .collect::<Vec<_>>();
    if !capability_gaps.is_empty() {
        unknowns.push(ImpactReviewItem {
            id: "api-unknown:db-capability".to_string(),
            node_id: Some(route.id.clone()),
            kind: "db-capability".to_string(),
            title: "DB에서 확인하지 못하는 구조".to_string(),
            detail: format!(
                "현재 DB 어댑터가 수집하지 않는 구조 정보가 {}종 있습니다. 실제 스키마에 해당 객체가 있을 때 경로가 불완전할 수 있으며, 다시 읽어도 지원 범위는 바뀌지 않습니다.",
                capability_gaps.len()
            ),
            truth_class: "unknown".to_string(),
            confidence: None,
            rank: 0,
            evidence: capability_gaps
                .iter()
                .map(|gap| Evidence {
                    kind: "db-capability".to_string(),
                    text: safe_text(&gap.message),
                })
                .collect(),
            location: None,
        });
    }
    for gap in relevant_gaps
        .iter()
        .filter(|gap| gap.kind != "db-capability")
    {
        unknowns.push(api_answer_item(
            &format!("api-unknown:gap:{}", gap.id),
            Some(route.id.clone()),
            &gap.kind,
            "인덱싱 메타데이터 누락",
            &gap.message,
            "unknown",
            route.location.clone(),
        ));
    }
    if let Some(db) = snapshot.metadata.db.as_ref() {
        if db.truncated == Some(true) {
            unknowns.push(api_answer_item(
                "api-unknown:db-truncated",
                Some(route.id.clone()),
                "db-inventory-truncated",
                "DB 인벤토리 일부만 확인됨",
                "DB 인벤토리가 잘려 있어 추가 테이블 후보가 누락됐을 수 있습니다.",
                "unknown",
                None,
            ));
        }
        if db.limit_clamped == Some(true) {
            unknowns.push(api_answer_item(
                "api-unknown:db-limit-clamped",
                Some(route.id.clone()),
                "db-limit-clamped",
                "DB 인벤토리 한도 조정됨",
                &format!(
                    "요청한 DB 한도({})가 엔진 한도({})로 조정되어 전체 범위를 확인하지 못했을 수 있습니다.",
                    db.limit_requested
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "알 수 없음".to_string()),
                    db.limit_applied
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "알 수 없음".to_string())
                ),
                "unknown",
                None,
            ));
        }
    }
    if candidate_linker_cap_reached {
        unknowns.push(api_answer_item(
            "api-unknown:candidate-linker-cap",
            Some(route.id.clone()),
            "candidate-cap",
            "DB 후보 선행 한도 도달",
            &format!(
                "코드 항목당 DB 후보가 최대 {MAX_CANDIDATES_PER_CODE_ITEM}개로 제한되어 추가 테이블 후보가 누락됐을 수 있습니다."
            ),
            "unknown",
            None,
        ));
    }

    let mut truncation_reasons = traversal.truncation_reasons.clone();
    if hidden_candidates > 0 {
        truncation_reasons.push(format!(
            "DB 후보는 상위 {API_DB_CANDIDATE_LIMIT}개만 표시합니다."
        ));
    }
    if candidate_linker_cap_reached {
        truncation_reasons.push(format!(
            "DB 후보 연결은 코드 항목당 최대 {MAX_CANDIDATES_PER_CODE_ITEM}개에서 선행 제한되었습니다."
        ));
    }
    let hidden_branches = traversal.hidden_branches + hidden_candidates;
    let hidden_branches_is_lower_bound = hidden_branches > 0 || candidate_linker_cap_reached;
    if hidden_branches > 0 {
        unknowns.push(api_answer_item(
            "api-unknown:truncated",
            Some(route.id.clone()),
            "truncated",
            "읽기 경로 일부가 접힘",
            &format!(
                "최소 {hidden_branches}개의 경계 관계/후보가 표시 한도 밖에 있습니다. 경계 아래는 탐색하지 않아 실제 숨은 항목은 더 많을 수 있습니다. {}",
                truncation_reasons.join(" ")
            ),
            "unknown",
            None,
        ));
    }
    assign_review_ranks(&mut unknowns);

    let mut recommended_checks = Vec::new();
    let snapshot_coverage_risk = !snapshot.stale_reasons.is_empty()
        || snapshot.metadata.migration.reindex_required
        || relevant_gaps.iter().any(|gap| gap.kind != "db-capability")
        || snapshot
            .metadata
            .db
            .as_ref()
            .is_some_and(|db| db.truncated == Some(true) || db.limit_clamped == Some(true));
    if snapshot_coverage_risk {
        recommended_checks.push(api_answer_item(
            "api-check:reindex",
            Some(route.id.clone()),
            "reindex",
            "Snapshot 범위 확인 후 다시 인덱싱",
            "stale·migration·metadata gap 또는 DB 인벤토리 한도를 해소한 뒤 API 경로를 다시 확인하세요.",
            "action",
            route.location.clone(),
        ));
    }
    if has_handler && db_relations.is_empty() && db_candidates.is_empty() {
        let (kind, title, detail) = if snapshot.metadata.db.is_some() {
            (
                "db-source-scope",
                "연결한 DB 범위 확인",
                "확정 코드 경로에 연결되는 DB 후보가 없습니다. 연결한 DB/DDL이 이 프로젝트와 같은 환경의 구조인지 확인하세요.",
            )
        } else {
            (
                "db-source",
                "DB 구조 연결",
                "DB 구조를 연결한 뒤 같은 API 경로를 다시 확인하세요.",
            )
        };
        recommended_checks.push(api_answer_item(
            "api-check:db-source",
            Some(route.id.clone()),
            kind,
            title,
            detail,
            "action",
            route.location.clone(),
        ));
    }
    if let Some(step) = steps.iter().find(|step| step.item.location.is_some()) {
        recommended_checks.push(api_answer_item(
            "api-check:first-source",
            step.item.node_id.clone(),
            "source",
            "첫 파일부터 열기",
            &format!("{}부터 읽고 다음 확정 CALLS를 따라가세요.", step.item.title),
            "action",
            step.item.location.clone(),
        ));
    }
    if !has_handler {
        recommended_checks.push(api_answer_item(
            "api-check:handles",
            Some(route.id.clone()),
            "route-registration",
            "Route 등록과 handler 연결 확인",
            "라우트 프레임워크 등록부에서 실제 handler를 확인한 뒤 다시 인덱싱하세요.",
            "action",
            route.location.clone(),
        ));
    }
    for (index, candidate) in db_candidates.iter().take(3).enumerate() {
        recommended_checks.push(api_answer_item(
            &format!("api-check:db:{index}"),
            candidate.node_id.clone(),
            "db-candidate",
            &format!("{} 사용 여부 검증", candidate.title),
            "Repository/query의 SQL·ORM 매핑에서 테이블 사용을 직접 확인하세요.",
            "action",
            candidate.location.clone(),
        ));
    }
    if hidden_branches > 0 || candidate_linker_cap_reached {
        recommended_checks.push(api_answer_item(
            "api-check:truncated",
            Some(route.id.clone()),
            "scope",
            "접힌 분기 별도 확인",
            "표시 한도에 걸린 분기는 검색으로 대상을 좁혀 별도로 확인하세요.",
            "action",
            route.location.clone(),
        ));
    }
    assign_review_ranks(&mut recommended_checks);

    ApiReadingAnswer {
        subject: route.name.clone(),
        steps,
        db_relations,
        db_candidates,
        unknowns,
        recommended_checks,
        hidden_branches,
        hidden_branches_is_lower_bound,
        truncated: hidden_branches > 0 || candidate_linker_cap_reached || hidden_db_relations > 0,
        truncation_reason: {
            if hidden_db_relations > 0 {
                truncation_reasons.push(format!(
                    "확정 DB 연결 중 {hidden_db_relations}개를 표시 한도로 접었습니다."
                ));
            }
            (!truncation_reasons.is_empty()).then(|| truncation_reasons.join(" "))
        },
    }
}

fn db_relation_items(
    links: &[&SnapshotLink],
    item_by_id: &HashMap<&str, &InventoryItem>,
) -> Vec<ImpactReviewItem> {
    let mut items = Vec::<ImpactReviewItem>::new();
    let mut target_indexes = HashMap::<&str, usize>::new();
    for link in links {
        let Some(source) = item_by_id.get(link.from.as_str()).copied() else {
            continue;
        };
        let Some(target) = item_by_id.get(link.to.as_str()).copied() else {
            continue;
        };
        let operation = if link.kind == "code_db_read" {
            "조회"
        } else {
            "변경"
        };
        if let Some(index) = target_indexes.get(target.id.as_str()).copied() {
            items[index].evidence.extend(safe_evidence(&link.evidence));
            if !items[index].detail.contains(operation) {
                items[index].detail.push_str(&format!(" · {operation}"));
            }
            continue;
        }
        target_indexes.insert(target.id.as_str(), items.len());
        items.push(ImpactReviewItem {
            id: format!("api-db-relation:{}", target.id),
            node_id: Some(target.id.clone()),
            kind: link.kind.clone(),
            title: target.name.clone(),
            detail: safe_text(&format!(
                "{} 코드의 실행 가능한 정적 SQL이 이 테이블을 {operation}합니다.",
                source.name
            )),
            truth_class: "confirmed".to_string(),
            confidence: None,
            rank: 0,
            evidence: safe_evidence(&link.evidence),
            location: source.location.clone(),
        });
    }
    for item in &mut items {
        let mut seen = HashSet::new();
        item.evidence
            .retain(|entry| seen.insert((entry.kind.clone(), entry.text.clone())));
    }
    items
}

fn api_reading_step(
    item: &InventoryItem,
    incoming: Option<&SnapshotLink>,
    depth: usize,
    rank: usize,
    item_by_id: &HashMap<&str, &InventoryItem>,
) -> ApiReadingStep {
    let (lane, lane_basis) = api_reading_lane(item, incoming);
    let incoming_evidence = incoming
        .map(|link| confirmed_link_edge(link, item_by_id).evidence)
        .unwrap_or_default();
    let evidence = if incoming_evidence.is_empty() {
        vec![Evidence {
            kind: "engine-node".to_string(),
            text: "코드 엔진 inventory에서 Route 항목을 읽었습니다.".to_string(),
        }]
    } else {
        incoming_evidence.clone()
    };
    ApiReadingStep {
        item: ImpactReviewItem {
            id: format!("api-step:{}", item.id),
            node_id: Some(item.id.clone()),
            kind: item.kind.clone(),
            title: item.name.clone(),
            detail: api_item_detail(item),
            truth_class: if incoming.is_some() {
                "confirmed".to_string()
            } else {
                "structural".to_string()
            },
            confidence: None,
            rank,
            evidence,
            location: item.location.clone(),
        },
        depth,
        lane: lane.to_string(),
        lane_basis: lane_basis.to_string(),
        incoming_evidence,
    }
}

fn api_reading_lane(
    item: &InventoryItem,
    incoming: Option<&SnapshotLink>,
) -> (&'static str, &'static str) {
    if incoming.is_none() || item.layer == "api" {
        return ("route", "engine-node");
    }
    if incoming.is_some_and(|link| link.kind == "code_handle") {
        return ("handler", "confirmed-handles");
    }
    let identity = format!(
        "{} {} {}",
        item.kind,
        item.engine_label.as_deref().unwrap_or_default(),
        item.name
    )
    .to_ascii_lowercase();
    if identity.contains("handler") || identity.contains("controller") {
        ("handler", "name-inferred")
    } else if ["repository", "query", "mapper", "dao"]
        .iter()
        .any(|token| identity.contains(token))
    {
        ("repository-query", "name-inferred")
    } else {
        ("service-function", "name-inferred")
    }
}

fn api_item_detail(item: &InventoryItem) -> String {
    item.location
        .as_ref()
        .map(|location| match location.line {
            Some(line) => format!("{}:{line}", location.path),
            None => location.path.clone(),
        })
        .unwrap_or_else(|| "소스 위치 정보 없음".to_string())
}

fn api_answer_item(
    id: &str,
    node_id: Option<String>,
    kind: &str,
    title: &str,
    detail: &str,
    truth_class: &str,
    location: Option<SourceLocation>,
) -> ImpactReviewItem {
    ImpactReviewItem {
        id: id.to_string(),
        node_id,
        kind: kind.to_string(),
        title: title.to_string(),
        detail: safe_text(detail),
        truth_class: truth_class.to_string(),
        confidence: None,
        rank: 0,
        evidence: Vec::new(),
        location,
    }
}
