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
    let client_request_links = client_request_links_for_route(snapshot, route.id.as_str());
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
            link.is_confirmed()
                && matches!(link.kind.as_str(), "code_db_read" | "code_db_write")
                && reachable_code_ids.contains(link.from.as_str())
                && item_by_id
                    .get(link.to.as_str())
                    .is_some_and(|item| item.is_db() && item.kind == "table")
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
                    .is_some_and(|item| item.is_db() && item.kind == "table")
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
    included_ids.extend(client_request_links.iter().map(|link| link.from.clone()));
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
    edges.extend(
        client_request_links
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
                weight: None,
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
        &client_request_links,
        &item_by_id,
    );

    VisualMap {
        id: format!("map:{}:{}", snapshot.workspace_id, route.id),
        workspace_id: snapshot.workspace_id.clone(),
        mode,
        focus: route.id.clone(),
        nodes,
        edges,
        overview_axis: None,
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
        representative_paths: None,
    }
}

const API_CALL_HOP_LIMIT: usize = 4;
const API_CODE_NODE_LIMIT: usize = 24;
const API_EDGE_LIMIT: usize = 32;
const API_DB_RELATION_LIMIT: usize = 8;
const API_DB_CANDIDATE_LIMIT: usize = 8;
const API_CLIENT_REQUEST_LIMIT: usize = 4;

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
        .filter(|item| item.is_project_code_item())
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
    link.is_confirmed()
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

fn client_request_links_for_route<'a>(
    snapshot: &'a InventorySnapshot,
    route_id: &str,
) -> Vec<&'a SnapshotLink> {
    let mut links = snapshot
        .links
        .iter()
        .filter(|link| {
            link.kind == "client_request"
                && link.to == route_id
                && matches!(link.truth_class.as_str(), "confirmed" | "candidate")
        })
        .collect::<Vec<_>>();
    links.sort_by(|left, right| {
        (left.truth_class != "confirmed")
            .cmp(&(right.truth_class != "confirmed"))
            .then_with(|| left.id.cmp(&right.id))
    });
    links.truncate(API_CLIENT_REQUEST_LIMIT);
    links
}
