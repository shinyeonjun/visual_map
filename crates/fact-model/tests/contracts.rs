use codebase_fact_model::analysis::{
    AnalysisUnit, ContextDimension, ContextDimensionKind, ProgrammingLanguage,
    ProviderConfigArtifact, ProviderConfigUse, ProviderDescriptor, ProviderExecutionContext,
    ProviderExecutionMode, ProviderOrigin, ProviderProtocol, SemanticContext, SemanticContextKind,
};
use codebase_fact_model::analysis_plan::{AnalysisPlan, FileAnalysisAssignment};
use codebase_fact_model::coverage::{
    AnalysisCapability, AnalysisGap, AnalysisScope, AnalysisUnitCompletion, AnalysisUnitReceipt,
    AnalysisUnitState, CapabilityExecutionState, CapabilityReceipt, CoverageDenominator,
    DeclaredSupport, EvidencePrecision, FileCoverageRecord, FileCoverageState, GapCode,
    SourceScopeCoverageRecord, SourceScopeState,
};
use codebase_fact_model::evidence::{
    ArtifactLocation, EvidenceKind, EvidenceLocation, EvidenceProducer, EvidenceProducerKind,
    FactEvidence,
};
use codebase_fact_model::fact_graph::{
    DispatchKind, ExecutionControlContext, ExecutionOccurrence, FactBundleManifest, FactEdge,
    FactEdgeFamily, FactEdgeKind, FactNode, FactNodeDetails, FactNodeFamily, FactNodeKind,
    FactRole, FactRoleAssignment, FactTruth, ResolutionMethod, Visibility,
};
use codebase_fact_model::identity::{
    AnalysisUnitId, EvidenceId, FactEdgeId, FactNodeId, SemanticContextId, Sha256Digest,
    SnapshotId, WorkspaceId,
};
use codebase_fact_model::language_ir::{
    IrDefinition, IrEndpoint, IrRelation, LanguageIrHeader, LanguageIrRecord,
    LanguageIrStreamValidator, LanguageRelationKind,
};
use codebase_fact_model::source::{
    RepositoryPath, SourceFileKind, SourceFlags, SourcePosition, SourceSpan,
};
use codebase_fact_model::source_manifest::{
    SourceEncoding, SourceEntryState, SourceLinkState, SourceManifest, SourceManifestFile,
};
use codebase_fact_model::validation::{ContractErrorCode, Validate};
use codebase_fact_model::ContractSchema;

#[test]
fn stable_ids_are_domain_separated_and_length_delimited() {
    let first = FactNodeId::from_components(&["ab", "c"]).unwrap();
    let repeated = FactNodeId::from_components(&["ab", "c"]).unwrap();
    let different_partition = FactNodeId::from_components(&["a", "bc"]).unwrap();
    let edge = FactEdgeId::from_components(&["ab", "c"]).unwrap();

    assert_eq!(first, repeated);
    assert_ne!(first, different_partition);
    assert_ne!(first.as_str(), edge.as_str());
    assert!(first.as_str().starts_with("node-"));
    assert_eq!(first.as_str().len(), "node-".len() + 64);
    assert!(FactNodeId::parse(first.as_str().to_uppercase()).is_err());
}

#[test]
fn provider_native_symbols_are_normalized_without_changing_safe_existing_ids() {
    use codebase_fact_model::identity::ProviderSymbolId;

    let safe = "scip-typescript npm fixture 1.0.0 src/main.ts/Service#run().";
    assert_eq!(
        ProviderSymbolId::from_provider_native(safe)
            .unwrap()
            .as_str(),
        safe
    );

    let multiline = "scip-typescript npm fixture 1.0.0 src/main.ts/Component().(`{\r\n  value,\r\n}`)typeLiteral1:value.";
    let normalized = ProviderSymbolId::from_provider_native(multiline).unwrap();
    assert!(!normalized.as_str().chars().any(char::is_control));
    assert_eq!(
        normalized,
        ProviderSymbolId::from_provider_native(multiline).unwrap()
    );
    assert!(ProviderSymbolId::parse(normalized.as_str()).is_ok());
}

#[test]
fn provider_native_symbol_normalization_is_collision_resistant_and_prefix_safe() {
    use codebase_fact_model::identity::ProviderSymbolId;

    let newline = ProviderSymbolId::from_provider_native("provider\nsymbol").unwrap();
    let tab = ProviderSymbolId::from_provider_native("provider\tsymbol").unwrap();
    assert_ne!(newline, tab);

    // A provider is free to emit text that resembles our derived form. It must
    // not be able to alias an identity that the boundary generated.
    let reserved_literal = newline.as_str().to_string();
    let normalized_literal = ProviderSymbolId::from_provider_native(&reserved_literal).unwrap();
    assert_ne!(newline, normalized_literal);
}

#[test]
fn provider_native_symbol_normalization_bounds_oversized_external_ids() {
    use codebase_fact_model::identity::ProviderSymbolId;

    let oversized = "x".repeat(20_000);
    let normalized = ProviderSymbolId::from_provider_native(&oversized).unwrap();
    assert!(normalized.as_str().len() < 256);
    assert!(ProviderSymbolId::parse(normalized.as_str()).is_ok());
}

#[test]
fn execution_occurrences_extend_edge_identity_without_breaking_legacy_ids() {
    let (mut edge, _) = fixture_fact_edge();
    let context = edge.semantic_context_id.as_ref().unwrap();
    let legacy_components = [
        format!("source:{}", edge.source_id.as_str()),
        format!("target:{}", edge.target_id.as_str()),
        format!("kind:{}", edge.kind.as_str()),
        "semantic_context".to_string(),
        "present".to_string(),
        context.as_str().to_string(),
        "qualifier".to_string(),
        "absent".to_string(),
    ];
    let legacy_refs = legacy_components
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();
    let legacy_id = FactEdgeId::from_components(&legacy_refs).unwrap();

    edge.execution = None;
    edge.id = FactEdge::stable_id(
        &edge.source_id,
        &edge.target_id,
        edge.kind,
        edge.semantic_context_id.as_ref(),
        edge.qualifier.as_deref(),
        None,
    )
    .unwrap();

    assert_eq!(edge.id, legacy_id);
    edge.validate().unwrap();
}

#[test]
fn provider_execution_context_is_canonical_and_deterministic() {
    let artifact = ProviderConfigArtifact {
        path: RepositoryPath::parse("tsconfig.json").unwrap(),
        content_digest: Sha256Digest::of_bytes(b"{}"),
        usage: ProviderConfigUse::ExplicitArgument,
    };
    let source_scope = Sha256Digest::of_bytes(b"src/main.ts");
    let first = ProviderExecutionContext::executed(
        ProviderExecutionMode::Project,
        RepositoryPath::root(),
        source_scope,
        1,
        vec![artifact.clone()],
        None,
        vec![ContextDimension {
            kind: ContextDimensionKind::Target,
            value: "es2022".to_string(),
        }],
        vec![ContextDimensionKind::ModuleMode],
    )
    .unwrap();
    let repeated = ProviderExecutionContext::executed(
        ProviderExecutionMode::Project,
        RepositoryPath::root(),
        source_scope,
        1,
        vec![artifact],
        None,
        vec![ContextDimension {
            kind: ContextDimensionKind::Target,
            value: "es2022".to_string(),
        }],
        vec![ContextDimensionKind::ModuleMode],
    )
    .unwrap();

    assert_eq!(first, repeated);
    assert_eq!(first.fingerprint, repeated.fingerprint);
    first.validate().unwrap();
}

#[test]
fn provider_execution_context_never_promotes_unknown_to_known() {
    let result = ProviderExecutionContext::executed(
        ProviderExecutionMode::Project,
        RepositoryPath::root(),
        Sha256Digest::of_bytes(b"scope"),
        1,
        Vec::new(),
        None,
        vec![ContextDimension {
            kind: ContextDimensionKind::Platform,
            value: "linux".to_string(),
        }],
        vec![ContextDimensionKind::Platform],
    );
    assert!(result.is_err());

    let not_executed = ProviderExecutionContext::not_executed(vec![
        ContextDimensionKind::Platform,
        ContextDimensionKind::Architecture,
    ])
    .unwrap();
    assert_eq!(not_executed.mode, ProviderExecutionMode::NotExecuted);
    assert!(not_executed.analysis_root.is_none());
    assert_eq!(not_executed.missing_dimensions.len(), 2);
}

#[test]
fn composite_provider_context_preserves_mixed_dimension_knowledge() {
    let context = ProviderExecutionContext::executed(
        ProviderExecutionMode::Composite,
        RepositoryPath::root(),
        Sha256Digest::of_bytes(b"two-shard-scope"),
        2,
        Vec::new(),
        Some(Sha256Digest::of_bytes(b"sorted-child-contexts")),
        vec![ContextDimension {
            kind: ContextDimensionKind::Platform,
            value: "linux".to_string(),
        }],
        vec![ContextDimensionKind::Platform],
    )
    .unwrap();

    assert_eq!(context.mode.as_str(), "composite");
    assert_eq!(context.dimensions.len(), 1);
    assert_eq!(
        context.missing_dimensions,
        vec![ContextDimensionKind::Platform]
    );
    context.validate().unwrap();
}

#[test]
fn digests_paths_and_ranges_fail_closed() {
    let digest = Sha256Digest::of_bytes(b"same source");
    assert_eq!(digest, digest.to_string().parse().unwrap());
    assert!(Sha256Digest::parse(&digest.to_string().to_uppercase()).is_err());

    for invalid in [
        "",
        "../escape.rs",
        "/absolute.rs",
        "C:/drive.rs",
        "a\\b.rs",
        "src/file.rs:alternate-stream",
        "a//b.rs",
    ] {
        assert!(
            RepositoryPath::parse(invalid).is_err(),
            "{invalid} was accepted"
        );
    }

    let invalid_span = SourceSpan {
        path: RepositoryPath::parse("src/lib.rs").unwrap(),
        content_digest: digest,
        start: position(2, 5, 20),
        end: position(1, 1, 10),
    };
    assert_eq!(
        invalid_span.validate().unwrap_err().code,
        ContractErrorCode::InvalidSourceRange
    );

    let root_artifact = ArtifactLocation {
        path: RepositoryPath::root(),
        content_digest: Sha256Digest::of_bytes(b"artifact"),
        pointer: None,
    };
    assert_eq!(
        root_artifact.validate().unwrap_err().code,
        ContractErrorCode::InvalidRepositoryPath
    );

    let mut root_config = fixture_unit().context;
    root_config.config_files = vec![RepositoryPath::root()];
    assert_eq!(
        root_config.validate().unwrap_err().code,
        ContractErrorCode::InvalidRepositoryPath
    );
}

#[test]
fn coverage_distinguishes_measured_empty_from_unmeasured() {
    let unit_id = fixture_unit().id;
    let measured_empty = CapabilityReceipt {
        unit_id: unit_id.clone(),
        capability: AnalysisCapability::DirectCalls,
        declared_support: DeclaredSupport::Required,
        execution_state: CapabilityExecutionState::Complete,
        precision: EvidencePrecision::ExactRange,
        denominator: CoverageDenominator::Known { eligible_count: 0 },
        covered_count: 0,
        emitted_fact_count: 0,
        emitted_relation_count: 0,
        truncated_count: 0,
        gap_codes: vec![],
    };
    measured_empty.validate().unwrap();

    let unmeasured = CapabilityReceipt {
        unit_id,
        capability: AnalysisCapability::DirectCalls,
        declared_support: DeclaredSupport::Conditional,
        execution_state: CapabilityExecutionState::NotRun,
        precision: EvidencePrecision::None,
        denominator: CoverageDenominator::Unknown,
        covered_count: 0,
        emitted_fact_count: 0,
        emitted_relation_count: 0,
        truncated_count: 0,
        gap_codes: vec![GapCode::MissingProjectMetadata],
    };
    unmeasured.validate().unwrap();

    let mut dishonest = unmeasured.clone();
    dishonest.gap_codes.clear();
    assert_eq!(
        dishonest.validate().unwrap_err().code,
        ContractErrorCode::InvalidReceipt
    );

    let mut invented_precision = unmeasured.clone();
    invented_precision.precision = EvidencePrecision::ExactRange;
    assert_eq!(
        invented_precision.validate().unwrap_err().code,
        ContractErrorCode::InvalidReceipt
    );

    let mut unmeasured_partial = unmeasured.clone();
    unmeasured_partial.execution_state = CapabilityExecutionState::Partial;
    assert_eq!(
        unmeasured_partial.validate().unwrap_err().code,
        ContractErrorCode::InvalidReceipt
    );

    let mut truncated_complete = measured_empty;
    truncated_complete.truncated_count = 1;
    assert_eq!(
        truncated_complete.validate().unwrap_err().code,
        ContractErrorCode::InvalidReceipt
    );

    let encoded = serde_json::to_value(CoverageDenominator::Known { eligible_count: 7 }).unwrap();
    assert_eq!(encoded["denominatorType"], "known");
    assert_eq!(encoded["eligibleCount"], 7);

    let mut evidence_ids = vec![
        EvidenceId::from_components(&["first gap evidence"]).unwrap(),
        EvidenceId::from_components(&["second gap evidence"]).unwrap(),
    ];
    evidence_ids.sort();
    let gap = AnalysisGap {
        code: GapCode::UnresolvedTarget,
        scope: AnalysisScope::AnalysisUnit {
            unit_id: fixture_unit().id,
        },
        capability: Some(AnalysisCapability::TypeRelations),
        evidence_ids,
        message: "The provider could not resolve an explicit target".to_string(),
    };
    gap.validate().unwrap();

    let mut unordered_gap = gap;
    unordered_gap.evidence_ids.reverse();
    assert_eq!(
        unordered_gap.validate().unwrap_err().code,
        ContractErrorCode::NonCanonicalValue
    );
}

#[test]
fn census_keeps_unenumerated_scopes_without_assigning_fake_units() {
    let scope = SourceScopeCoverageRecord {
        path: RepositoryPath::parse("node_modules").unwrap(),
        state: SourceScopeState::Excluded,
        descendants_enumerated: false,
        gap_codes: vec![
            GapCode::ExcludedByRule,
            GapCode::DependencyScopeNotEnumerated,
        ],
    };
    scope.validate().unwrap();

    let excluded_file = FileCoverageRecord {
        unit_id: None,
        path: RepositoryPath::parse("vendor/generated.js").unwrap(),
        language: Some(ProgrammingLanguage::JavaScript),
        file_kind: SourceFileKind::Vendor,
        state: FileCoverageState::Excluded,
        byte_size: 0,
        line_count: None,
        non_blank_line_count: None,
        content_digest: None,
        gap_codes: vec![GapCode::ExcludedByRule],
    };
    excluded_file.validate().unwrap();

    let mut records = fixture_ir_records();
    records.insert(1, LanguageIrRecord::File(excluded_file));
    let mut validator = LanguageIrStreamValidator::default();
    let error = records
        .iter()
        .find_map(|record| validator.push(record).err())
        .expect("a unit stream must reject a file that has no analysis unit");
    assert_eq!(error.code, ContractErrorCode::StreamOrder);

    let workspace_gap = AnalysisGap {
        code: GapCode::MissingProjectMetadata,
        scope: AnalysisScope::Workspace,
        capability: Some(AnalysisCapability::ProjectStructure),
        evidence_ids: vec![],
        message: "Workspace metadata was not measured by this unit".to_string(),
    };
    let mut records = fixture_ir_records();
    records.insert(1, LanguageIrRecord::Gap(workspace_gap));
    let mut validator = LanguageIrStreamValidator::default();
    let error = records
        .iter()
        .find_map(|record| validator.push(record).err())
        .expect("a unit stream must reject a workspace-scoped gap");
    assert_eq!(error.code, ContractErrorCode::StreamOrder);
}

#[test]
fn source_manifest_is_deterministic_and_cannot_hide_unenumerated_files() {
    let workspace_id = WorkspaceId::parse("ws-0123456789abcdef").unwrap();
    let source = SourceManifestFile {
        path: RepositoryPath::parse("src/lib.rs").unwrap(),
        languages: vec![ProgrammingLanguage::Rust],
        file_kind: SourceFileKind::Source,
        state: SourceEntryState::Included,
        byte_size: 13,
        line_count: Some(1),
        non_blank_line_count: Some(1),
        content_digest: Some(Sha256Digest::of_bytes(b"pub fn run()")),
        encoding: SourceEncoding::Utf8,
        link_state: SourceLinkState::Regular,
        gap_codes: vec![],
    };
    let ignored_scope = SourceScopeCoverageRecord {
        path: RepositoryPath::parse("target").unwrap(),
        state: SourceScopeState::Excluded,
        descendants_enumerated: false,
        gap_codes: vec![GapCode::ProductIgnored, GapCode::ExcludedByRule],
    };
    let first = SourceManifest::new(
        workspace_id.clone(),
        vec![source.clone()],
        vec![ignored_scope.clone()],
    )
    .unwrap();
    let repeated = SourceManifest::new(workspace_id, vec![source], vec![ignored_scope]).unwrap();
    assert_eq!(first.manifest_digest, repeated.manifest_digest);
    first.validate().unwrap();
    let encoded = serde_json::to_value(&first).unwrap();
    assert_eq!(encoded["schema"], "codebase-workspace.source-manifest.v1");
    assert_eq!(
        serde_json::from_value::<SourceManifest>(encoded).unwrap(),
        first
    );

    let mut tampered = first.clone();
    tampered.files[0].byte_size += 1;
    assert_eq!(
        tampered.validate().unwrap_err().code,
        ContractErrorCode::InvalidDigest
    );

    let hidden_file = SourceManifestFile {
        path: RepositoryPath::parse("target/generated.rs").unwrap(),
        languages: vec![ProgrammingLanguage::Rust],
        file_kind: SourceFileKind::Generated,
        state: SourceEntryState::Excluded,
        byte_size: 0,
        line_count: None,
        non_blank_line_count: None,
        content_digest: None,
        encoding: SourceEncoding::NotRead,
        link_state: SourceLinkState::Regular,
        gap_codes: vec![GapCode::ProductIgnored],
    };
    let error =
        SourceManifest::new(first.workspace_id, vec![hidden_file], first.scopes).unwrap_err();
    assert_eq!(error.code, ContractErrorCode::InvalidReceipt);
}

#[test]
fn analysis_plan_owns_every_included_language_candidate_exactly() {
    let workspace_id = WorkspaceId::parse("ws-0123456789abcdef").unwrap();
    let manifest = SourceManifest::new(
        workspace_id.clone(),
        vec![SourceManifestFile {
            path: RepositoryPath::parse("src/auth.ts").unwrap(),
            languages: vec![ProgrammingLanguage::TypeScript],
            file_kind: SourceFileKind::Source,
            state: SourceEntryState::Included,
            byte_size: 24,
            line_count: Some(1),
            non_blank_line_count: Some(1),
            content_digest: Some(Sha256Digest::of_bytes(b"export class Auth {}")),
            encoding: SourceEncoding::Utf8,
            link_state: SourceLinkState::Regular,
            gap_codes: vec![],
        }],
        vec![],
    )
    .unwrap();
    let unit = fixture_unit();
    let plan = AnalysisPlan::new(
        workspace_id.clone(),
        manifest.manifest_digest,
        vec![unit.clone()],
        vec![FileAnalysisAssignment {
            path: RepositoryPath::parse("src/auth.ts").unwrap(),
            language: ProgrammingLanguage::TypeScript,
            unit_ids: vec![unit.id.clone()],
        }],
        vec![],
    )
    .unwrap();
    plan.validate_against(&manifest).unwrap();
    let encoded = serde_json::to_value(&plan).unwrap();
    assert_eq!(encoded["schema"], "codebase-workspace.analysis-plan.v1");
    assert_eq!(
        serde_json::from_value::<AnalysisPlan>(encoded).unwrap(),
        plan
    );

    let duplicate_gap = AnalysisGap {
        code: GapCode::MissingProjectMetadata,
        scope: AnalysisScope::AnalysisUnit {
            unit_id: unit.id.clone(),
        },
        capability: Some(AnalysisCapability::ProjectStructure),
        evidence_ids: vec![],
        message: "No compiler project metadata was available".to_string(),
    };
    let duplicate_error = AnalysisPlan::new(
        workspace_id.clone(),
        manifest.manifest_digest,
        vec![unit.clone()],
        vec![FileAnalysisAssignment {
            path: RepositoryPath::parse("src/auth.ts").unwrap(),
            language: ProgrammingLanguage::TypeScript,
            unit_ids: vec![unit.id],
        }],
        vec![duplicate_gap.clone(), duplicate_gap],
    )
    .unwrap_err();
    assert_eq!(duplicate_error.code, ContractErrorCode::DuplicateValue);

    let empty = AnalysisPlan::new(
        workspace_id,
        manifest.manifest_digest,
        vec![],
        vec![],
        vec![],
    )
    .unwrap();
    assert_eq!(
        empty.validate_against(&manifest).unwrap_err().code,
        ContractErrorCode::InvalidReceipt
    );
}

#[test]
fn language_ir_stream_enforces_order_unit_and_declared_counts() {
    let records = fixture_ir_records();
    let mut validator = LanguageIrStreamValidator::default();
    for record in &records {
        validator.push(record).unwrap();
    }
    validator.finish().unwrap();

    let mut missing_header = LanguageIrStreamValidator::default();
    assert_eq!(
        missing_header.push(&records[1]).unwrap_err().code,
        ContractErrorCode::StreamOrder
    );

    let mut wrong_count_records = records.clone();
    let LanguageIrRecord::Complete(completion) = wrong_count_records.last_mut().unwrap() else {
        panic!("fixture must end in a completion receipt");
    };
    completion.relation_count += 1;
    let mut wrong_count = LanguageIrStreamValidator::default();
    let error = wrong_count_records
        .iter()
        .find_map(|record| wrong_count.push(record).err())
        .expect("wrong count must fail");
    assert_eq!(error.code, ContractErrorCode::StreamOrder);

    let mut completed = LanguageIrStreamValidator::default();
    for record in &records {
        completed.push(record).unwrap();
    }
    assert_eq!(
        completed.push(&records[1]).unwrap_err().code,
        ContractErrorCode::StreamOrder
    );

    let mut wrong_language_records = records.clone();
    let LanguageIrRecord::File(file) = &mut wrong_language_records[1] else {
        panic!("fixture must contain a file receipt");
    };
    file.language = Some(ProgrammingLanguage::JavaScript);
    let mut wrong_language = LanguageIrStreamValidator::default();
    let error = wrong_language_records
        .iter()
        .find_map(|record| wrong_language.push(record).err())
        .expect("file language mismatch must fail");
    assert_eq!(error.code, ContractErrorCode::StreamOrder);

    let mut wrong_context_records = records.clone();
    let LanguageIrRecord::Relation(relation) = &mut wrong_context_records[4] else {
        panic!("fixture must contain a relation");
    };
    relation.semantic_context_id = SemanticContextId::from_components(&["wrong-context"]).unwrap();
    let mut wrong_context = LanguageIrStreamValidator::default();
    let error = wrong_context_records
        .iter()
        .find_map(|record| wrong_context.push(record).err())
        .expect("relation context mismatch must fail");
    assert_eq!(error.code, ContractErrorCode::StreamOrder);

    let mut duplicate_capability_records = records.clone();
    duplicate_capability_records.insert(6, duplicate_capability_records[5].clone());
    let LanguageIrRecord::Complete(completion) = duplicate_capability_records.last_mut().unwrap()
    else {
        panic!("fixture must end in completion");
    };
    completion.capability_receipt_count += 1;
    let mut duplicate_capability = LanguageIrStreamValidator::default();
    let error = duplicate_capability_records
        .iter()
        .find_map(|record| duplicate_capability.push(record).err())
        .expect("duplicate capability receipt must fail");
    assert_eq!(error.code, ContractErrorCode::StreamOrder);

    let LanguageIrRecord::Header(header) = &records[0] else {
        panic!("fixture must start with a header");
    };
    let LanguageIrRecord::Complete(completion) = records.last().unwrap() else {
        panic!("fixture must end with completion");
    };
    let receipt = AnalysisUnitReceipt {
        unit: header.unit.clone(),
        provider: header.provider.clone(),
        completion: completion.clone(),
    };
    receipt.validate().unwrap();

    let mut mismatched_receipt = receipt;
    mismatched_receipt.completion.unit_id = AnalysisUnitId::from_components(&["other"]).unwrap();
    assert_eq!(
        mismatched_receipt.validate().unwrap_err().code,
        ContractErrorCode::InvalidReceipt
    );
}

#[test]
fn not_executed_language_ir_is_valid_but_cannot_claim_provider_semantics() {
    let records = fixture_ir_records();
    let LanguageIrRecord::Header(mut header) = records[0].clone() else {
        panic!("fixture must start with a header");
    };
    header.execution_context = ProviderExecutionContext::not_executed(vec![
        ContextDimensionKind::ModuleMode,
        ContextDimensionKind::Target,
    ])
    .unwrap();
    header.validate().unwrap();

    let completion = AnalysisUnitCompletion {
        unit_id: header.unit.id.clone(),
        state: AnalysisUnitState::Failed,
        file_record_count: 0,
        definition_count: 0,
        relation_count: 0,
        evidence_count: 0,
        capability_receipt_count: 0,
        gap_count: 0,
        issue_count: 0,
    };
    let mut valid_gap_stream = LanguageIrStreamValidator::default();
    valid_gap_stream
        .push(&LanguageIrRecord::Header(header.clone()))
        .unwrap();
    valid_gap_stream
        .push(&LanguageIrRecord::Complete(completion))
        .unwrap();
    valid_gap_stream.finish().unwrap();

    let mut invented_semantics = LanguageIrStreamValidator::default();
    invented_semantics
        .push(&LanguageIrRecord::Header(header.clone()))
        .unwrap();
    assert_eq!(
        invented_semantics.push(&records[2]).unwrap_err().code,
        ContractErrorCode::StreamOrder
    );

    let mut invented_provider_relation = LanguageIrStreamValidator::default();
    invented_provider_relation
        .push(&LanguageIrRecord::Header(header.clone()))
        .unwrap();
    assert_eq!(
        invented_provider_relation
            .push(&records[4])
            .unwrap_err()
            .code,
        ContractErrorCode::StreamOrder
    );
}

#[test]
fn language_ir_structure_endpoints_are_exact_and_cannot_hide_arbitrary_files() {
    let unit = fixture_unit();
    let package = IrEndpoint::Structure {
        unit_id: unit.id.clone(),
        kind: FactNodeKind::Package,
        qualified_name: "example.com/shop/orders".to_string(),
    };
    package.validate().unwrap();

    let namespace = IrEndpoint::Structure {
        unit_id: unit.id.clone(),
        kind: FactNodeKind::Namespace,
        qualified_name: "Shop.Orders".to_string(),
    };
    namespace.validate().unwrap();

    let error = IrEndpoint::Structure {
        unit_id: unit.id,
        kind: FactNodeKind::Class,
        qualified_name: "Shop.Orders.Order".to_string(),
    }
    .validate()
    .unwrap_err();
    assert_eq!(error.code, ContractErrorCode::NonCanonicalValue);
}

#[test]
fn unknown_fields_and_enum_variants_fail_deserialization() {
    assert!(serde_json::from_str::<FactTruth>("\"unknown\"").is_err());
    assert!(serde_json::from_str::<IrEndpoint>(
        r#"{"endpointType":"file","path":"src/lib.rs","unexpected":true}"#
    )
    .is_err());
    assert!(serde_json::from_str::<EvidenceLocation>(
        r#"{"locationType":"source","span":{"path":"src/lib.rs","contentDigest":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","start":{"line":0,"utf8Column":0,"byteOffset":0},"end":{"line":0,"utf8Column":1,"byteOffset":1}},"unexpected":true}"#
    )
    .is_err());
    assert!(serde_json::from_str::<CoverageDenominator>(
        r#"{"denominatorType":"known","eligibleCount":1,"unexpected":true}"#
    )
    .is_err());

    let LanguageIrRecord::Header(header) = &fixture_ir_records()[0] else {
        panic!("fixture must start with a header");
    };
    let mut value = serde_json::to_value(header).unwrap();
    value
        .as_object_mut()
        .unwrap()
        .insert("unexpected".to_string(), serde_json::json!(true));
    assert!(serde_json::from_value::<LanguageIrHeader>(value).is_err());
}

#[test]
fn language_ids_serialize_to_the_ten_contract_names() {
    let languages = [
        ProgrammingLanguage::TypeScript,
        ProgrammingLanguage::JavaScript,
        ProgrammingLanguage::Python,
        ProgrammingLanguage::Java,
        ProgrammingLanguage::CSharp,
        ProgrammingLanguage::C,
        ProgrammingLanguage::Cpp,
        ProgrammingLanguage::Go,
        ProgrammingLanguage::Rust,
        ProgrammingLanguage::Dart,
    ];
    assert_eq!(
        serde_json::to_value(languages).unwrap(),
        serde_json::json!([
            "typescript",
            "javascript",
            "python",
            "java",
            "csharp",
            "c",
            "cpp",
            "go",
            "rust",
            "dart"
        ])
    );
}

#[test]
fn canonical_nodes_and_edges_require_consistent_families_and_evidence() {
    let (edge, evidence_id) = fixture_fact_edge();
    edge.validate().unwrap();

    let mut wrong_id = edge.clone();
    wrong_id.id = FactEdgeId::from_components(&["wrong"]).unwrap();
    assert_eq!(
        wrong_id.validate().unwrap_err().code,
        ContractErrorCode::InvalidIdentifier
    );

    let mut missing_evidence = edge.clone();
    missing_evidence.evidence_ids.clear();
    assert_eq!(
        missing_evidence.validate().unwrap_err().code,
        ContractErrorCode::MissingEvidence
    );

    let mut wrong_family = edge;
    wrong_family.family = FactEdgeFamily::Data;
    assert_eq!(
        wrong_family.validate().unwrap_err().code,
        ContractErrorCode::NonCanonicalValue
    );

    let unit = fixture_unit();
    let node = FactNode {
        id: FactNode::stable_id(
            FactNodeKind::Class,
            Some(ProgrammingLanguage::TypeScript),
            Some(&unit.id),
            "src/auth.AuthService",
            None,
        )
        .unwrap(),
        snapshot_id: fixture_snapshot_id(),
        family: FactNodeFamily::Symbol,
        kind: FactNodeKind::Class,
        native_kind: Some("Class".to_string()),
        qualified_name: "src/auth.AuthService".to_string(),
        display_name: "AuthService".to_string(),
        signature: None,
        details: None,
        visibility: Visibility::Public,
        language: Some(ProgrammingLanguage::TypeScript),
        analysis_unit_id: Some(unit.id),
        parent_id: None,
        definition_evidence_id: Some(evidence_id.clone()),
        evidence_ids: vec![evidence_id.clone()],
        roles: vec![FactRoleAssignment {
            role: FactRole::Service,
            evidence_ids: vec![evidence_id],
        }],
        flags: SourceFlags::default(),
    };
    node.validate().unwrap();

    let mut renamed_display = node.clone();
    renamed_display.display_name = "로그인 서비스".to_string();
    renamed_display.validate().unwrap();

    let mut changed_identity = node.clone();
    changed_identity.qualified_name = "src/auth.RenamedService".to_string();
    assert_eq!(
        changed_identity.validate().unwrap_err().code,
        ContractErrorCode::InvalidIdentifier
    );

    let mut duplicated_role = node;
    duplicated_role.roles.push(duplicated_role.roles[0].clone());
    assert_eq!(
        duplicated_role.validate().unwrap_err().code,
        ContractErrorCode::DuplicateValue
    );
}

#[test]
fn http_route_nodes_require_typed_normalized_identity_details() {
    let unit = fixture_unit();
    let evidence_id = fixture_evidence().id;
    let route = FactNode {
        id: FactNode::stable_id(
            FactNodeKind::HttpRoute,
            Some(ProgrammingLanguage::TypeScript),
            Some(&unit.id),
            "GET /users/:id",
            None,
        )
        .unwrap(),
        snapshot_id: fixture_snapshot_id(),
        family: FactNodeFamily::Interface,
        kind: FactNodeKind::HttpRoute,
        native_kind: Some("express:http_route".to_string()),
        qualified_name: "GET /users/:id".to_string(),
        display_name: "GET /users/:id".to_string(),
        signature: None,
        details: Some(FactNodeDetails::HttpRoute {
            method: "GET".to_string(),
            path: "/users/:id".to_string(),
        }),
        visibility: Visibility::Public,
        language: Some(ProgrammingLanguage::TypeScript),
        analysis_unit_id: Some(unit.id),
        parent_id: None,
        definition_evidence_id: Some(evidence_id.clone()),
        evidence_ids: vec![evidence_id],
        roles: Vec::new(),
        flags: SourceFlags::default(),
    };
    route.validate().unwrap();

    let mut missing_details = route.clone();
    missing_details.details = None;
    assert_eq!(
        missing_details.validate().unwrap_err().code,
        ContractErrorCode::NonCanonicalValue
    );

    let mut lowercase_method = route.clone();
    lowercase_method.details = Some(FactNodeDetails::HttpRoute {
        method: "get".to_string(),
        path: "/users/:id".to_string(),
    });
    assert_eq!(
        lowercase_method.validate().unwrap_err().code,
        ContractErrorCode::NonCanonicalValue
    );

    let mut mismatched_identity = route;
    mismatched_identity.details = Some(FactNodeDetails::HttpRoute {
        method: "POST".to_string(),
        path: "/users/:id".to_string(),
    });
    assert_eq!(
        mismatched_identity.validate().unwrap_err().code,
        ContractErrorCode::NonCanonicalValue
    );
}

#[test]
fn language_ir_header_and_fact_edge_match_golden_json() {
    let header = &fixture_ir_records()[0];
    let header_json = format!("{}\n", serde_json::to_string_pretty(header).unwrap());
    assert_eq!(header_json, include_str!("golden/language-ir-header.json"));

    let (edge, _) = fixture_fact_edge();
    let edge_json = format!("{}\n", serde_json::to_string_pretty(&edge).unwrap());
    assert_eq!(edge_json, include_str!("golden/fact-edge.json"));
}

#[test]
fn bundle_manifest_ties_snapshot_to_semantic_inputs_not_timestamp() {
    let workspace_id = WorkspaceId::parse("ws-0123456789abcdef").unwrap();
    let source_manifest_digest = Sha256Digest::of_bytes(b"source manifest");
    let config_digest = Sha256Digest::of_bytes(b"config");
    let analysis_plan_digest = Sha256Digest::of_bytes(b"analysis plan");
    let provider_set_digest = Sha256Digest::of_bytes(b"provider set");
    let execution_context_set_digest = Sha256Digest::of_bytes(b"execution contexts");
    let snapshot_id = SnapshotId::from_execution_inputs(
        &workspace_id,
        source_manifest_digest,
        analysis_plan_digest,
        provider_set_digest,
        execution_context_set_digest,
    )
    .unwrap();
    let manifest = FactBundleManifest {
        schema: ContractSchema::CanonicalFactV1,
        snapshot_id,
        workspace_id,
        source_manifest_digest,
        config_digest,
        analysis_plan_digest,
        provider_set_digest,
        execution_context_set_digest,
        semantic_digest: Sha256Digest::of_bytes(b"canonical semantic rows"),
        bundle_digest: Sha256Digest::of_bytes(b"complete sqlite bundle"),
        analysis_unit_receipt_count: 1,
        node_count: 0,
        edge_count: 0,
        evidence_count: 0,
        file_coverage_count: 1,
        source_scope_coverage_count: 0,
        capability_receipt_count: 1,
        gap_count: 0,
        issue_count: 0,
        completed_at_unix_ms: 1_000,
    };
    manifest.validate().unwrap();

    let mut later_timestamp = manifest.clone();
    later_timestamp.completed_at_unix_ms += 1;
    later_timestamp.validate().unwrap();
    assert_eq!(manifest.snapshot_id, later_timestamp.snapshot_id);

    let mut wrong_schema = manifest.clone();
    wrong_schema.schema = ContractSchema::LanguageIrV1;
    assert_eq!(
        wrong_schema.validate().unwrap_err().code,
        ContractErrorCode::InvalidSchema
    );

    let mut wrong_snapshot = manifest;
    wrong_snapshot.snapshot_id = SnapshotId::from_components(&["wrong"]).unwrap();
    assert_eq!(
        wrong_snapshot.validate().unwrap_err().code,
        ContractErrorCode::InvalidIdentifier
    );
}

fn fixture_ir_records() -> Vec<LanguageIrRecord> {
    let unit = fixture_unit();
    let snapshot_id = fixture_snapshot_id();
    let source_digest = Sha256Digest::of_bytes(b"source manifest");
    let evidence = fixture_evidence();
    let evidence_id = evidence.id.clone();
    let source_symbol = codebase_fact_model::identity::ProviderSymbolId::parse(
        "scip src/auth.ts/AuthService#login().",
    )
    .unwrap();
    let target_symbol = codebase_fact_model::identity::ProviderSymbolId::parse(
        "scip src/session.ts/SessionStore#save().",
    )
    .unwrap();
    let header = LanguageIrHeader {
        schema: ContractSchema::LanguageIrV2,
        snapshot_id,
        source_manifest_digest: source_digest,
        unit: unit.clone(),
        provider: ProviderDescriptor {
            name: "scip-typescript".to_string(),
            version: Some("1.0.0".to_string()),
            protocol: ProviderProtocol::Scip,
            origin: ProviderOrigin::ManagedBundle,
            artifact_digest: Sha256Digest::of_bytes(b"provider"),
        },
        execution_context: ProviderExecutionContext::executed(
            ProviderExecutionMode::Project,
            unit.root.clone(),
            Sha256Digest::of_bytes(b"provider source scope"),
            1,
            vec![ProviderConfigArtifact {
                path: RepositoryPath::parse("tsconfig.json").unwrap(),
                content_digest: Sha256Digest::of_bytes(b"tsconfig canonical fields"),
                usage: ProviderConfigUse::ExplicitArgument,
            }],
            None,
            vec![ContextDimension {
                kind: ContextDimensionKind::ModuleMode,
                value: "node16".to_string(),
            }],
            vec![ContextDimensionKind::Target],
        )
        .unwrap(),
    };
    let file = FileCoverageRecord {
        unit_id: Some(unit.id.clone()),
        path: RepositoryPath::parse("src/auth.ts").unwrap(),
        language: Some(ProgrammingLanguage::TypeScript),
        file_kind: SourceFileKind::Source,
        state: FileCoverageState::Indexed,
        byte_size: 128,
        line_count: Some(1),
        non_blank_line_count: Some(1),
        content_digest: Some(Sha256Digest::of_bytes(b"export class AuthService {}")),
        gap_codes: vec![],
    };
    let definition = IrDefinition {
        unit_id: unit.id.clone(),
        symbol_id: source_symbol.clone(),
        native_kind: "Class".to_string(),
        canonical_kind_hint: FactNodeKind::Class,
        qualified_name: "src/auth.AuthService".to_string(),
        display_name: "AuthService".to_string(),
        signature: None,
        visibility: Visibility::Public,
        parent_symbol_id: None,
        definition_evidence_id: evidence_id.clone(),
        flags: SourceFlags::default(),
    };
    let relation = IrRelation {
        unit_id: unit.id.clone(),
        source: IrEndpoint::NativeSymbol {
            symbol_id: source_symbol,
        },
        target: IrEndpoint::NativeSymbol {
            symbol_id: target_symbol,
        },
        kind: LanguageRelationKind::Calls,
        truth: FactTruth::Confirmed,
        resolution: ResolutionMethod::Provider,
        dispatch: DispatchKind::Direct,
        semantic_context_id: unit.context.id.clone(),
        execution: Some(ExecutionOccurrence {
            call_site_evidence_id: evidence_id.clone(),
            lexical_ordinal: 0,
            control: ExecutionControlContext::default(),
        }),
        evidence_ids: vec![evidence_id],
    };
    let capability = CapabilityReceipt {
        unit_id: unit.id.clone(),
        capability: AnalysisCapability::DirectCalls,
        declared_support: DeclaredSupport::Required,
        execution_state: CapabilityExecutionState::Complete,
        precision: EvidencePrecision::ExactRange,
        denominator: CoverageDenominator::Known { eligible_count: 1 },
        covered_count: 1,
        emitted_fact_count: 1,
        emitted_relation_count: 1,
        truncated_count: 0,
        gap_codes: vec![],
    };
    let completion = AnalysisUnitCompletion {
        unit_id: unit.id,
        state: AnalysisUnitState::Complete,
        file_record_count: 1,
        definition_count: 1,
        relation_count: 1,
        evidence_count: 1,
        capability_receipt_count: 1,
        gap_count: 0,
        issue_count: 0,
    };
    vec![
        LanguageIrRecord::Header(Box::new(header)),
        LanguageIrRecord::File(file),
        LanguageIrRecord::Evidence(evidence),
        LanguageIrRecord::Definition(definition),
        LanguageIrRecord::Relation(relation),
        LanguageIrRecord::CapabilityReceipt(capability),
        LanguageIrRecord::Complete(completion),
    ]
}

fn fixture_unit() -> AnalysisUnit {
    let context = SemanticContext::new(
        SemanticContextKind::CompilerProject,
        Sha256Digest::of_bytes(b"tsconfig canonical fields"),
        vec![RepositoryPath::parse("tsconfig.json").unwrap()],
        vec![ContextDimension {
            kind: ContextDimensionKind::ModuleMode,
            value: "node16".to_string(),
        }],
    )
    .unwrap();
    AnalysisUnit::new(
        WorkspaceId::parse("ws-0123456789abcdef").unwrap(),
        ProgrammingLanguage::TypeScript,
        RepositoryPath::root(),
        context,
        1,
    )
    .unwrap()
}

fn fixture_snapshot_id() -> SnapshotId {
    SnapshotId::from_analysis_inputs(
        &WorkspaceId::parse("ws-0123456789abcdef").unwrap(),
        Sha256Digest::of_bytes(b"source manifest"),
        Sha256Digest::of_bytes(b"config"),
        Sha256Digest::of_bytes(b"provider set"),
    )
    .unwrap()
}

fn fixture_evidence() -> FactEvidence {
    let digest = Sha256Digest::of_bytes(b"export class AuthService {}");
    FactEvidence::new(
        EvidenceKind::CallSite,
        EvidenceProducer {
            kind: EvidenceProducerKind::Scip,
            name: "scip-typescript".to_string(),
            version: Some("1.0.0".to_string()),
            strategy: Some("provider-occurrence".to_string()),
        },
        EvidenceLocation::Source {
            span: SourceSpan {
                path: RepositoryPath::parse("src/auth.ts").unwrap(),
                content_digest: digest,
                start: position(0, 0, 0),
                end: position(0, 6, 6),
            },
        },
        Some("resolved call occurrence".to_string()),
    )
    .unwrap()
}

fn fixture_fact_edge() -> (FactEdge, EvidenceId) {
    let unit = fixture_unit();
    let source = FactNode::stable_id(
        FactNodeKind::Method,
        Some(ProgrammingLanguage::TypeScript),
        Some(&unit.id),
        "src/auth.AuthService.login",
        None,
    )
    .unwrap();
    let target = FactNode::stable_id(
        FactNodeKind::Method,
        Some(ProgrammingLanguage::TypeScript),
        Some(&unit.id),
        "src/session.SessionStore.save",
        None,
    )
    .unwrap();
    let evidence_id = fixture_evidence().id;
    let execution = ExecutionOccurrence {
        call_site_evidence_id: evidence_id.clone(),
        lexical_ordinal: 0,
        control: ExecutionControlContext::default(),
    };
    let id = FactEdge::stable_id(
        &source,
        &target,
        FactEdgeKind::Calls,
        Some(&unit.context.id),
        None,
        Some(&execution),
    )
    .unwrap();
    (
        FactEdge {
            id,
            snapshot_id: fixture_snapshot_id(),
            source_id: source,
            target_id: target,
            family: FactEdgeFamily::Code,
            kind: FactEdgeKind::Calls,
            truth: FactTruth::Confirmed,
            resolution: ResolutionMethod::Provider,
            dispatch: DispatchKind::Direct,
            semantic_context_id: Some(unit.context.id),
            qualifier: None,
            execution: Some(execution),
            evidence_ids: vec![evidence_id.clone()],
        },
        evidence_id,
    )
}

fn position(line: u32, utf8_column: u32, byte_offset: u64) -> SourcePosition {
    SourcePosition {
        line,
        utf8_column,
        byte_offset,
    }
}

/// Every execution state the helper can be handed must produce a receipt the
/// contract accepts.
///
/// The two fields are bound in both directions — a run that completed has to
/// say how it measured, one that never ran may not claim it did — and the rule
/// used to be restated at each receipt site. One site enumerated only
/// `NotApplicable` and left `Failed` claiming an exact range; another
/// hard-coded a measured precision beside a variable state. Both were latent
/// until a capability reached the state they missed, and the contract then
/// aborted the analysis it was supposed to describe.
#[test]
fn derived_precision_satisfies_the_receipt_contract_for_every_execution_state() {
    let states = [
        CapabilityExecutionState::Complete,
        CapabilityExecutionState::Partial,
        CapabilityExecutionState::Failed,
        CapabilityExecutionState::NotRun,
        CapabilityExecutionState::NotApplicable,
    ];
    for state in states {
        let precision = EvidencePrecision::for_execution(state, EvidencePrecision::ExactRange);
        let receipt = CapabilityReceipt {
            unit_id: AnalysisUnitId::from_components(&["unit"]).unwrap(),
            capability: AnalysisCapability::DirectCalls,
            declared_support: DeclaredSupport::Conditional,
            execution_state: state,
            precision,
            denominator: CoverageDenominator::Unknown,
            covered_count: 0,
            emitted_fact_count: 0,
            emitted_relation_count: 0,
            truncated_count: 0,
            gap_codes: vec![GapCode::DynamicDispatch],
        };
        receipt
            .validate()
            .unwrap_or_else(|error| panic!("{state:?} produced an invalid receipt: {error}"));
    }
}

/// A state that did not run may not borrow the precision of one that did.
#[test]
fn derived_precision_refuses_to_claim_measurement_a_run_never_made() {
    for state in [
        CapabilityExecutionState::Failed,
        CapabilityExecutionState::NotRun,
        CapabilityExecutionState::NotApplicable,
    ] {
        assert_eq!(
            EvidencePrecision::for_execution(state, EvidencePrecision::ExactRange),
            EvidencePrecision::None,
            "{state:?} must not claim a precision"
        );
    }
    assert_eq!(
        EvidencePrecision::for_execution(
            CapabilityExecutionState::Partial,
            EvidencePrecision::Symbol
        ),
        EvidencePrecision::Symbol,
    );
}
