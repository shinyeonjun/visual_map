fn api_reading_answer(
    snapshot: &InventorySnapshot,
    route: &InventoryItem,
    traversal: &ApiFlowTraversal<'_>,
    db_projection: ApiDatabaseProjection<'_>,
    client_request_links: &[&SnapshotLink],
    item_by_id: &HashMap<&str, &InventoryItem>,
) -> ApiReadingAnswer {
    let ApiDatabaseProjection {
        relations: db_relation_links,
        candidates,
        hidden_relations: hidden_db_relations,
        hidden_candidates,
        candidate_cap_reached: candidate_linker_cap_reached,
    } = db_projection;
    let route_mount_evidence = traversal
        .links
        .iter()
        .filter(|link| link.from == route.id && link.kind == "code_handle")
        .flat_map(|link| link.evidence.iter())
        .filter(|evidence| evidence.kind == "route-mount")
        .cloned()
        .collect::<Vec<_>>();
    let mut steps = vec![api_reading_step(
        route,
        None,
        0,
        1,
        item_by_id,
        &route_mount_evidence,
    )];
    let mut client_requests = client_request_links
        .iter()
        .filter_map(|link| {
            let source = item_by_id.get(link.from.as_str()).copied()?;
            let request = link
                .evidence
                .iter()
                .find(|evidence| evidence.kind == "client-request")
                .map(|evidence| evidence.text.as_str())
                .unwrap_or("클라이언트 요청");
            Some(ImpactReviewItem {
                id: format!("api-client-request:{}", link.id),
                node_id: Some(source.id.clone()),
                kind: "client-request".to_string(),
                title: request.to_string(),
                detail: safe_text(&format!(
                    "{}에서 {} API를 요청합니다. 정적 연결 상태: {}.",
                    source.name,
                    route.name,
                    if link.is_confirmed() {
                        "확정"
                    } else {
                        "후보"
                    }
                )),
                truth_class: if link.is_confirmed() {
                    "confirmed".to_string()
                } else {
                    "candidate".to_string()
                },
                confidence: Some(if link.is_confirmed() {
                    "high".to_string()
                } else {
                    "medium".to_string()
                }),
                rank: 0,
                evidence: safe_evidence(&link.evidence),
                location: source.location.clone(),
            })
        })
        .collect::<Vec<_>>();
    assign_review_ranks(&mut client_requests);
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
            &[],
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

    let (method, subject) = api_route_identity(route);
    ApiReadingAnswer {
        subject,
        method,
        steps,
        client_requests,
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
