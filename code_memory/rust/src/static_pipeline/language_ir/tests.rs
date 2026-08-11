use super::capabilities::{capability_policies, AdapterMeasurement};
use super::direct::execution_contexts_by_unit;
use super::source_coordinates::SourceCoordinates;
use super::{
    emit_direct_language_ir, reconcile_provider_execution_contexts, DirectLanguageIrInput,
};
use crate::frameworks::{Analysis as FrameworkAnalysis, FrameworkFact, FrameworkOutput};
use crate::{
    executed_provider_context, normalize_scip_language, not_executed_provider_context, Diagnostic,
    DiagnosticCode, DocumentOutput, ExecutedProviderContextInput, FileCoverageOutput,
    LanguageOutput, OccurrenceOutput, ProviderKind, ProviderUnitBatch, RelationOutput,
    SymbolOutput, LANGUAGES,
};
use codebase_fact_model::analysis::{
    ContextDimension, ContextDimensionKind, ProgrammingLanguage, ProviderConfigUse,
    ProviderExecutionContext, ProviderExecutionMode, ProviderProtocol,
};
use codebase_fact_model::coverage::{
    AnalysisCapability, AnalysisScope, CapabilityExecutionState, CapabilityReceipt,
    DeclaredSupport, GapCode,
};
use codebase_fact_model::fact_graph::{
    DispatchKind, FactEdge, FactEdgeKind, FactNode, FactNodeDetails, FactNodeKind, FactRole,
};
use codebase_fact_model::identity::{ProviderSymbolId, Sha256Digest};
use codebase_fact_model::language_ir::{IrEndpoint, LanguageIrRecord};
use codebase_fact_model::source::RepositoryPath;
use codebase_fact_model::source_manifest::SourceManifestFile;
use codebase_fact_model::validation::Validate;
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::static_pipeline::analysis_unit_planner::plan_analysis_units;
use crate::static_pipeline::canonical::{normalize_language_ir, CanonicalLanguageInput};
use crate::static_pipeline::framework_ir::{adapt_framework_routes, framework_analyzer_set_digest};
use crate::static_pipeline::source_census::SourceCensus;

struct TestEnvironmentOverride {
    key: &'static str,
    previous: Option<OsString>,
}

impl TestEnvironmentOverride {
    fn set(key: &'static str, value: &str) -> Self {
        let previous = std::env::var_os(key);
        std::env::set_var(key, value);
        Self { key, previous }
    }

    fn update(&self, value: &str) {
        std::env::set_var(self.key, value);
    }
}

impl Drop for TestEnvironmentOverride {
    fn drop(&mut self) {
        match self.previous.take() {
            Some(value) => std::env::set_var(self.key, value),
            None => std::env::remove_var(self.key),
        }
    }
}

#[test]
fn all_ten_languages_emit_valid_deterministic_unit_streams() {
    let fixture = DonorFixture::all_languages();
    let first = fixture.emit().unwrap();
    let repeated = fixture.emit().unwrap();
    let first_receipt = &first.receipt;
    let repeated_receipt = &repeated.receipt;

    assert_eq!(first_receipt.emitted_unit_count, 10);
    assert_eq!(first_receipt.unavailable_unit_count, 0);
    assert_eq!(first_receipt.file_record_count, 10);
    assert_eq!(first_receipt.definition_count, 22);
    assert_eq!(first_receipt.relation_count, 10);
    assert_eq!(first_receipt.capability_receipt_count, 10 * 9);
    assert_eq!(first_receipt.omitted_definition_count, 0);
    assert_eq!(first_receipt.omitted_relation_count, 0);
    assert_eq!(first_receipt.snapshot_id, repeated_receipt.snapshot_id);
    assert_eq!(
        first_receipt.provider_set_digest,
        repeated_receipt.provider_set_digest
    );
    assert_eq!(
        first_receipt.stream_set_digest,
        repeated_receipt.stream_set_digest
    );
    assert_eq!(first_receipt.record_count, repeated_receipt.record_count);
    assert_eq!(
        first.artifact.content_digest,
        repeated.artifact.content_digest
    );
}

#[test]
fn all_ten_languages_normalize_opaque_provider_symbols_without_losing_joins() {
    let mut fixture = DonorFixture::all_languages();
    let replacements = fixture
        .documents
        .iter()
        .flat_map(|document| document.symbols.iter())
        .map(|symbol| {
            (
                symbol.symbol.clone(),
                format!("{}\r\nprovider-native-fragment", symbol.symbol),
            )
        })
        .collect::<BTreeMap<_, _>>();

    for document in &mut fixture.documents {
        for symbol in &mut document.symbols {
            symbol.symbol = replacements[&symbol.symbol].clone();
            if let Some(parent) = &mut symbol.enclosing_symbol {
                *parent = replacements[parent].clone();
            }
        }
        for occurrence in &mut document.occurrences {
            occurrence.symbol = replacements[&occurrence.symbol].clone();
        }
    }
    for relation in &mut fixture.relations {
        relation.from = replacements[&relation.from].clone();
        relation.to = replacements[&relation.to].clone();
    }

    let first = fixture.emit().unwrap();
    let repeated = fixture.emit().unwrap();
    assert_eq!(first.receipt.emitted_unit_count, 10);
    assert_eq!(first.receipt.definition_count, 22);
    assert_eq!(first.receipt.relation_count, 10);
    assert_eq!(first.receipt.omitted_definition_count, 0);
    assert_eq!(first.receipt.omitted_relation_count, 0);
    assert_eq!(
        first.receipt.stream_set_digest,
        repeated.receipt.stream_set_digest
    );

    let records = read_ir_records(&first.artifact.path);
    let definition_ids = records
        .iter()
        .filter_map(|record| match record {
            LanguageIrRecord::Definition(definition) => {
                assert!(!definition.symbol_id.as_str().chars().any(char::is_control));
                assert!(!definition.qualified_name.chars().any(char::is_control));
                if let Some(parent) = &definition.parent_symbol_id {
                    assert!(!parent.as_str().chars().any(char::is_control));
                }
                Some(definition.symbol_id.clone())
            }
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    for relation in records.iter().filter_map(|record| match record {
        LanguageIrRecord::Relation(relation) => Some(relation),
        _ => None,
    }) {
        for endpoint in [&relation.source, &relation.target] {
            if let IrEndpoint::NativeSymbol { symbol_id } = endpoint {
                assert!(definition_ids.contains(symbol_id));
            }
        }
    }
}

#[test]
fn multiline_scip_type_literal_identity_keeps_definition_and_relation() {
    let mut fixture = DonorFixture::one_language(ProgrammingLanguage::TypeScript);
    let document = fixture.documents.first_mut().unwrap();
    let original = document.symbols[0].symbol.clone();
    let multiline = "scip-typescript npm fixture 1.0.0 src/main.ts/Component().(`{\r\n  value,\r\n}`)typeLiteral1:value.".to_string();
    document.symbols[0].symbol = multiline.clone();
    document.symbols[0].display_name = None;
    for occurrence in &mut document.occurrences {
        if occurrence.symbol == original {
            occurrence.symbol = multiline.clone();
        }
    }
    for relation in &mut fixture.relations {
        if relation.from == original {
            relation.from = multiline.clone();
        }
        if relation.to == original {
            relation.to = multiline.clone();
        }
    }

    let emission = fixture.emit().unwrap();
    assert_eq!(emission.receipt.definition_count, 2);
    assert_eq!(emission.receipt.relation_count, 1);
    assert_eq!(emission.receipt.omitted_definition_count, 0);
    assert_eq!(emission.receipt.omitted_relation_count, 0);

    let records = read_ir_records(&emission.artifact.path);
    let expected_symbol = ProviderSymbolId::from_provider_native(&multiline).unwrap();
    let definition = records
        .iter()
        .find_map(|record| match record {
            LanguageIrRecord::Definition(definition) if definition.symbol_id == expected_symbol => {
                Some(definition)
            }
            _ => None,
        })
        .expect("multiline SCIP type literal definition");
    assert!(!definition.symbol_id.as_str().chars().any(char::is_control));
    assert!(!definition.display_name.chars().any(char::is_control));
    assert_eq!(definition.qualified_name, definition.symbol_id.as_str());
}

#[test]
fn empty_provider_symbol_is_scoped_omission_not_project_failure() {
    let mut fixture = DonorFixture::one_language(ProgrammingLanguage::TypeScript);
    let document = fixture.documents.first_mut().unwrap();
    let original = document.symbols[0].symbol.clone();
    document.symbols[0].symbol.clear();
    for occurrence in &mut document.occurrences {
        if occurrence.symbol == original {
            occurrence.symbol.clear();
        }
    }
    for relation in &mut fixture.relations {
        if relation.from == original {
            relation.from.clear();
        }
        if relation.to == original {
            relation.to.clear();
        }
    }

    let emission = fixture
        .emit()
        .expect("one malformed symbol must not abort the repository");
    assert!(emission.receipt.omitted_definition_count > 0);
    assert!(emission.receipt.omitted_relation_count > 0);
}

#[test]
fn all_ten_languages_preserve_language_specific_dispatch_without_guessing() {
    let fixture = DonorFixture::all_languages();
    let emission = fixture.emit().unwrap();
    let relations = read_ir_records(&emission.artifact.path)
        .into_iter()
        .filter_map(|record| match record {
            LanguageIrRecord::Relation(relation) => Some(relation),
            _ => None,
        })
        .collect::<Vec<_>>();

    for language in all_languages() {
        let marker = format!("fixture {} Target#", language.as_str());
        let relation = relations
            .iter()
            .find(|relation| {
                matches!(
                    &relation.target,
                    IrEndpoint::NativeSymbol { symbol_id } if symbol_id.as_str() == marker
                )
            })
            .unwrap_or_else(|| panic!("{} call relation", language.as_str()));
        let expected = match language {
            ProgrammingLanguage::TypeScript
            | ProgrammingLanguage::JavaScript
            | ProgrammingLanguage::Python => DispatchKind::Dynamic,
            _ => DispatchKind::Direct,
        };
        assert_eq!(relation.dispatch, expected, "{}", language.as_str());
        let execution = relation
            .execution
            .as_ref()
            .unwrap_or_else(|| panic!("{} execution occurrence", language.as_str()));
        assert_eq!(execution.lexical_ordinal, 0, "{}", language.as_str());
        assert!(
            relation
                .evidence_ids
                .contains(&execution.call_site_evidence_id),
            "{}",
            language.as_str()
        );
    }
}

#[test]
fn parallel_source_inventory_is_byte_identical_to_the_serial_path() {
    let fixture = DonorFixture::typescript_many_files(64);
    let workers = TestEnvironmentOverride::set("CODE_MEMORY_MAX_LANGUAGE_IR_WORKERS", "1");
    let serial = fixture.emit().unwrap();
    workers.update("8");
    let parallel = fixture.emit().unwrap();

    assert_eq!(
        serial.receipt.stream_set_digest,
        parallel.receipt.stream_set_digest
    );
    assert_eq!(
        serial.receipt.semantic_payload_set_digest,
        parallel.receipt.semantic_payload_set_digest
    );
    assert_eq!(
        serial.artifact.content_digest,
        parallel.artifact.content_digest
    );
    assert_eq!(serial.artifact.record_count, parallel.artifact.record_count);

    let normalize = |emission: &super::direct::DirectLanguageIrEmission, name: &str| {
        let output = TestProject::new(name);
        normalize_language_ir(CanonicalLanguageInput {
            project_root: &fixture.project.root,
            repository_display_name: "fixture",
            manifest: &fixture.census.manifest,
            plan: &fixture.plan,
            ir_path: &emission.artifact.path,
            ir_snapshot_id: &emission.artifact.snapshot_id,
            ir_content_digest: emission.artifact.content_digest,
            ir_record_count: emission.artifact.record_count,
            provider_set_digest: emission.receipt.provider_set_digest,
            execution_context_set_digest: emission.receipt.execution_context_set_digest,
            framework_ir: None,
            test_ir: None,
            output_root: &output.root,
        })
        .unwrap()
    };
    let serial_bundle = normalize(&serial, "serial-source-inventory-bundle");
    let parallel_bundle = normalize(&parallel, "parallel-source-inventory-bundle");
    assert_eq!(
        serial_bundle.receipt.semantic_digest,
        parallel_bundle.receipt.semantic_digest
    );
    assert_eq!(
        serial_bundle.artifact.bundle_digest,
        parallel_bundle.artifact.bundle_digest
    );
}

#[test]
fn all_ten_languages_link_into_one_deterministic_canonical_bundle() {
    let fixture = DonorFixture::all_languages();
    let first_ir = fixture.emit().unwrap();
    let repeated_ir = fixture.emit().unwrap();
    let first_output = TestProject::new("canonical-bundle-first");
    let repeated_output = TestProject::new("canonical-bundle-repeated");

    let first = normalize_language_ir(CanonicalLanguageInput {
        project_root: &fixture.project.root,
        repository_display_name: "fixture",
        manifest: &fixture.census.manifest,
        plan: &fixture.plan,
        ir_path: &first_ir.artifact.path,
        ir_snapshot_id: &first_ir.artifact.snapshot_id,
        ir_content_digest: first_ir.artifact.content_digest,
        ir_record_count: first_ir.artifact.record_count,
        provider_set_digest: first_ir.receipt.provider_set_digest,
        execution_context_set_digest: first_ir.receipt.execution_context_set_digest,
        framework_ir: None,
        test_ir: None,
        output_root: &first_output.root,
    })
    .unwrap();
    let repeated = normalize_language_ir(CanonicalLanguageInput {
        project_root: &fixture.project.root,
        repository_display_name: "fixture",
        manifest: &fixture.census.manifest,
        plan: &fixture.plan,
        ir_path: &repeated_ir.artifact.path,
        ir_snapshot_id: &repeated_ir.artifact.snapshot_id,
        ir_content_digest: repeated_ir.artifact.content_digest,
        ir_record_count: repeated_ir.artifact.record_count,
        provider_set_digest: repeated_ir.receipt.provider_set_digest,
        execution_context_set_digest: repeated_ir.receipt.execution_context_set_digest,
        framework_ir: None,
        test_ir: None,
        output_root: &repeated_output.root,
    })
    .unwrap();

    assert_eq!(first.receipt.provider_definition_identity_count, 22);
    assert_eq!(first.receipt.canonical_definition_node_count, 22);
    assert_eq!(first.receipt.retained_definition_node_count, 22);
    assert_eq!(first.receipt.pruned_definition_node_count, 0);
    assert_eq!(first.receipt.resolved_relation_count, 10);
    assert_eq!(first.receipt.unresolved_relation_count, 0);
    assert_eq!(first.manifest.analysis_unit_receipt_count, 10);
    assert_eq!(first.manifest.node_count, 33);
    assert_eq!(first.manifest.edge_count, 42);
    assert_eq!(first.manifest.file_coverage_count, 10);
    assert_eq!(first.manifest.capability_receipt_count, 110);
    assert_eq!(first.receipt.dangling_endpoint_count, 0);
    assert_eq!(first.receipt.confirmed_without_evidence_count, 0);
    assert_eq!(first.receipt.duplicate_logical_edge_count, 0);
    assert_eq!(
        first.receipt.semantic_digest,
        repeated.receipt.semantic_digest
    );
    assert_eq!(
        first.artifact.bundle_digest,
        repeated.artifact.bundle_digest
    );
    assert!(first.artifact.bundle_path.is_file());
    assert!(first.artifact.manifest_path.is_file());

    let connection = Connection::open(&first.artifact.bundle_path).unwrap();
    let mut statement = connection
        .prepare("SELECT payload_json FROM edges WHERE kind='calls' ORDER BY id")
        .unwrap();
    let call_edges = statement
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .map(|payload| serde_json::from_str::<FactEdge>(&payload.unwrap()).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(call_edges.len(), 10);
    assert_eq!(
        call_edges
            .iter()
            .filter(|edge| edge.dispatch == DispatchKind::Direct)
            .count(),
        7
    );
    assert_eq!(
        call_edges
            .iter()
            .filter(|edge| edge.dispatch == DispatchKind::Dynamic)
            .count(),
        3
    );
}

#[test]
fn canonical_bundle_retains_unreferenced_methods_for_later_trace_linking() {
    let mut fixture = DonorFixture::one_language(ProgrammingLanguage::Java);
    fixture.relations.clear();
    let emission = fixture.emit().unwrap();
    let output = TestProject::new("canonical-unreferenced-methods");

    let canonical = normalize_language_ir(CanonicalLanguageInput {
        project_root: &fixture.project.root,
        repository_display_name: "fixture",
        manifest: &fixture.census.manifest,
        plan: &fixture.plan,
        ir_path: &emission.artifact.path,
        ir_snapshot_id: &emission.artifact.snapshot_id,
        ir_content_digest: emission.artifact.content_digest,
        ir_record_count: emission.artifact.record_count,
        provider_set_digest: emission.receipt.provider_set_digest,
        execution_context_set_digest: emission.receipt.execution_context_set_digest,
        framework_ir: None,
        test_ir: None,
        output_root: &output.root,
    })
    .unwrap();

    let connection = Connection::open(&canonical.artifact.bundle_path).unwrap();
    let retained_methods: u64 = connection
        .query_row(
            "SELECT COUNT(*) FROM nodes WHERE kind='method'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(retained_methods, 2);
    assert_eq!(canonical.receipt.pruned_definition_node_count, 0);
}

#[test]
fn exact_sql_literal_links_callable_to_query_and_read_write_tables() {
    let mut fixture = DonorFixture::one_language(ProgrammingLanguage::Python);
    let source = r#"def Target():
    pass

def Caller(connection):
    connection.execute('''
        INSERT INTO sessions (id)
        SELECT id FROM staged_sessions
    ''')
    Target()
"#;
    fixture.project.write("main.py", source.as_bytes());
    let target_range = token_range(source, "Target", 0);
    let caller_range = unique_token_range(source, "Caller");
    let call_range = token_range(source, "Target", 1);
    fixture.documents[0].occurrences[0].range = target_range.clone();
    fixture.documents[0].occurrences[0].enclosing_range = target_range;
    fixture.documents[0].occurrences[1].range = caller_range.clone();
    fixture.documents[0].occurrences[1].enclosing_range = caller_range;
    fixture.relations[0].range = call_range;
    fixture.census = SourceCensus::scan(&fixture.project.root).unwrap();
    fixture.plan = plan_analysis_units(&fixture.project.root, &fixture.census.manifest).unwrap();

    let emission = fixture.emit().unwrap();
    let records = read_ir_records(&emission.artifact.path);
    assert_eq!(
        records
            .iter()
            .filter(|record| matches!(record, LanguageIrRecord::Definition(definition) if definition.canonical_kind_hint == FactNodeKind::Query))
            .count(),
        1
    );
    assert_eq!(
        records
            .iter()
            .filter(|record| matches!(record, LanguageIrRecord::Definition(definition) if definition.canonical_kind_hint == FactNodeKind::TableReference))
            .count(),
        2
    );

    let output = TestProject::new("canonical-sql-literal");
    let canonical = normalize_language_ir(CanonicalLanguageInput {
        project_root: &fixture.project.root,
        repository_display_name: "fixture",
        manifest: &fixture.census.manifest,
        plan: &fixture.plan,
        ir_path: &emission.artifact.path,
        ir_snapshot_id: &emission.artifact.snapshot_id,
        ir_content_digest: emission.artifact.content_digest,
        ir_record_count: emission.artifact.record_count,
        provider_set_digest: emission.receipt.provider_set_digest,
        execution_context_set_digest: emission.receipt.execution_context_set_digest,
        framework_ir: None,
        test_ir: None,
        output_root: &output.root,
    })
    .unwrap();

    let connection = Connection::open(&canonical.artifact.bundle_path).unwrap();
    let nodes = connection
        .prepare("SELECT payload_json FROM nodes ORDER BY id")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .map(|row| serde_json::from_str::<FactNode>(&row.unwrap()).unwrap())
        .map(|node| (node.id.clone(), node))
        .collect::<BTreeMap<_, _>>();
    let edges = connection
        .prepare("SELECT payload_json FROM edges ORDER BY id")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .map(|row| serde_json::from_str::<FactEdge>(&row.unwrap()).unwrap())
        .collect::<Vec<_>>();

    let query = nodes
        .values()
        .find(|node| node.kind == FactNodeKind::Query)
        .expect("query node");
    let sessions = nodes
        .values()
        .find(|node| node.kind == FactNodeKind::TableReference && node.display_name == "sessions")
        .expect("sessions table reference");
    let staged = nodes
        .values()
        .find(|node| {
            node.kind == FactNodeKind::TableReference && node.display_name == "staged_sessions"
        })
        .expect("staged table reference");
    let executes = edges
        .iter()
        .find(|edge| edge.kind == FactEdgeKind::ExecutesQuery)
        .expect("callable executes query");
    assert_eq!(nodes[&executes.source_id].display_name, "Caller");
    assert_eq!(executes.target_id, query.id);
    assert!(edges.iter().any(|edge| {
        edge.kind == FactEdgeKind::Writes
            && edge.source_id == query.id
            && edge.target_id == sessions.id
    }));
    assert!(edges.iter().any(|edge| {
        edge.kind == FactEdgeKind::Reads
            && edge.source_id == query.id
            && edge.target_id == staged.id
    }));
}

#[test]
fn framework_route_adapter_deduplicates_candidates_and_links_only_exact_handlers() {
    let fixture = DonorFixture::one_language(ProgrammingLanguage::TypeScript);
    let emission = fixture.emit().unwrap();
    let route_fact = |id: &str, path: Option<&str>| FrameworkFact {
        id: id.to_string(),
        kind: "HTTP_ROUTE".to_string(),
        framework: "express".to_string(),
        symbol: Some("fixture typescript Caller#call().".to_string()),
        method: Some("get".to_string()),
        path: path.map(str::to_string),
        source_file: "main.ts".to_string(),
        source_line: 2,
        source_end_line: 2,
        source_range: Vec::new(),
        evidence: vec!["fixture registration".to_string()],
        properties: BTreeMap::new(),
    };
    let analysis = FrameworkAnalysis {
        frameworks: vec![FrameworkOutput {
            id: "express".to_string(),
            language: "typescript".to_string(),
            name: "Express".to_string(),
            kind: "backend".to_string(),
            adapter: "typescript".to_string(),
            status: "detected".to_string(),
            matched_signals: Vec::new(),
            files: vec!["main.ts".to_string()],
            facts: vec![
                route_fact("route-health", Some("/health")),
                route_fact("route-health-duplicate", Some("/health")),
                route_fact("route-dynamic", None),
                route_fact("route-dynamic-duplicate", None),
            ],
        }],
        relations: Vec::new(),
    };
    let framework_ir = adapt_framework_routes(
        &fixture.project.root,
        &fixture.census.manifest,
        &fixture.plan,
        &emission.receipt.snapshot_id,
        &analysis,
    )
    .unwrap();

    assert_eq!(framework_ir.receipt.raw_candidate_count, 4);
    assert_eq!(framework_ir.receipt.planned_route_record_count, 2);
    assert_eq!(framework_ir.receipt.emitted_route_record_count, 1);
    assert_eq!(framework_ir.receipt.rejected_route_record_count, 1);
    assert_eq!(framework_ir.receipt.handler_reference_count, 1);
    assert_eq!(framework_ir.receipt.gap_count, 1);
    let audit = framework_ir.unit_audit.values().next().unwrap();
    assert_eq!(audit.candidate_count, 2);
    assert_eq!(audit.accepted_route_count, 1);
    assert_eq!(audit.rejected_route_count, 1);
    assert_eq!(audit.handler_reference_count, 1);

    let output = TestProject::new("canonical-framework-route-bundle");
    let canonical = normalize_language_ir(CanonicalLanguageInput {
        project_root: &fixture.project.root,
        repository_display_name: "fixture",
        manifest: &fixture.census.manifest,
        plan: &fixture.plan,
        ir_path: &emission.artifact.path,
        ir_snapshot_id: &emission.artifact.snapshot_id,
        ir_content_digest: emission.artifact.content_digest,
        ir_record_count: emission.artifact.record_count,
        provider_set_digest: emission.receipt.provider_set_digest,
        execution_context_set_digest: emission.receipt.execution_context_set_digest,
        framework_ir: Some(&framework_ir),
        test_ir: None,
        output_root: &output.root,
    })
    .unwrap();

    assert_eq!(canonical.receipt.framework_route_node_count, 1);
    assert_eq!(canonical.receipt.framework_exposes_edge_count, 1);
    assert_eq!(canonical.receipt.framework_handles_edge_count, 1);
    assert_eq!(canonical.receipt.framework_unresolved_handler_count, 0);

    let connection = Connection::open(&canonical.artifact.bundle_path).unwrap();
    let route_payload: String = connection
        .query_row(
            "SELECT payload_json FROM nodes WHERE kind = 'http_route'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let route: FactNode = serde_json::from_str(&route_payload).unwrap();
    assert_eq!(route.qualified_name, "GET /health");
    assert_eq!(
        route.details,
        Some(FactNodeDetails::HttpRoute {
            method: "GET".to_string(),
            path: "/health".to_string(),
        })
    );

    let mut statement = connection
        .prepare("SELECT payload_json FROM nodes ORDER BY id")
        .unwrap();
    let handler = statement
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .map(|row| serde_json::from_str::<FactNode>(&row.unwrap()).unwrap())
        .find(|node| {
            node.roles
                .iter()
                .any(|assignment| assignment.role == FactRole::Handler)
        })
        .expect("exact handler role");
    assert_eq!(handler.kind, FactNodeKind::Function);
    assert_eq!(handler.display_name, "Caller");

    let mut statement = connection
        .prepare("SELECT payload_json FROM capability_receipts ORDER BY record_key")
        .unwrap();
    let receipt = statement
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .map(|row| serde_json::from_str::<CapabilityReceipt>(&row.unwrap()).unwrap())
        .find(|receipt| receipt.capability == AnalysisCapability::FrameworkBindings)
        .expect("framework capability receipt");
    assert_eq!(receipt.execution_state, CapabilityExecutionState::Partial);
    assert_eq!(receipt.covered_count, 1);
    assert_eq!(receipt.emitted_fact_count, 1);
    assert_eq!(receipt.emitted_relation_count, 2);
    assert_eq!(receipt.gap_codes, vec![GapCode::RuntimeRegistration]);
}

#[test]
fn framework_pack_bytes_participate_in_analyzer_identity() {
    let pack_root = TestProject::new("framework-analyzer-identity");
    pack_root.write("packs/framework/catalog.json", br#"{"version":1}"#);
    let first = framework_analyzer_set_digest(&pack_root.root).unwrap();
    let repeated = framework_analyzer_set_digest(&pack_root.root).unwrap();
    assert_eq!(first, repeated);

    pack_root.write("packs/framework/catalog.json", br#"{"version":2}"#);
    let changed = framework_analyzer_set_digest(&pack_root.root).unwrap();
    assert_ne!(first, changed);
}

#[test]
fn human_diagnostic_wording_does_not_change_canonical_semantics() {
    let fixture = DonorFixture::one_language(ProgrammingLanguage::TypeScript);
    let diagnostic = |message: &str| Diagnostic {
        language: "typescript".to_string(),
        level: "warning",
        code: DiagnosticCode::ProviderFailed,
        message: message.to_string(),
        detail: None,
        path: None,
        line: None,
    };
    let first_ir = fixture
        .emit_with_coordinator(&[diagnostic("provider unavailable")])
        .unwrap();
    let second_ir = fixture
        .emit_with_coordinator(&[diagnostic("provider could not be started")])
        .unwrap();
    assert_eq!(first_ir.receipt.snapshot_id, second_ir.receipt.snapshot_id);
    assert_ne!(
        first_ir.artifact.content_digest,
        second_ir.artifact.content_digest
    );

    let first_output = TestProject::new("canonical-human-wording-first");
    let second_output = TestProject::new("canonical-human-wording-second");
    let normalize = |emission: &super::direct::DirectLanguageIrEmission, output: &TestProject| {
        normalize_language_ir(CanonicalLanguageInput {
            project_root: &fixture.project.root,
            repository_display_name: "fixture",
            manifest: &fixture.census.manifest,
            plan: &fixture.plan,
            ir_path: &emission.artifact.path,
            ir_snapshot_id: &emission.artifact.snapshot_id,
            ir_content_digest: emission.artifact.content_digest,
            ir_record_count: emission.artifact.record_count,
            provider_set_digest: emission.receipt.provider_set_digest,
            execution_context_set_digest: emission.receipt.execution_context_set_digest,
            framework_ir: None,
            test_ir: None,
            output_root: &output.root,
        })
        .unwrap()
    };
    let first = normalize(&first_ir, &first_output);
    let second = normalize(&second_ir, &second_output);

    assert_eq!(
        first.receipt.semantic_digest,
        second.receipt.semantic_digest
    );
    assert_ne!(first.artifact.bundle_digest, second.artifact.bundle_digest);
    assert_ne!(first.artifact.bundle_path, second.artifact.bundle_path);
}

#[test]
fn canonical_linker_never_resolves_an_unregistered_same_name_symbol() {
    let fixture = DonorFixture::one_language(ProgrammingLanguage::TypeScript);
    let emission = fixture.emit().unwrap();
    let mut records = read_ir_records(&emission.artifact.path);
    let relation = records
        .iter_mut()
        .find_map(|record| match record {
            LanguageIrRecord::Relation(relation) => Some(relation),
            _ => None,
        })
        .expect("fixture relation");
    relation.target = IrEndpoint::NativeSymbol {
        // It deliberately keeps the same human name. Only the provider-native
        // identity differs, so a name-based fallback would create a false edge.
        symbol_id: ProviderSymbolId::parse("fixture typescript Target#missing().").unwrap(),
    };
    let rewritten = TestProject::new("canonical-unregistered-symbol-ir");
    let (ir_path, digest, count) = write_ir_records(&rewritten, &records);
    let output = TestProject::new("canonical-unregistered-symbol-bundle");

    let canonical = normalize_language_ir(CanonicalLanguageInput {
        project_root: &fixture.project.root,
        repository_display_name: "fixture",
        manifest: &fixture.census.manifest,
        plan: &fixture.plan,
        ir_path: &ir_path,
        ir_snapshot_id: &emission.artifact.snapshot_id,
        ir_content_digest: digest,
        ir_record_count: count,
        provider_set_digest: emission.receipt.provider_set_digest,
        execution_context_set_digest: emission.receipt.execution_context_set_digest,
        framework_ir: None,
        test_ir: None,
        output_root: &output.root,
    })
    .unwrap();

    assert_eq!(canonical.receipt.resolved_relation_count, 0);
    assert_eq!(canonical.receipt.unresolved_relation_count, 1);
    assert_eq!(canonical.manifest.node_count, 4);
    assert_eq!(canonical.manifest.edge_count, 3);
    assert_eq!(canonical.receipt.dangling_endpoint_count, 0);
}

#[test]
fn canonical_linker_merges_duplicate_logical_edges_without_losing_evidence() {
    let fixture = DonorFixture::one_language(ProgrammingLanguage::TypeScript);
    let emission = fixture.emit().unwrap();
    let mut records = read_ir_records(&emission.artifact.path);
    let relation = records
        .iter()
        .find_map(|record| match record {
            LanguageIrRecord::Relation(relation) => Some(relation.clone()),
            _ => None,
        })
        .expect("fixture relation");
    let completion_index = records
        .iter()
        .position(|record| matches!(record, LanguageIrRecord::Complete(_)))
        .expect("fixture completion");
    records.insert(completion_index, LanguageIrRecord::Relation(relation));
    let LanguageIrRecord::Complete(completion) = records.last_mut().unwrap() else {
        panic!("completion remains last");
    };
    completion.relation_count += 1;
    let rewritten = TestProject::new("canonical-duplicate-edge-ir");
    let (ir_path, digest, count) = write_ir_records(&rewritten, &records);
    let output = TestProject::new("canonical-duplicate-edge-bundle");

    let canonical = normalize_language_ir(CanonicalLanguageInput {
        project_root: &fixture.project.root,
        repository_display_name: "fixture",
        manifest: &fixture.census.manifest,
        plan: &fixture.plan,
        ir_path: &ir_path,
        ir_snapshot_id: &emission.artifact.snapshot_id,
        ir_content_digest: digest,
        ir_record_count: count,
        provider_set_digest: emission.receipt.provider_set_digest,
        execution_context_set_digest: emission.receipt.execution_context_set_digest,
        framework_ir: None,
        test_ir: None,
        output_root: &output.root,
    })
    .unwrap();

    assert_eq!(canonical.receipt.resolved_relation_count, 2);
    assert_eq!(canonical.receipt.merged_edge_count, 1);
    assert_eq!(canonical.manifest.edge_count, 4);
    assert_eq!(canonical.receipt.duplicate_logical_edge_count, 0);
}

#[test]
fn canonical_linker_preserves_distinct_written_calls_to_the_same_target() {
    let mut fixture = DonorFixture::one_language(ProgrammingLanguage::TypeScript);
    let source = "function Target() {}\nfunction Caller() { Target(); Target(); }\n";
    fixture.project.write("main.ts", source.as_bytes());
    fixture.relations[0].range = token_range(source, "Target", 1);
    let mut second_call = fixture.relations[0].clone();
    second_call.range = token_range(source, "Target", 2);
    fixture.relations.push(second_call);
    fixture.census = SourceCensus::scan(&fixture.project.root).unwrap();
    fixture.plan = plan_analysis_units(&fixture.project.root, &fixture.census.manifest).unwrap();

    let emission = fixture.emit().unwrap();
    let mut ir_occurrences = read_ir_records(&emission.artifact.path)
        .into_iter()
        .filter_map(|record| match record {
            LanguageIrRecord::Relation(relation) => relation.execution,
            _ => None,
        })
        .collect::<Vec<_>>();
    ir_occurrences.sort_by_key(|execution| execution.lexical_ordinal);
    assert_eq!(ir_occurrences.len(), 2);
    assert_eq!(ir_occurrences[0].lexical_ordinal, 0);
    assert_eq!(ir_occurrences[1].lexical_ordinal, 1);
    assert_ne!(
        ir_occurrences[0].call_site_evidence_id,
        ir_occurrences[1].call_site_evidence_id
    );

    let output = TestProject::new("canonical-two-call-occurrences");
    let canonical = normalize_language_ir(CanonicalLanguageInput {
        project_root: &fixture.project.root,
        repository_display_name: "fixture",
        manifest: &fixture.census.manifest,
        plan: &fixture.plan,
        ir_path: &emission.artifact.path,
        ir_snapshot_id: &emission.artifact.snapshot_id,
        ir_content_digest: emission.artifact.content_digest,
        ir_record_count: emission.artifact.record_count,
        provider_set_digest: emission.receipt.provider_set_digest,
        execution_context_set_digest: emission.receipt.execution_context_set_digest,
        framework_ir: None,
        test_ir: None,
        output_root: &output.root,
    })
    .unwrap();

    let connection = Connection::open(&canonical.artifact.bundle_path).unwrap();
    let mut statement = connection
        .prepare("SELECT payload_json FROM edges WHERE kind='calls' ORDER BY execution_site_id")
        .unwrap();
    let call_edges = statement
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .map(|payload| serde_json::from_str::<FactEdge>(&payload.unwrap()).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(call_edges.len(), 2);
    assert_ne!(call_edges[0].id, call_edges[1].id);
    assert_eq!(canonical.receipt.resolved_relation_count, 2);
    assert_eq!(canonical.receipt.merged_edge_count, 0);
    assert_eq!(canonical.receipt.duplicate_logical_edge_count, 0);
}

#[test]
fn canonical_linker_keeps_valid_forward_definition_evidence_references() {
    let fixture = DonorFixture::one_language(ProgrammingLanguage::TypeScript);
    let emission = fixture.emit().unwrap();
    let mut records = read_ir_records(&emission.artifact.path);
    let definition_evidence_id = records
        .iter()
        .find_map(|record| match record {
            LanguageIrRecord::Definition(definition) => {
                Some(definition.definition_evidence_id.clone())
            }
            _ => None,
        })
        .expect("fixture definition evidence");
    let evidence_index = records
        .iter()
        .position(|record| {
            matches!(record, LanguageIrRecord::Evidence(evidence) if evidence.id == definition_evidence_id)
        })
        .expect("fixture evidence record");
    let evidence = records.remove(evidence_index);
    let relation_index = records
        .iter()
        .position(|record| matches!(record, LanguageIrRecord::Relation(_)))
        .expect("fixture relation");
    assert!(records[..relation_index].iter().any(|record| {
        matches!(record, LanguageIrRecord::Definition(definition) if definition.definition_evidence_id == definition_evidence_id)
    }));
    records.insert(relation_index, evidence);

    let rewritten = TestProject::new("canonical-forward-definition-evidence-ir");
    let (ir_path, digest, count) = write_ir_records(&rewritten, &records);
    let output = TestProject::new("canonical-forward-definition-evidence-bundle");
    let canonical = normalize_language_ir(CanonicalLanguageInput {
        project_root: &fixture.project.root,
        repository_display_name: "fixture",
        manifest: &fixture.census.manifest,
        plan: &fixture.plan,
        ir_path: &ir_path,
        ir_snapshot_id: &emission.artifact.snapshot_id,
        ir_content_digest: digest,
        ir_record_count: count,
        provider_set_digest: emission.receipt.provider_set_digest,
        execution_context_set_digest: emission.receipt.execution_context_set_digest,
        framework_ir: None,
        test_ir: None,
        output_root: &output.root,
    })
    .unwrap();

    assert_eq!(canonical.receipt.provider_definition_identity_count, 2);
    assert_eq!(canonical.receipt.resolved_relation_count, 1);
    assert_eq!(canonical.receipt.confirmed_without_evidence_count, 0);
}

#[test]
fn canonical_linker_rejects_a_relation_whose_evidence_record_is_missing() {
    let fixture = DonorFixture::one_language(ProgrammingLanguage::TypeScript);
    let emission = fixture.emit().unwrap();
    let mut records = read_ir_records(&emission.artifact.path);
    let relation_evidence = records
        .iter()
        .find_map(|record| match record {
            LanguageIrRecord::Relation(relation) => relation.evidence_ids.first().cloned(),
            _ => None,
        })
        .expect("fixture relation evidence");
    records.retain(|record| {
        !matches!(record, LanguageIrRecord::Evidence(evidence) if evidence.id == relation_evidence)
    });
    let LanguageIrRecord::Complete(completion) = records.last_mut().unwrap() else {
        panic!("completion remains last");
    };
    completion.evidence_count -= 1;
    let rewritten = TestProject::new("canonical-missing-evidence-ir");
    let (ir_path, digest, count) = write_ir_records(&rewritten, &records);
    let output = TestProject::new("canonical-missing-evidence-bundle");

    let error = normalize_language_ir(CanonicalLanguageInput {
        project_root: &fixture.project.root,
        repository_display_name: "fixture",
        manifest: &fixture.census.manifest,
        plan: &fixture.plan,
        ir_path: &ir_path,
        ir_snapshot_id: &emission.artifact.snapshot_id,
        ir_content_digest: digest,
        ir_record_count: count,
        provider_set_digest: emission.receipt.provider_set_digest,
        execution_context_set_digest: emission.receipt.execution_context_set_digest,
        framework_ir: None,
        test_ir: None,
        output_root: &output.root,
    })
    .err()
    .expect("missing evidence must block publication");
    assert!(error.contains("relation references missing evidence"));
    assert!(fs::read_dir(&output.root).unwrap().next().is_none());
}

#[test]
fn actual_provider_context_partitions_snapshot_identity() {
    let fixture = DonorFixture::one_language(ProgrammingLanguage::TypeScript);
    let first = fixture
        .emit_with_batches(fixture.provider_batches(), &[])
        .unwrap();

    let mut changed_batches = fixture.provider_batches();
    let original = &changed_batches[0].execution_context;
    changed_batches[0].execution_context = ProviderExecutionContext::executed(
        ProviderExecutionMode::GeneratedProject,
        original.analysis_root.clone().unwrap(),
        original.source_scope_digest.unwrap(),
        original.source_file_count,
        original.config_artifacts.clone(),
        Some(Sha256Digest::of_bytes(
            b"different generated provider project",
        )),
        original.dimensions.clone(),
        original.missing_dimensions.clone(),
    )
    .unwrap();
    let changed = fixture.emit_with_batches(changed_batches, &[]).unwrap();

    assert_ne!(first.receipt.snapshot_id, changed.receipt.snapshot_id);
    assert_ne!(
        first.receipt.execution_context_set_digest,
        changed.receipt.execution_context_set_digest
    );
    assert_ne!(
        first.artifact.content_digest,
        changed.artifact.content_digest
    );
}

#[test]
fn direct_provider_batches_publish_one_complete_stream_authority() {
    let fixture = DonorFixture::all_languages();
    let emission = fixture.emit().unwrap();
    let bytes = fs::read(&emission.artifact.path).unwrap();
    let text = std::str::from_utf8(&bytes).unwrap();
    let records = text
        .lines()
        .map(|line| serde_json::from_str::<LanguageIrRecord>(line).unwrap())
        .collect::<Vec<_>>();

    assert_eq!(emission.receipt.emitted_unit_count, 10);
    assert_eq!(records.len() as u64, emission.receipt.record_count);
    assert_eq!(
        emission.artifact.record_count,
        emission.receipt.record_count
    );
    assert_eq!(emission.artifact.byte_count, bytes.len() as u64);
    assert_eq!(
        emission.artifact.content_digest,
        Sha256Digest::of_bytes(&bytes)
    );
    assert_eq!(
        emission.artifact.stream_set_digest,
        emission.receipt.stream_set_digest
    );
    assert!(emission.artifact.complete);
}

#[test]
fn coordinator_diagnostics_are_in_the_authoritative_stream() {
    let fixture = DonorFixture::one_language(ProgrammingLanguage::TypeScript);
    let coordinator_diagnostics = vec![Diagnostic {
        language: "typescript".to_string(),
        level: "warning",
        code: DiagnosticCode::ProviderFailed,
        message: "TypeScript project model unavailable".to_string(),
        detail: None,
        path: None,
        line: None,
    }];

    let emission = fixture
        .emit_with_coordinator(&coordinator_diagnostics)
        .unwrap();

    assert_eq!(emission.receipt.issue_count, 1);
    assert_eq!(
        emission.artifact.stream_set_digest,
        emission.receipt.stream_set_digest
    );
}

#[test]
fn all_ten_provider_execution_contexts_reconcile_deterministically() {
    let fixture = DonorFixture::all_languages();
    let batches = fixture.provider_batches();
    let first = reconcile_provider_execution_contexts(&batches, &fixture.plan).unwrap();
    let repeated = reconcile_provider_execution_contexts(&batches, &fixture.plan).unwrap();
    let first = serde_json::to_value(first).unwrap();
    let repeated = serde_json::to_value(repeated).unwrap();

    assert_eq!(first, repeated);
    assert_eq!(first["executionCount"], 10);
    assert_eq!(first["partialExecutionCount"], 10);
    assert_eq!(first["notExecutedCount"], 0);
    assert!(first["contextSetDigest"]
        .as_str()
        .is_some_and(|value| value.len() == 64));
}

#[test]
fn several_execution_shards_form_one_honest_composite_unit_context() {
    let fixture = DonorFixture::one_language(ProgrammingLanguage::Dart);
    let mut batches = fixture.provider_batches();
    batches.extend(fixture.provider_batches());

    let contexts = execution_contexts_by_unit(&batches, &fixture.plan).unwrap();
    let context = contexts.values().next().expect("one analysis unit");

    assert_eq!(contexts.len(), 1);
    assert_eq!(context.mode, ProviderExecutionMode::Composite);
    assert_eq!(context.source_file_count, 1);
    assert!(context.generated_context_digest.is_some());
    context.validate().unwrap();
}

#[test]
fn provider_execution_root_mismatch_is_blocking() {
    let fixture = DonorFixture::one_language(ProgrammingLanguage::Java);
    let mut batches = fixture.provider_batches();
    let source = fixture.project.root.join(&batches[0].source_files[0]);
    let nested_root = source.parent().unwrap();
    batches[0].execution_context = ProviderExecutionContext::executed(
        ProviderExecutionMode::InferredWorkspace,
        RepositoryPath::parse(
            nested_root
                .strip_prefix(&fixture.project.root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/"),
        )
        .unwrap_or_else(|_| RepositoryPath::parse("other-root").unwrap()),
        Sha256Digest::of_bytes(b"valid-but-wrong-root-scope"),
        1,
        Vec::new(),
        None,
        Vec::new(),
        Vec::new(),
    )
    .unwrap();

    let error = reconcile_provider_execution_contexts(&batches, &fixture.plan).unwrap_err();
    assert!(error.contains("executed root"));
}

#[test]
fn provider_execution_rejects_unplanned_configuration() {
    let fixture = DonorFixture::one_language(ProgrammingLanguage::Java);
    let mut batches = fixture.provider_batches();
    let rogue = fixture.project.root.join("provider-only.config");
    fs::write(&rogue, "not in the AnalysisPlan\n").unwrap();
    let source = fixture.project.root.join(&batches[0].source_files[0]);
    let spec = LANGUAGES
        .iter()
        .find(|language| language.id == "java")
        .unwrap();
    batches[0].execution_context = executed_provider_context(ExecutedProviderContextInput {
        project_root: &fixture.project.root,
        language: spec,
        mode: ProviderExecutionMode::Project,
        analysis_root: &fixture.project.root,
        source_files: std::slice::from_ref(&source),
        config_files: vec![(rogue, ProviderConfigUse::ExplicitArgument)],
        generated_context_digest: None,
        dimensions: Vec::new(),
    })
    .unwrap();

    let error = reconcile_provider_execution_contexts(&batches, &fixture.plan).unwrap_err();
    assert!(error.contains("outside AnalysisPlan"));
}

#[test]
fn provider_execution_rejects_a_project_dimension_different_from_the_plan() {
    let fixture = DonorFixture::one_language(ProgrammingLanguage::TypeScript);
    let mut batches = fixture.provider_batches();
    let source = fixture.project.root.join(&batches[0].source_files[0]);
    let config = fixture.project.root.join("tsconfig.json");
    let spec = LANGUAGES
        .iter()
        .find(|language| language.id == "typescript")
        .unwrap();
    batches[0].execution_context = executed_provider_context(ExecutedProviderContextInput {
        project_root: &fixture.project.root,
        language: spec,
        mode: ProviderExecutionMode::Project,
        analysis_root: &fixture.project.root,
        source_files: std::slice::from_ref(&source),
        config_files: vec![(config, ProviderConfigUse::ExplicitArgument)],
        generated_context_digest: None,
        dimensions: vec![ContextDimension {
            kind: ContextDimensionKind::Target,
            value: "es5".to_string(),
        }],
    })
    .unwrap();

    let error = reconcile_provider_execution_contexts(&batches, &fixture.plan).unwrap_err();
    assert!(error.contains("dimensions outside AnalysisPlan"));
}

#[test]
fn direct_provider_scope_rejects_out_of_scope_semantic_payload() {
    let mut fixture = DonorFixture::one_language(ProgrammingLanguage::Java);
    fixture.documents[0].path = "invented/Outside.java".to_string();

    let error = fixture
        .emit()
        .err()
        .expect("out-of-scope payload must fail");

    assert!(error.contains("out-of-scope document"));
}

#[test]
fn every_language_uses_the_same_fail_closed_provider_missing_contract() {
    for language in all_languages() {
        let mut fixture = DonorFixture::one_language(language);
        fixture.languages[0].files_indexed = 0;
        fixture.languages[0].files_missing = 1;
        fixture.languages[0].status = "missing-tool";
        fixture.documents.clear();
        fixture.relations.clear();
        fixture.coverage[0].status = "missing";
        fixture.coverage[0].reason = Some("provider-missing".to_string());
        fixture.diagnostics.push(Diagnostic {
            language: language.as_str().to_string(),
            level: "error",
            code: DiagnosticCode::ProviderMissing,
            message: "fixture provider is unavailable".to_string(),
            detail: None,
            path: None,
            line: None,
        });

        let mut batches = fixture.provider_batches();
        let spec = LANGUAGES
            .iter()
            .find(|spec| spec.contract_language == language)
            .unwrap();
        batches[0].execution_context = not_executed_provider_context(spec);
        let emission = fixture.emit_with_batches(batches, &[]).unwrap();
        let receipt = emission.receipt;
        assert_eq!(receipt.definition_count, 0, "{}", language.as_str());
        assert_eq!(receipt.relation_count, 0, "{}", language.as_str());
        assert!(receipt.gap_count > 0, "{}", language.as_str());
        assert!(receipt.issue_count > 0, "{}", language.as_str());
    }
}

#[test]
fn capability_registry_is_closed_and_unique_for_every_language() {
    for language in all_languages() {
        for protocol in [
            ProviderProtocol::Scip,
            ProviderProtocol::LanguageServerProtocol,
            ProviderProtocol::CompilerApi,
        ] {
            let policies = capability_policies(language, protocol);
            assert_eq!(policies.len(), 9);
            let capabilities = policies
                .iter()
                .map(|policy| policy.capability)
                .collect::<BTreeSet<_>>();
            assert_eq!(capabilities.len(), 9);
            assert_eq!(
                capabilities,
                BTreeSet::from([
                    AnalysisCapability::ProjectStructure,
                    AnalysisCapability::Definitions,
                    AnalysisCapability::Imports,
                    AnalysisCapability::Exports,
                    AnalysisCapability::DirectCalls,
                    AnalysisCapability::TypeRelations,
                    AnalysisCapability::Overrides,
                    AnalysisCapability::OrmQuery,
                    AnalysisCapability::EventExternal,
                ])
            );
        }
    }
}

#[test]
fn imports_use_full_policy_only_because_the_independent_site_audit_can_downgrade_it() {
    for language in all_languages() {
        for protocol in [
            ProviderProtocol::Scip,
            ProviderProtocol::LanguageServerProtocol,
            ProviderProtocol::CompilerApi,
        ] {
            let policy = capability_policies(language, protocol)
                .into_iter()
                .find(|policy| policy.capability == AnalysisCapability::Imports)
                .expect("closed imports capability");
            assert_eq!(policy.declared_support, DeclaredSupport::Required);
            assert_eq!(policy.measurement, AdapterMeasurement::Full);
        }
    }
}

#[test]
fn lsp_utf16_coordinates_become_utf8_byte_offsets_without_guessing() {
    let project = TestProject::new("utf16");
    project.write("pyproject.toml", b"[project]\nname='fixture'\n");
    project.write("main.py", "😀Target\n".as_bytes());
    let census = SourceCensus::scan(&project.root).unwrap();
    let file = manifest_file(&census, "main.py");
    let coordinates = SourceCoordinates::load(&project.root, file).unwrap();
    let span = coordinates
        .span(&[0, 2, 0, 8], ProviderProtocol::LanguageServerProtocol)
        .unwrap();
    assert_eq!(span.start.utf8_column, 4);
    assert_eq!(span.start.byte_offset, 4);
    assert_eq!(span.end.utf8_column, 10);
    assert_eq!(span.end.byte_offset, 10);
    assert!(coordinates
        .span(&[0, 1, 0, 8], ProviderProtocol::LanguageServerProtocol)
        .is_err());
}

#[test]
fn default_language_ir_receipt_stays_bounded_while_diagnostics_keep_audit_detail() {
    let emission = DonorFixture::one_language(ProgrammingLanguage::TypeScript)
        .emit()
        .unwrap();
    let receipt = serde_json::to_value(&emission.receipt).unwrap();
    let diagnostics = serde_json::to_value(&emission.diagnostics).unwrap();

    assert_eq!(
        receipt["schema"],
        "codebase-workspace.language-ir-migration-receipt.v7"
    );
    assert!(receipt.get("definitionLanguageSummaries").is_none());
    assert!(receipt.get("definitionAuditSample").is_none());
    assert!(receipt.get("importAuditSample").is_none());
    assert!(receipt.get("typeRelationAuditSample").is_none());
    assert!(receipt.get("unavailableUnitSample").is_none());
    assert_eq!(
        diagnostics["schema"],
        "codebase-workspace.language-ir-diagnostic-receipt.v1"
    );
    assert!(diagnostics["definitionLanguageSummaries"].is_array());
    assert!(diagnostics["definitionMetadataAuditSample"].is_array());
}

#[test]
fn source_change_after_census_fails_the_ir_boundary() {
    let fixture = DonorFixture::one_language(ProgrammingLanguage::TypeScript);
    let path = fixture.project.root.join("main.ts");
    fs::write(&path, b"Target\nChange\n").unwrap();
    let error = fixture.emit().err().expect("source mutation must fail");
    assert!(
        error.contains("source digest changed after census")
            || error.contains("source size changed after census")
    );
}

#[test]
fn invalid_provider_range_is_omitted_and_never_promoted_to_a_relation() {
    let mut fixture = DonorFixture::one_language(ProgrammingLanguage::TypeScript);
    fixture.relations[0].range = vec![999, 0, 999, 1];
    let receipt = fixture.emit().unwrap().receipt;
    assert_eq!(receipt.relation_count, 0);
    assert_eq!(receipt.omitted_relation_count, 1);
    assert!(receipt.gap_count > 0);
}

#[test]
fn provider_relation_at_a_declaration_is_not_forged_into_a_call_site() {
    let mut fixture = DonorFixture::one_language(ProgrammingLanguage::TypeScript);
    fixture.relations[0].range = fixture.documents[0].occurrences[1].range.clone();

    let receipt = fixture.emit().unwrap().receipt;

    assert_eq!(receipt.relation_count, 0);
    assert_eq!(receipt.omitted_relation_count, 1);
    assert!(receipt.gap_count > 0);
}

#[test]
fn python_class_call_is_kept_as_dynamic_construction_only_with_exact_type_target() {
    let mut fixture = DonorFixture::one_language(ProgrammingLanguage::Python);
    let source = "class Box:\n    pass\n\ndef Caller():\n    Box()\n";
    fixture.project.write("main.py", source.as_bytes());
    let document = fixture.documents.first_mut().unwrap();
    document.symbols[0].kind = "Class".to_string();
    document.symbols[0].display_name = Some("Box".to_string());
    document.symbols[1].display_name = Some("Caller".to_string());
    document.occurrences[0].range = token_range(source, "Box", 0);
    document.occurrences[0].enclosing_range = document.occurrences[0].range.clone();
    document.occurrences[1].range = unique_token_range(source, "Caller");
    document.occurrences[1].enclosing_range = document.occurrences[1].range.clone();
    fixture.relations[0].kind = "CONSTRUCTS".to_string();
    fixture.relations[0].range = token_range(source, "Box", 1);
    fixture.census = SourceCensus::scan(&fixture.project.root).unwrap();
    fixture.plan = plan_analysis_units(&fixture.project.root, &fixture.census.manifest).unwrap();

    let emission = fixture.emit().unwrap();
    let relation = read_ir_records(&emission.artifact.path)
        .into_iter()
        .find_map(|record| match record {
            LanguageIrRecord::Relation(relation) => Some(relation),
            _ => None,
        })
        .expect("exact Python class target must retain the construction fact");

    assert_eq!(relation.dispatch, DispatchKind::Dynamic);
    assert_eq!(emission.receipt.relation_count, 1);
    assert_eq!(emission.receipt.omitted_relation_count, 0);
}

#[test]
fn generic_provider_references_are_not_persisted_as_product_relations() {
    for native_kind in ["REFERENCES", "SYMBOL_REFERENCE"] {
        let mut fixture = DonorFixture::one_language(ProgrammingLanguage::TypeScript);
        fixture.relations[0].kind = native_kind.to_string();

        let receipt = fixture.emit().unwrap().receipt;

        assert_eq!(receipt.relation_count, 0, "{native_kind}");
        assert_eq!(receipt.omitted_relation_count, 0, "{native_kind}");
        assert_eq!(receipt.capability_receipt_count, 9, "{native_kind}");
    }
}

#[test]
fn definitions_do_not_create_a_relation_without_provider_evidence() {
    let mut fixture = DonorFixture::one_language(ProgrammingLanguage::Python);
    fixture.relations.clear();
    let receipt = fixture.emit().unwrap().receipt;
    assert_eq!(receipt.definition_count, 2);
    assert_eq!(receipt.relation_count, 0);
}

#[test]
fn incomplete_syntax_tree_is_published_as_an_exact_file_gap() {
    let mut fixture = DonorFixture::one_language(ProgrammingLanguage::Python);
    fixture.project.write("main.py", b"def broken(:\n");
    fixture.documents[0].symbols.clear();
    fixture.documents[0].occurrences.clear();
    fixture.relations.clear();
    fixture.census = SourceCensus::scan(&fixture.project.root).unwrap();
    fixture.plan = plan_analysis_units(&fixture.project.root, &fixture.census.manifest).unwrap();

    let emission = fixture.emit().unwrap();
    let gap = read_ir_records(&emission.artifact.path)
        .into_iter()
        .find_map(|record| match record {
            LanguageIrRecord::Gap(gap)
                if gap.code == GapCode::ProviderExecutionIncomplete
                    && gap.capability == Some(AnalysisCapability::Definitions)
                    && matches!(
                        &gap.scope,
                        AnalysisScope::File { path, .. } if path.as_str() == "main.py"
                    ) =>
            {
                Some(gap)
            }
            _ => None,
        })
        .expect("syntax failure must remain visible at file scope");

    assert!(matches!(gap.scope, AnalysisScope::File { .. }));
}

#[test]
fn java_blank_provider_signature_is_absent_instead_of_rejecting_the_unit() {
    let mut fixture = DonorFixture::one_language(ProgrammingLanguage::Java);
    fixture.documents[0].symbols[1].signature = Some("  \t ".to_string());

    let receipt = fixture.emit().unwrap().receipt;

    assert_eq!(receipt.definition_count, 3);
    assert_eq!(receipt.omitted_definition_count, 0);
}

#[test]
fn csharp_provider_display_language_is_normalized_before_ir_partitioning() {
    let mut fixture = DonorFixture::one_language(ProgrammingLanguage::CSharp);
    fixture.documents[0].language = "C#".to_string();

    let receipt = fixture.emit().unwrap().receipt;

    assert_eq!(receipt.definition_count, 3);
    assert_eq!(receipt.relation_count, 1);
}

#[test]
fn managed_provider_digest_mismatch_is_rejected_before_provenance() {
    let fixture = DonorFixture::one_language(ProgrammingLanguage::TypeScript);
    let manifest_path = fixture.providers.root.join("manifest.json");
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
    for provider in manifest["providers"].as_array_mut().unwrap() {
        if provider["command"] == "scip-typescript" {
            provider["sha256"] = serde_json::Value::String("0".repeat(64));
        }
    }
    fs::write(&manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    let error = fixture
        .emit()
        .err()
        .expect("provider digest mismatch must fail");
    assert!(error.contains("provider artifact digest does not match"));
}

#[test]
fn missing_language_output_is_a_typed_coordinator_gap_not_empty_success() {
    let mut fixture = DonorFixture::one_language(ProgrammingLanguage::TypeScript);
    fixture.languages.clear();
    let emission = fixture.emit().unwrap();
    let receipt = emission.receipt;

    assert_eq!(receipt.emitted_unit_count, 0);
    assert_eq!(receipt.unavailable_unit_count, 1);
    assert_eq!(emission.diagnostics.unavailable_unit_sample.len(), 1);
    let gap = &emission.diagnostics.unavailable_unit_sample[0];
    assert_eq!(gap.language, ProgrammingLanguage::TypeScript);
    assert_eq!(gap.gap_code, GapCode::ProviderExecutionIncomplete);
    assert!(!gap.unit_id.is_empty());
    assert_eq!(gap.root.as_str(), ".");
}

#[test]
fn zero_width_provider_document_sentinel_is_not_a_definition() {
    let mut fixture = DonorFixture::one_language(ProgrammingLanguage::TypeScript);
    let document = fixture.documents.first_mut().unwrap();
    let caller = document.symbols[1].symbol.clone();
    let caller_range = document.occurrences[1].range.clone();
    let sentinel = "scip-typescript npm fixture 1.0.0 `main.ts`/".to_string();
    document.symbols.push(SymbolOutput {
        symbol: sentinel.clone(),
        kind: "Namespace".to_string(),
        display_name: Some("ts`".to_string()),
        documentation: Vec::new(),
        signature: None,
        enclosing_symbol: None,
    });
    document.occurrences.push(OccurrenceOutput {
        symbol: sentinel.clone(),
        range: vec![0, 0, 0],
        enclosing_range: Vec::new(),
        definition: true,
        import: false,
        read: false,
        write: false,
    });
    fixture.relations.push(RelationOutput {
        from: caller,
        to: sentinel,
        kind: "CALLS".to_string(),
        path: "main.ts".to_string(),
        range: caller_range,
        confidence: Some(1.0),
        strategy: Some("fixture-document-sentinel".to_string()),
    });

    let receipt = fixture.emit().unwrap().receipt;
    assert_eq!(receipt.definition_count, 2);
    assert_eq!(receipt.relation_count, 1);
    assert_eq!(receipt.omitted_definition_count, 1);
}

struct DonorFixture {
    project: TestProject,
    providers: TestProject,
    artifacts: TestProject,
    census: SourceCensus,
    plan: codebase_fact_model::analysis_plan::AnalysisPlan,
    languages: Vec<LanguageOutput>,
    coverage: Vec<FileCoverageOutput>,
    documents: Vec<DocumentOutput>,
    relations: Vec<RelationOutput>,
    diagnostics: Vec<Diagnostic>,
}

impl DonorFixture {
    fn all_languages() -> Self {
        Self::new(&all_languages())
    }

    fn one_language(language: ProgrammingLanguage) -> Self {
        Self::new(&[language])
    }

    fn typescript_many_files(file_count: usize) -> Self {
        assert!(file_count >= 1);
        let mut fixture = Self::one_language(ProgrammingLanguage::TypeScript);
        let spec = LANGUAGES
            .iter()
            .find(|candidate| candidate.contract_language == ProgrammingLanguage::TypeScript)
            .unwrap();
        for ordinal in 1..file_count {
            let path = format!("src/file-{ordinal:04}.ts");
            let target_name = format!("Target{ordinal:04}");
            let caller_name = format!("Caller{ordinal:04}");
            let source = format!(
                "export function {target_name}() {{}}\nexport function {caller_name}() {{ {target_name}(); }}\n"
            );
            fixture.project.write(&path, source.as_bytes());
            let target_range = token_range(&source, &target_name, 0);
            let caller_range = unique_token_range(&source, &caller_name);
            let call_range = token_range(&source, &target_name, 1);
            let target = format!("fixture typescript {target_name}#");
            let caller = format!("fixture typescript {caller_name}#call().");
            fixture.documents.push(DocumentOutput {
                language: ProgrammingLanguage::TypeScript.as_str().to_string(),
                path: path.clone(),
                symbols: vec![
                    SymbolOutput {
                        symbol: target.clone(),
                        kind: "Function".to_string(),
                        display_name: Some(target_name),
                        documentation: Vec::new(),
                        signature: None,
                        enclosing_symbol: None,
                    },
                    SymbolOutput {
                        symbol: caller.clone(),
                        kind: "Function".to_string(),
                        display_name: Some(caller_name),
                        documentation: Vec::new(),
                        signature: None,
                        enclosing_symbol: None,
                    },
                ],
                occurrences: vec![
                    OccurrenceOutput {
                        symbol: target.clone(),
                        range: target_range.clone(),
                        enclosing_range: target_range,
                        definition: true,
                        import: false,
                        read: false,
                        write: false,
                    },
                    OccurrenceOutput {
                        symbol: caller.clone(),
                        range: caller_range.clone(),
                        enclosing_range: caller_range.clone(),
                        definition: true,
                        import: false,
                        read: false,
                        write: false,
                    },
                ],
            });
            fixture.relations.push(RelationOutput {
                from: caller,
                to: target,
                kind: "CALLS".to_string(),
                path: path.clone(),
                range: call_range,
                confidence: Some(1.0),
                strategy: Some("fixture-provider".to_string()),
            });
            fixture.coverage.push(FileCoverageOutput {
                language: ProgrammingLanguage::TypeScript.as_str().to_string(),
                path,
                status: "indexed",
                reason: None,
            });
        }
        let language = fixture.languages.first_mut().unwrap();
        language.name = spec.name.to_string();
        language.files_found = file_count;
        language.files_indexed = file_count;
        fixture.census = SourceCensus::scan(&fixture.project.root).unwrap();
        fixture.plan =
            plan_analysis_units(&fixture.project.root, &fixture.census.manifest).unwrap();
        fixture
    }

    fn new(languages: &[ProgrammingLanguage]) -> Self {
        let project = TestProject::new("language-ir-project");
        write_project_markers(&project);
        let providers = TestProject::new("language-ir-providers");
        write_provider_catalog(&providers);
        let artifacts = TestProject::new("language-ir-artifacts");

        let mut language_outputs = Vec::new();
        let mut coverage = Vec::new();
        let mut documents = Vec::new();
        let mut relations = Vec::new();
        for language in languages {
            let spec = LANGUAGES
                .iter()
                .find(|candidate| candidate.contract_language == *language)
                .unwrap();
            let path = format!("main.{}", fixture_extension(*language));
            let (source, target_kind, caller_kind) = fixture_definitions(*language);
            project.write(&path, source.as_bytes());
            let target_range = token_range(&source, "Target", 0);
            let caller_range = unique_token_range(&source, "Caller");
            let call_range = token_range(&source, "Target", 1);
            let target = format!("fixture {} Target#", language.as_str());
            let caller = format!("fixture {} Caller#call().", language.as_str());
            let container = matches!(
                language,
                ProgrammingLanguage::Java | ProgrammingLanguage::CSharp
            )
            .then(|| {
                (
                    format!("fixture {} Fixture#", language.as_str()),
                    unique_token_range(&source, "Fixture"),
                )
            });
            let enclosing_symbol = container.as_ref().map(|(symbol, _)| symbol.clone());
            let mut symbols = vec![
                SymbolOutput {
                    symbol: target.clone(),
                    kind: target_kind.to_string(),
                    display_name: Some("Target".to_string()),
                    documentation: Vec::new(),
                    signature: None,
                    enclosing_symbol: enclosing_symbol.clone(),
                },
                SymbolOutput {
                    symbol: caller.clone(),
                    kind: caller_kind.to_string(),
                    display_name: Some("Caller".to_string()),
                    documentation: Vec::new(),
                    signature: None,
                    enclosing_symbol,
                },
            ];
            let mut occurrences = vec![
                OccurrenceOutput {
                    symbol: target.clone(),
                    range: target_range.clone(),
                    enclosing_range: target_range,
                    definition: true,
                    import: false,
                    read: false,
                    write: false,
                },
                OccurrenceOutput {
                    symbol: caller.clone(),
                    range: caller_range.clone(),
                    enclosing_range: caller_range.clone(),
                    definition: true,
                    import: false,
                    read: false,
                    write: false,
                },
            ];
            if let Some((container_symbol, container_range)) = container {
                symbols.push(SymbolOutput {
                    symbol: container_symbol.clone(),
                    kind: "Class".to_string(),
                    display_name: Some("Fixture".to_string()),
                    documentation: Vec::new(),
                    signature: None,
                    enclosing_symbol: None,
                });
                occurrences.push(OccurrenceOutput {
                    symbol: container_symbol,
                    range: container_range.clone(),
                    enclosing_range: container_range,
                    definition: true,
                    import: false,
                    read: false,
                    write: false,
                });
            }
            documents.push(DocumentOutput {
                language: language.as_str().to_string(),
                path: path.clone(),
                symbols,
                occurrences,
            });
            relations.push(RelationOutput {
                from: caller,
                to: target,
                kind: "CALLS".to_string(),
                path: path.clone(),
                range: call_range,
                confidence: Some(1.0),
                strategy: Some("fixture-provider".to_string()),
            });
            coverage.push(FileCoverageOutput {
                language: language.as_str().to_string(),
                path,
                status: "indexed",
                reason: None,
            });
            language_outputs.push(LanguageOutput {
                id: language.as_str().to_string(),
                name: spec.name.to_string(),
                provider: match spec.provider {
                    ProviderKind::Scip => "scip",
                    ProviderKind::Lsp => "native-lsp",
                },
                files_found: 1,
                files_indexed: 1,
                files_excluded: 0,
                files_missing: 0,
                status: "indexed",
            });
        }
        let census = SourceCensus::scan(&project.root).unwrap();
        let plan = plan_analysis_units(&project.root, &census.manifest).unwrap();
        Self {
            project,
            providers,
            artifacts,
            census,
            plan,
            languages: language_outputs,
            coverage,
            documents,
            relations,
            diagnostics: Vec::new(),
        }
    }

    fn emit(&self) -> Result<super::direct::DirectLanguageIrEmission, String> {
        self.emit_with_coordinator(&[])
    }

    fn emit_with_coordinator(
        &self,
        coordinator_diagnostics: &[Diagnostic],
    ) -> Result<super::direct::DirectLanguageIrEmission, String> {
        let batches = self.provider_batches();
        self.emit_with_batches(batches, coordinator_diagnostics)
    }

    fn emit_with_batches(
        &self,
        batches: Vec<ProviderUnitBatch>,
        coordinator_diagnostics: &[Diagnostic],
    ) -> Result<super::direct::DirectLanguageIrEmission, String> {
        static ARTIFACT_COUNTER: AtomicU64 = AtomicU64::new(0);
        let discovered_files = self
            .coverage
            .iter()
            .map(|entry| (entry.language.clone(), self.project.root.join(&entry.path)))
            .collect::<Vec<_>>();
        let artifact_root = self.artifacts.root.join(format!(
            "run-{}",
            ARTIFACT_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        emit_direct_language_ir(DirectLanguageIrInput {
            project_root: &self.project.root,
            manifest: &self.census.manifest,
            plan: &self.plan,
            providers_root: Some(&self.providers.root),
            batches,
            discovered_files: &discovered_files,
            file_relations: &[],
            project_model_files: &[],
            coordinator_diagnostics,
            static_analyzer_set_digest: Sha256Digest::of_bytes(b"test-static-analyzer-set"),
            artifact_root: &artifact_root,
        })
    }

    fn provider_batches(&self) -> Vec<ProviderUnitBatch> {
        self.languages
            .iter()
            .map(|language| {
                let source_files = self
                    .coverage
                    .iter()
                    .filter(|coverage| coverage.language == language.id)
                    .map(|coverage| coverage.path.clone())
                    .collect::<Vec<_>>();
                let spec = LANGUAGES
                    .iter()
                    .find(|spec| spec.id == language.id)
                    .expect("closed language");
                let absolute_source_files = source_files
                    .iter()
                    .map(|path| self.project.root.join(path))
                    .collect::<Vec<_>>();
                ProviderUnitBatch {
                    language: language.clone(),
                    source_files,
                    execution_context: executed_provider_context(ExecutedProviderContextInput {
                        project_root: &self.project.root,
                        language: spec,
                        mode: ProviderExecutionMode::InferredWorkspace,
                        analysis_root: &self.project.root,
                        source_files: &absolute_source_files,
                        config_files: Vec::new(),
                        generated_context_digest: None,
                        dimensions: Vec::new(),
                    })
                    .expect("fixture execution context"),
                    documents: self
                        .documents
                        .iter()
                        .filter(|document| {
                            normalize_scip_language(&document.language, &language.id) == language.id
                        })
                        .cloned()
                        .collect(),
                    relations: self
                        .relations
                        .iter()
                        .filter(|relation| {
                            self.coverage.iter().any(|coverage| {
                                coverage.language == language.id && coverage.path == relation.path
                            })
                        })
                        .cloned()
                        .collect(),
                    diagnostics: self
                        .diagnostics
                        .iter()
                        .filter(|diagnostic| diagnostic.language == language.id)
                        .cloned()
                        .collect(),
                    project_excluded_files: 0,
                }
            })
            .collect()
    }
}

fn fixture_definitions(language: ProgrammingLanguage) -> (String, &'static str, &'static str) {
    match language {
        ProgrammingLanguage::TypeScript => (
            "function Target() {}\nfunction Caller() { Target(); }\n".to_string(),
            "Function",
            "Function",
        ),
        ProgrammingLanguage::JavaScript => (
            "function Target() {}\nfunction Caller() { Target(); }\n".to_string(),
            "Function",
            "Function",
        ),
        ProgrammingLanguage::Python => (
            "def Target():\n    pass\n\ndef Caller():\n    Target()\n".to_string(),
            "Function",
            "Function",
        ),
        ProgrammingLanguage::Java => (
            "class Fixture { static void Target() {} static void Caller() { Target(); } }\n"
                .to_string(),
            "Method",
            "Method",
        ),
        ProgrammingLanguage::CSharp => (
            "class Fixture { static void Target() {} static void Caller() { Target(); } }\n"
                .to_string(),
            "Method",
            "Method",
        ),
        ProgrammingLanguage::C => (
            "void Target(void) {}\nvoid Caller(void) { Target(); }\n".to_string(),
            "Function",
            "Function",
        ),
        ProgrammingLanguage::Cpp => (
            "void Target() {}\nvoid Caller() { Target(); }\n".to_string(),
            "Function",
            "Function",
        ),
        ProgrammingLanguage::Go => (
            "package fixture\nfunc Target() {}\nfunc Caller() { Target() }\n".to_string(),
            "Function",
            "Function",
        ),
        ProgrammingLanguage::Rust => (
            "fn Target() {}\nfn Caller() { Target(); }\n".to_string(),
            "Function",
            "Function",
        ),
        ProgrammingLanguage::Dart => (
            "void Target() {}\nvoid Caller() { Target(); }\n".to_string(),
            "Function",
            "Function",
        ),
    }
}

fn unique_token_range(source: &str, token: &str) -> Vec<i32> {
    let mut matches = source.match_indices(token);
    let (offset, _) = matches.next().expect("fixture token");
    assert!(matches.next().is_none(), "fixture token must be unique");
    let prefix = &source[..offset];
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count();
    let column = prefix
        .rfind('\n')
        .map(|newline| offset - newline - 1)
        .unwrap_or(offset);
    vec![
        line as i32,
        column as i32,
        line as i32,
        (column + token.len()) as i32,
    ]
}

fn token_range(source: &str, token: &str, ordinal: usize) -> Vec<i32> {
    let (offset, _) = source
        .match_indices(token)
        .nth(ordinal)
        .unwrap_or_else(|| panic!("fixture token {token} occurrence {ordinal}"));
    let prefix = &source[..offset];
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count();
    let column = prefix
        .rfind('\n')
        .map(|newline| offset - newline - 1)
        .unwrap_or(offset);
    vec![
        line as i32,
        column as i32,
        line as i32,
        (column + token.len()) as i32,
    ]
}

fn write_project_markers(project: &TestProject) {
    for (path, bytes) in [
        ("tsconfig.json", b"{}".as_slice()),
        ("pyproject.toml", b"[project]\nname='fixture'\n".as_slice()),
        ("pom.xml", b"<project/>\n".as_slice()),
        ("fixture.csproj", b"<Project/>\n".as_slice()),
        ("compile_flags.txt", b"-I.\n".as_slice()),
        ("go.mod", b"module fixture\n".as_slice()),
        ("Cargo.toml", b"[package]\nname='fixture'\n".as_slice()),
        ("pubspec.yaml", b"name: fixture\n".as_slice()),
    ] {
        project.write(path, bytes);
    }
}

fn write_provider_catalog(providers: &TestProject) {
    let mut entries = Vec::new();
    let commands = LANGUAGES
        .iter()
        .map(|language| language.tool)
        .chain(std::iter::once("clangd"))
        .collect::<BTreeSet<_>>();
    for command in &commands {
        let relative = format!("bin/{command}");
        let bytes = format!("provider fixture: {command}\n").into_bytes();
        providers.write(&relative, &bytes);
        entries.push(serde_json::json!({
            "command": command,
            "path": relative,
            "version": "fixture-1",
            "sha256": format!("{:x}", Sha256::digest(&bytes)),
        }));
    }
    providers.write(
        "manifest.json",
        serde_json::to_vec(&serde_json::json!({ "providers": entries }))
            .unwrap()
            .as_slice(),
    );
}

fn manifest_file<'a>(census: &'a SourceCensus, path: &str) -> &'a SourceManifestFile {
    let path = RepositoryPath::parse(path).unwrap();
    census
        .manifest
        .files
        .iter()
        .find(|file| file.path == path)
        .unwrap()
}

fn fixture_extension(language: ProgrammingLanguage) -> &'static str {
    match language {
        ProgrammingLanguage::TypeScript => "ts",
        ProgrammingLanguage::JavaScript => "js",
        ProgrammingLanguage::Python => "py",
        ProgrammingLanguage::Java => "java",
        ProgrammingLanguage::CSharp => "cs",
        ProgrammingLanguage::C => "c",
        ProgrammingLanguage::Cpp => "cpp",
        ProgrammingLanguage::Go => "go",
        ProgrammingLanguage::Rust => "rs",
        ProgrammingLanguage::Dart => "dart",
    }
}

fn all_languages() -> [ProgrammingLanguage; 10] {
    [
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
    ]
}

fn read_ir_records(path: &Path) -> Vec<LanguageIrRecord> {
    fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn write_ir_records(
    output: &TestProject,
    records: &[LanguageIrRecord],
) -> (PathBuf, Sha256Digest, u64) {
    let mut bytes = Vec::new();
    for record in records {
        serde_json::to_writer(&mut bytes, record).unwrap();
        bytes.push(b'\n');
    }
    let path = output.root.join("language-ir.jsonl");
    fs::write(&path, &bytes).unwrap();
    (path, Sha256Digest::of_bytes(&bytes), records.len() as u64)
}

struct TestProject {
    root: PathBuf,
}

impl TestProject {
    fn new(label: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let nonce = COUNTER.fetch_add(1, Ordering::Relaxed);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "codebase-workspace-{label}-{}-{nanos}-{nonce}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        Self { root }
    }

    fn write(&self, relative: impl AsRef<Path>, bytes: &[u8]) {
        let path = self.root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, bytes).unwrap();
    }
}

impl Drop for TestProject {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
