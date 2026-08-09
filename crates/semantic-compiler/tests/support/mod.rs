#![allow(dead_code)]

use codebase_fact_model::{
    analysis::ProgrammingLanguage,
    fact_graph::{FactEdgeFamily, FactNodeKind, FactRole, FactTruth},
    identity::{EvidenceId, FactEdgeId, FactNodeId, Sha256Digest, SnapshotId, WorkspaceId},
    source::RepositoryPath,
};
use codebase_semantic_compiler::{BaseSemanticDraft, CompiledBasePrompt};
use codebase_semantic_model::{
    AiProviderDescriptor, AiProviderKind, AnchorFactSummary, AreaCategory, AreaProposal,
    BaseSemanticInput, BoundaryRelationCount, BoundaryRelationSummary, EvidenceExcerpt,
    LabelSource, OutputLanguage, ProjectSemanticContext, ProjectSemanticProposal, ProposalKey,
    RegionAssignment, RegionId, RelationBundleId, ScopeReceipt, SemanticFallbackReason,
    SemanticRevisionProposal, StaticRegionKind, StaticRegionSummary, TracePathId, TracePathState,
    TracePathSummary, BASE_SEMANTIC_SCHEMA_VERSION,
};

#[derive(Clone)]
pub struct FixtureIds {
    pub order_region: RegionId,
    pub auth_region: RegionId,
    pub order_route: FactNodeId,
    pub order_controller: FactNodeId,
    pub order_service: FactNodeId,
    pub auth_guard: FactNodeId,
    pub order_route_evidence: EvidenceId,
    pub order_controller_evidence: EvidenceId,
    pub order_service_evidence: EvidenceId,
    pub auth_evidence: EvidenceId,
    pub order_trace: TracePathId,
}

fn node(name: &str) -> FactNodeId {
    FactNodeId::from_components(&["fixture", name]).unwrap()
}

fn edge(name: &str) -> FactEdgeId {
    FactEdgeId::from_components(&["fixture", name]).unwrap()
}

fn evidence(name: &str) -> EvidenceId {
    EvidenceId::from_components(&["fixture", name]).unwrap()
}

pub fn fixture_draft() -> (BaseSemanticDraft, FixtureIds) {
    let order_region = RegionId::from_components(&["fixture", "orders"]).unwrap();
    let auth_region = RegionId::from_components(&["fixture", "auth"]).unwrap();
    let repository = node("repository");
    let order_route = node("POST /orders");
    let order_controller = node("OrderController.create");
    let order_service = node("OrderService.create");
    let auth_guard = node("AuthGuard.verify");
    let order_route_evidence = evidence("order-route");
    let order_controller_evidence = evidence("order-controller");
    let order_service_evidence = evidence("order-service");
    let auth_evidence = evidence("auth-guard");
    let orders_auth_bundle =
        RelationBundleId::from_components(&["fixture", "orders-auth"]).unwrap();
    let route_controller = edge("route-controller");
    let controller_service = edge("controller-service");
    let service_auth = edge("service-auth");
    let order_trace = TracePathSummary::stable_id(
        &order_route,
        &[route_controller.clone(), controller_service.clone()],
    )
    .unwrap();

    let input = BaseSemanticInput {
        repository: ProjectSemanticContext {
            fact_id: repository.clone(),
            name: "commerce-platform".to_string(),
            languages: vec![ProgrammingLanguage::TypeScript],
            framework_fact_ids: vec![],
            root_region_ids: vec![order_region.clone(), auth_region.clone()],
        },
        regions: vec![
            StaticRegionSummary {
                region_id: order_region.clone(),
                parent_region_id: None,
                structural_label: "src/orders".to_string(),
                structural_kind: StaticRegionKind::Module,
                path_roots: vec![RepositoryPath::parse("src/orders").unwrap()],
                languages: vec![ProgrammingLanguage::TypeScript],
                file_count: 9,
                effective_loc: 1_240,
                anchor_fact_ids: vec![
                    order_route.clone(),
                    order_controller.clone(),
                    order_service.clone(),
                ],
                representative_trace_path_ids: vec![order_trace.clone()],
                inbound_bundle_ids: vec![],
                outbound_bundle_ids: vec![orders_auth_bundle.clone()],
            },
            StaticRegionSummary {
                region_id: auth_region.clone(),
                parent_region_id: None,
                structural_label: "src/auth".to_string(),
                structural_kind: StaticRegionKind::Module,
                path_roots: vec![RepositoryPath::parse("src/auth").unwrap()],
                languages: vec![ProgrammingLanguage::TypeScript],
                file_count: 5,
                effective_loc: 760,
                anchor_fact_ids: vec![auth_guard.clone()],
                representative_trace_path_ids: vec![],
                inbound_bundle_ids: vec![orders_auth_bundle.clone()],
                outbound_bundle_ids: vec![],
            },
        ],
        anchors: vec![
            AnchorFactSummary {
                fact_id: order_route.clone(),
                owner_region_id: order_region.clone(),
                kind: FactNodeKind::HttpRoute,
                name: "POST /orders".to_string(),
                qualified_name: None,
                signature: None,
                static_roles: vec![],
                evidence_ids: vec![order_route_evidence.clone()],
            },
            AnchorFactSummary {
                fact_id: order_controller.clone(),
                owner_region_id: order_region.clone(),
                kind: FactNodeKind::Method,
                name: "create".to_string(),
                qualified_name: Some("OrderController.create".to_string()),
                signature: Some("create(request: CreateOrderRequest): Promise<Order>".to_string()),
                static_roles: vec![FactRole::Handler],
                evidence_ids: vec![order_controller_evidence.clone()],
            },
            AnchorFactSummary {
                fact_id: order_service.clone(),
                owner_region_id: order_region.clone(),
                kind: FactNodeKind::Method,
                name: "create".to_string(),
                qualified_name: Some("OrderService.create".to_string()),
                signature: Some("create(input: CreateOrderInput): Promise<Order>".to_string()),
                static_roles: vec![],
                evidence_ids: vec![order_service_evidence.clone()],
            },
            AnchorFactSummary {
                fact_id: auth_guard.clone(),
                owner_region_id: auth_region.clone(),
                kind: FactNodeKind::Method,
                name: "verify".to_string(),
                qualified_name: Some("AuthGuard.verify".to_string()),
                signature: Some("verify(token: string): Session".to_string()),
                static_roles: vec![FactRole::Middleware],
                evidence_ids: vec![auth_evidence.clone()],
            },
        ],
        boundary_relations: vec![BoundaryRelationSummary {
            bundle_id: orders_auth_bundle,
            source_region_id: order_region.clone(),
            target_region_id: auth_region.clone(),
            families: vec![BoundaryRelationCount {
                family: FactEdgeFamily::Code,
                truth: FactTruth::Confirmed,
                relation_count: 1,
            }],
            representative_edge_ids: vec![service_auth],
            evidence_ids: vec![auth_evidence.clone()],
        }],
        representative_traces: vec![TracePathSummary {
            trace_path_id: order_trace.clone(),
            entry_fact_id: order_route.clone(),
            ordered_fact_ids: vec![
                order_route.clone(),
                order_controller.clone(),
                order_service.clone(),
            ],
            ordered_edge_ids: vec![route_controller, controller_service],
            state: TracePathState::Complete,
            evidence_ids: vec![
                order_route_evidence.clone(),
                order_controller_evidence.clone(),
                order_service_evidence.clone(),
            ],
        }],
        excerpts: vec![
            EvidenceExcerpt {
                evidence_id: order_controller_evidence.clone(),
                owner_region_id: order_region.clone(),
                file_fact_id: node("order-controller-file"),
                relative_path: RepositoryPath::parse("src/orders/order.controller.ts").unwrap(),
                start_line: 40,
                end_line: 45,
                content_hash: Sha256Digest::of_bytes(b"order-controller-source"),
                text: "async create(request: CreateOrderRequest) { return this.orders.create(request); }"
                    .to_string(),
            },
            EvidenceExcerpt {
                evidence_id: auth_evidence.clone(),
                owner_region_id: auth_region.clone(),
                file_fact_id: node("auth-guard-file"),
                relative_path: RepositoryPath::parse("src/auth/auth.guard.ts").unwrap(),
                start_line: 12,
                end_line: 16,
                content_hash: Sha256Digest::of_bytes(b"auth-guard-source"),
                text: "verify(token: string) { return this.sessions.verify(token); }".to_string(),
            },
        ],
        previous_revision: None,
    };

    (
        BaseSemanticDraft {
            workspace_id: WorkspaceId::parse("ws-0123456789abcdef").unwrap(),
            snapshot_id: SnapshotId::from_components(&["fixture", "snapshot"]).unwrap(),
            provider: AiProviderDescriptor {
                kind: AiProviderKind::Codex,
                model: "gpt-5.6-sol".to_string(),
                effort: codebase_semantic_model::ReasoningEffort::High,
            },
            output_language: OutputLanguage::Korean,
            scope_receipt: ScopeReceipt {
                included: 2,
                total: 2,
                truncated: false,
                reason: None,
            },
            input,
        },
        FixtureIds {
            order_region,
            auth_region,
            order_route,
            order_controller,
            order_service,
            auth_guard,
            order_route_evidence,
            order_controller_evidence,
            order_service_evidence,
            auth_evidence,
            order_trace,
        },
    )
}

pub fn valid_proposal(compiled: &CompiledBasePrompt, ids: &FixtureIds) -> SemanticRevisionProposal {
    SemanticRevisionProposal {
        schema_version: BASE_SEMANTIC_SCHEMA_VERSION,
        snapshot_id: compiled.packet.snapshot_id.clone(),
        semantic_input_digest: compiled.packet.semantic_input_digest,
        project: ProjectSemanticProposal {
            summary: "주문 처리와 사용자 인증을 제공하는 커머스 백엔드입니다.".to_string(),
            aliases: vec!["commerce-platform".to_string()],
            representative_fact_ids: vec![ids.order_route.clone(), ids.auth_guard.clone()],
            evidence_ids: vec![ids.order_route_evidence.clone(), ids.auth_evidence.clone()],
        },
        areas: vec![
            AreaProposal {
                proposal_key: ProposalKey::parse("orders").unwrap(),
                parent_proposal_key: None,
                level: 0,
                label: "주문".to_string(),
                summary: "주문 요청의 생성과 처리를 담당합니다.".to_string(),
                category: AreaCategory::Domain,
                representative_fact_ids: vec![ids.order_route.clone()],
                representative_trace_path_ids: vec![ids.order_trace.clone()],
                evidence_ids: vec![ids.order_route_evidence.clone()],
                aliases: vec!["orders".to_string()],
                label_source: LabelSource::Semantic,
                fallback_reason: None,
            },
            AreaProposal {
                proposal_key: ProposalKey::parse("create-order").unwrap(),
                parent_proposal_key: Some(ProposalKey::parse("orders").unwrap()),
                level: 1,
                label: "주문 생성".to_string(),
                summary: "주문 생성 요청을 받아 서비스 호출로 전달합니다.".to_string(),
                category: AreaCategory::Domain,
                representative_fact_ids: vec![
                    ids.order_controller.clone(),
                    ids.order_service.clone(),
                ],
                representative_trace_path_ids: vec![ids.order_trace.clone()],
                evidence_ids: vec![
                    ids.order_controller_evidence.clone(),
                    ids.order_service_evidence.clone(),
                ],
                aliases: vec!["create order".to_string()],
                label_source: LabelSource::Semantic,
                fallback_reason: None,
            },
            AreaProposal {
                proposal_key: ProposalKey::parse("authentication").unwrap(),
                parent_proposal_key: None,
                level: 0,
                label: "인증".to_string(),
                summary: "요청 토큰을 검증하고 세션을 확인합니다.".to_string(),
                category: AreaCategory::Domain,
                representative_fact_ids: vec![ids.auth_guard.clone()],
                representative_trace_path_ids: vec![],
                evidence_ids: vec![ids.auth_evidence.clone()],
                aliases: vec!["auth".to_string(), "session".to_string()],
                label_source: LabelSource::Semantic,
                fallback_reason: None,
            },
        ],
        assignments: vec![
            RegionAssignment {
                region_id: ids.order_region.clone(),
                area_proposal_key: ProposalKey::parse("create-order").unwrap(),
            },
            RegionAssignment {
                region_id: ids.auth_region.clone(),
                area_proposal_key: ProposalKey::parse("authentication").unwrap(),
            },
        ],
        unassigned_regions: vec![],
        warnings: vec![],
    }
}

pub fn structural_proposal(compiled: &CompiledBasePrompt) -> SemanticRevisionProposal {
    let areas = compiled
        .packet
        .input
        .regions
        .iter()
        .enumerate()
        .map(|(index, region)| AreaProposal {
            proposal_key: ProposalKey::parse(format!("local-{index}")).unwrap(),
            parent_proposal_key: None,
            level: 0,
            label: region.structural_label.clone(),
            summary: format!("{} 구조 단위를 그대로 표시합니다.", region.structural_label),
            category: AreaCategory::Structural,
            representative_fact_ids: vec![],
            representative_trace_path_ids: vec![],
            evidence_ids: vec![],
            aliases: vec![],
            label_source: LabelSource::Structural,
            fallback_reason: Some(SemanticFallbackReason::InsufficientSemanticSignal),
        })
        .collect::<Vec<_>>();
    let assignments = compiled
        .packet
        .input
        .regions
        .iter()
        .enumerate()
        .map(|(index, region)| RegionAssignment {
            region_id: region.region_id.clone(),
            area_proposal_key: ProposalKey::parse(format!("local-{index}")).unwrap(),
        })
        .collect();
    SemanticRevisionProposal {
        schema_version: BASE_SEMANTIC_SCHEMA_VERSION,
        snapshot_id: compiled.packet.snapshot_id.clone(),
        semantic_input_digest: compiled.packet.semantic_input_digest,
        project: ProjectSemanticProposal {
            summary: "이 로컬 구조 단위의 현재 책임을 설명합니다.".to_string(),
            aliases: vec![],
            representative_fact_ids: vec![],
            evidence_ids: vec![],
        },
        areas,
        assignments,
        unassigned_regions: vec![],
        warnings: vec![],
    }
}
