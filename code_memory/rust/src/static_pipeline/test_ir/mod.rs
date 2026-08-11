//! Typed test-case and test-to-production intermediate representation.
//!
//! The adapter consumes the sealed Language IR call graph plus exact
//! language/framework test syntax. It never links a test to production by
//! filename, name similarity, or directory proximity.

mod inventory;

#[cfg(test)]
mod tests;

use crate::static_pipeline::language_ir::artifact::visit_language_ir_records;
use crate::static_pipeline::language_ir::syntax::parse_tree;
use crate::static_pipeline::source_evidence::VerifiedSourceFile;
use codebase_fact_model::analysis::ProgrammingLanguage;
use codebase_fact_model::analysis_plan::AnalysisPlan;
use codebase_fact_model::coverage::{AnalysisCapability, AnalysisGap, AnalysisScope, GapCode};
use codebase_fact_model::evidence::{
    EvidenceKind, EvidenceLocation, EvidenceProducer, EvidenceProducerKind, FactEvidence,
};
use codebase_fact_model::identity::{
    AnalysisUnitId, EvidenceId, ProviderSymbolId, Sha256Digest, SnapshotId,
};
use codebase_fact_model::language_ir::{
    IrDefinition, IrEndpoint, IrRelation, LanguageIrRecord, LanguageRelationKind,
};
use codebase_fact_model::source::{RepositoryPath, SourceFlags, SourceSpan};
use codebase_fact_model::source_manifest::{SourceEntryState, SourceManifest};
use codebase_fact_model::validation::Validate;
use inventory::{inventory_test_cases, likely_contains_test_syntax};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

const TEST_IR_SCHEMA: &str = "codebase-workspace.test-ir.v1";
const TEST_ADAPTER_NAME: &str = "code-memory-exact-test-relation-adapter";
const TEST_ADAPTER_VERSION: &str = "1";
const TEST_ANALYZER_DIGEST_DOMAIN: &[u8] = b"codebase-workspace.test-analyzer.v1\0";

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TestCaseRecord {
    pub(crate) unit_id: AnalysisUnitId,
    pub(crate) language: ProgrammingLanguage,
    pub(crate) framework: String,
    pub(crate) native_kind: String,
    pub(crate) qualified_name: String,
    pub(crate) display_name: String,
    pub(crate) source_path: RepositoryPath,
    pub(crate) marker_evidence_id: EvidenceId,
    pub(crate) body_span: SourceSpan,
    pub(crate) flags: SourceFlags,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TestRelationRecord {
    pub(crate) unit_id: AnalysisUnitId,
    pub(crate) test_qualified_name: String,
    pub(crate) target_symbol_id: ProviderSymbolId,
    pub(crate) evidence_ids: Vec<EvidenceId>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TestUnitAudit {
    pub(crate) test_source_file_count: u64,
    pub(crate) inventory_failed_file_count: u64,
    pub(crate) detected_test_case_count: u64,
    pub(crate) linked_test_case_count: u64,
    pub(crate) logical_relation_count: u64,
    pub(crate) ignored_non_project_call_count: u64,
    pub(crate) ignored_test_helper_call_count: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TestIrReceipt {
    pub(crate) schema: &'static str,
    pub(crate) snapshot_id: SnapshotId,
    pub(crate) detected_test_case_count: u64,
    pub(crate) linked_test_case_count: u64,
    pub(crate) emitted_relation_count: u64,
    pub(crate) inventory_failed_file_count: u64,
    pub(crate) evidence_count: u64,
    pub(crate) gap_count: u64,
    pub(crate) content_digest: Sha256Digest,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TestIr {
    pub(crate) snapshot_id: SnapshotId,
    pub(crate) cases: Vec<TestCaseRecord>,
    pub(crate) relations: Vec<TestRelationRecord>,
    pub(crate) evidence: Vec<FactEvidence>,
    pub(crate) gaps: Vec<AnalysisGap>,
    pub(crate) unit_audit: BTreeMap<AnalysisUnitId, TestUnitAudit>,
    pub(crate) receipt: TestIrReceipt,
}

impl TestIr {
    pub(crate) fn empty(snapshot_id: &SnapshotId, plan: &AnalysisPlan) -> Self {
        let cases = Vec::new();
        let relations = Vec::new();
        let evidence = Vec::new();
        let gaps = Vec::new();
        let unit_audit = plan
            .units
            .iter()
            .map(|unit| (unit.id.clone(), TestUnitAudit::default()))
            .collect::<BTreeMap<_, _>>();
        let content_digest = test_ir_content_digest(
            snapshot_id,
            &cases,
            &relations,
            &evidence,
            &gaps,
            &unit_audit,
        )
        .expect("empty Test IR is serializable");
        Self {
            snapshot_id: snapshot_id.clone(),
            cases,
            relations,
            evidence,
            gaps,
            unit_audit,
            receipt: TestIrReceipt {
                schema: TEST_IR_SCHEMA,
                snapshot_id: snapshot_id.clone(),
                detected_test_case_count: 0,
                linked_test_case_count: 0,
                emitted_relation_count: 0,
                inventory_failed_file_count: 0,
                evidence_count: 0,
                gap_count: 0,
                content_digest,
            },
        }
    }
}

pub(crate) fn test_analyzer_digest() -> Sha256Digest {
    let mut hasher = Sha256::new();
    hasher.update(TEST_ANALYZER_DIGEST_DOMAIN);
    hash_component(&mut hasher, TEST_ADAPTER_NAME.as_bytes());
    hash_component(&mut hasher, TEST_ADAPTER_VERSION.as_bytes());
    Sha256Digest::of_bytes(&hasher.finalize())
}

pub(crate) fn combine_static_analyzer_digests(
    framework_digest: Sha256Digest,
    test_digest: Sha256Digest,
) -> Sha256Digest {
    let mut hasher = Sha256::new();
    hasher.update(b"codebase-workspace.static-analyzer-set.v1\0");
    hash_component(&mut hasher, framework_digest.to_string().as_bytes());
    hash_component(&mut hasher, test_digest.to_string().as_bytes());
    Sha256Digest::of_bytes(&hasher.finalize())
}

pub(crate) fn adapt_test_relations(
    project_root: &Path,
    manifest: &SourceManifest,
    plan: &AnalysisPlan,
    snapshot_id: &SnapshotId,
    language_ir_path: &Path,
) -> Result<TestIr, String> {
    manifest
        .validate()
        .map_err(|error| format!("invalid Source Manifest before Test IR: {error}"))?;
    plan.validate_against(manifest)
        .map_err(|error| format!("invalid Analysis Plan before Test IR: {error}"))?;

    let units = plan
        .units
        .iter()
        .map(|unit| (unit.id.clone(), unit))
        .collect::<BTreeMap<_, _>>();
    let manifest_files = manifest
        .files
        .iter()
        .map(|file| (file.path.clone(), file))
        .collect::<BTreeMap<_, _>>();
    let mut ir_snapshot_ids = BTreeSet::new();
    let mut language_evidence = BTreeMap::<EvidenceId, FactEvidence>::new();
    let mut definitions = Vec::<IrDefinition>::new();
    let mut call_relations = Vec::<IrRelation>::new();
    visit_language_ir_records(language_ir_path, |record| {
        match record {
            LanguageIrRecord::Header(header) => {
                ir_snapshot_ids.insert(header.snapshot_id.clone());
            }
            LanguageIrRecord::Evidence(item) => {
                language_evidence.insert(item.id.clone(), item);
            }
            LanguageIrRecord::Definition(item) => definitions.push(item),
            LanguageIrRecord::Relation(item)
                if matches!(
                    item.kind,
                    LanguageRelationKind::Calls | LanguageRelationKind::Constructs
                ) =>
            {
                call_relations.push(item)
            }
            _ => {}
        }
        Ok(())
    })?;
    if ir_snapshot_ids != BTreeSet::from([snapshot_id.clone()]) {
        return Err("Test IR input contains another Language IR snapshot".to_string());
    }

    let local_definitions = definitions
        .iter()
        .map(|definition| {
            (
                (definition.unit_id.clone(), definition.symbol_id.clone()),
                definition,
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut global_definitions = BTreeMap::<ProviderSymbolId, Vec<&IrDefinition>>::new();
    for definition in &definitions {
        global_definitions
            .entry(definition.symbol_id.clone())
            .or_default()
            .push(definition);
    }

    let mut evidence = BTreeMap::<EvidenceId, FactEvidence>::new();
    let mut gaps = Vec::new();
    let mut cases = Vec::new();
    let mut unit_audit = units
        .keys()
        .cloned()
        .map(|unit_id| (unit_id, TestUnitAudit::default()))
        .collect::<BTreeMap<_, _>>();

    for assignment in &plan.assignments {
        let manifest_file = manifest_files
            .get(&assignment.path)
            .copied()
            .ok_or_else(|| {
                format!(
                    "Test IR assignment is absent from Source Manifest: {}",
                    assignment.path
                )
            })?;
        if manifest_file.state != SourceEntryState::Included {
            continue;
        }
        let source = VerifiedSourceFile::load(project_root, manifest_file)?;
        let should_inventory = likely_contains_test_syntax(
            assignment.language,
            manifest_file.file_kind,
            source.text(),
        );
        for unit_id in &assignment.unit_ids {
            let audit = unit_audit
                .get_mut(unit_id)
                .ok_or_else(|| format!("Test IR assignment references unknown unit {unit_id}"))?;
            if manifest_file.file_kind == codebase_fact_model::source::SourceFileKind::Test {
                audit.test_source_file_count += 1;
            }
        }
        if !should_inventory {
            continue;
        }
        let tree = match parse_tree(
            assignment.language.as_str(),
            assignment.path.as_str(),
            source.text(),
            "test-case-inventory",
        ) {
            Ok(tree) => tree,
            Err(error) => {
                for unit_id in &assignment.unit_ids {
                    unit_audit
                        .get_mut(unit_id)
                        .expect("planned Test IR unit")
                        .inventory_failed_file_count += 1;
                    gaps.push(AnalysisGap {
                        code: GapCode::ProviderExecutionIncomplete,
                        scope: AnalysisScope::File {
                            unit_id: Some(unit_id.clone()),
                            path: assignment.path.clone(),
                        },
                        capability: Some(AnalysisCapability::TestRelations),
                        evidence_ids: Vec::new(),
                        message: format!(
                            "Exact test-case syntax inventory failed; no test relationship was guessed: {}",
                            bounded_message(&error)
                        ),
                    });
                }
                continue;
            }
        };
        let syntax_cases = inventory_test_cases(
            assignment.language,
            assignment.path.as_str(),
            manifest_file.file_kind,
            tree.root_node(),
            source.text(),
        );
        for unit_id in &assignment.unit_ids {
            let unit = units
                .get(unit_id)
                .ok_or_else(|| format!("Test IR case references unknown unit {unit_id}"))?;
            if unit.language != assignment.language {
                return Err(format!(
                    "Test IR assignment language differs from unit {unit_id}"
                ));
            }
            for syntax_case in &syntax_cases {
                // One unmappable range costs this test registration, never the
                // run: the file coverage below still reports what was scanned.
                let (Ok(marker_span), Ok(body_span)) = (
                    source.utf8_span(&syntax_case.marker_range),
                    source.utf8_span(&syntax_case.body_range),
                ) else {
                    continue;
                };
                let registration_evidence = FactEvidence::new(
                    EvidenceKind::FrameworkRegistration,
                    EvidenceProducer {
                        kind: EvidenceProducerKind::SyntaxParser,
                        name: TEST_ADAPTER_NAME.to_string(),
                        version: Some(TEST_ADAPTER_VERSION.to_string()),
                        strategy: Some("exact-test-registration".to_string()),
                    },
                    EvidenceLocation::Source {
                        span: marker_span.clone(),
                    },
                    None,
                )
                .map_err(|error| format!("cannot build test registration evidence: {error}"))?;
                let marker_evidence_id = registration_evidence.id.clone();
                evidence.insert(marker_evidence_id.clone(), registration_evidence);
                let qualified_name = format!(
                    "{}#test@{}:{}-{}:{}",
                    assignment.path.as_str(),
                    marker_span.start.line,
                    marker_span.start.utf8_column,
                    marker_span.end.line,
                    marker_span.end.utf8_column
                );
                cases.push(TestCaseRecord {
                    unit_id: unit_id.clone(),
                    language: assignment.language,
                    framework: syntax_case.framework.clone(),
                    native_kind: syntax_case.native_kind.clone(),
                    qualified_name,
                    display_name: syntax_case.display_name.clone(),
                    source_path: assignment.path.clone(),
                    marker_evidence_id,
                    body_span,
                    flags: SourceFlags {
                        test: true,
                        generated: manifest_file.file_kind
                            == codebase_fact_model::source::SourceFileKind::Generated,
                        vendor: manifest_file.file_kind
                            == codebase_fact_model::source::SourceFileKind::Vendor,
                        external: false,
                    },
                });
                unit_audit
                    .get_mut(unit_id)
                    .expect("planned Test IR unit")
                    .detected_test_case_count += 1;
            }
        }
    }
    cases.sort_by(|left, right| {
        (&left.unit_id, &left.qualified_name).cmp(&(&right.unit_id, &right.qualified_name))
    });
    cases.dedup_by(|left, right| {
        left.unit_id == right.unit_id && left.qualified_name == right.qualified_name
    });

    let cases_by_unit_path = cases.iter().enumerate().fold(
        BTreeMap::<(AnalysisUnitId, RepositoryPath), Vec<usize>>::new(),
        |mut index, (ordinal, case)| {
            index
                .entry((case.unit_id.clone(), case.source_path.clone()))
                .or_default()
                .push(ordinal);
            index
        },
    );
    let mut relation_evidence = BTreeMap::<(String, ProviderSymbolId), BTreeSet<EvidenceId>>::new();
    for relation in &call_relations {
        let IrEndpoint::NativeSymbol { symbol_id } = &relation.target else {
            continue;
        };
        let source_spans = relation
            .evidence_ids
            .iter()
            .filter_map(|id| language_evidence.get(id))
            .filter_map(|item| match &item.location {
                EvidenceLocation::Source { span } => Some(span),
                _ => None,
            })
            .collect::<Vec<_>>();
        for span in source_spans {
            let key = (relation.unit_id.clone(), span.path.clone());
            let Some(case_ordinals) = cases_by_unit_path.get(&key) else {
                continue;
            };
            let selected = case_ordinals
                .iter()
                .filter(|ordinal| span_contains(&cases[**ordinal].body_span, span))
                .min_by_key(|ordinal| span_size(&cases[**ordinal].body_span));
            let Some(selected) = selected else {
                continue;
            };
            let case = &cases[*selected];
            let target_definition = resolve_definition(
                &relation.unit_id,
                symbol_id,
                &local_definitions,
                &global_definitions,
            );
            let audit = unit_audit
                .get_mut(&relation.unit_id)
                .expect("Language IR relation unit is planned");
            let Some(target_definition) = target_definition else {
                audit.ignored_non_project_call_count += 1;
                continue;
            };
            if target_definition.flags.test {
                audit.ignored_test_helper_call_count += 1;
                continue;
            }
            let item = relation_evidence
                .entry((case.qualified_name.clone(), symbol_id.clone()))
                .or_default();
            item.insert(case.marker_evidence_id.clone());
            item.extend(relation.evidence_ids.iter().cloned());
        }
    }

    let mut relations = relation_evidence
        .into_iter()
        .map(|((test_qualified_name, target_symbol_id), evidence_ids)| {
            let case = cases
                .iter()
                .find(|case| case.qualified_name == test_qualified_name)
                .expect("relation source TestCase exists");
            TestRelationRecord {
                unit_id: case.unit_id.clone(),
                test_qualified_name,
                target_symbol_id,
                evidence_ids: evidence_ids.into_iter().collect(),
            }
        })
        .collect::<Vec<_>>();
    relations.sort_by(|left, right| {
        (
            &left.unit_id,
            &left.test_qualified_name,
            &left.target_symbol_id,
        )
            .cmp(&(
                &right.unit_id,
                &right.test_qualified_name,
                &right.target_symbol_id,
            ))
    });
    let linked_cases = relations
        .iter()
        .map(|relation| {
            (
                relation.unit_id.clone(),
                relation.test_qualified_name.clone(),
            )
        })
        .collect::<BTreeSet<_>>();
    for case in &cases {
        if linked_cases.contains(&(case.unit_id.clone(), case.qualified_name.clone())) {
            continue;
        }
        gaps.push(AnalysisGap {
            code: GapCode::UnresolvedTarget,
            scope: AnalysisScope::File {
                unit_id: Some(case.unit_id.clone()),
                path: case.source_path.clone(),
            },
            capability: Some(AnalysisCapability::TestRelations),
            evidence_ids: vec![case.marker_evidence_id.clone()],
            message: format!(
                "Test case '{}' has no exact direct call or construction edge to a project-local production symbol",
                case.display_name
            ),
        });
    }
    for (unit_id, audit) in &mut unit_audit {
        audit.linked_test_case_count = linked_cases
            .iter()
            .filter(|(candidate, _)| candidate == unit_id)
            .count() as u64;
        audit.logical_relation_count = relations
            .iter()
            .filter(|relation| &relation.unit_id == unit_id)
            .count() as u64;
    }
    canonicalize_gaps(&mut gaps);
    let evidence = evidence.into_values().collect::<Vec<_>>();
    let content_digest = test_ir_content_digest(
        snapshot_id,
        &cases,
        &relations,
        &evidence,
        &gaps,
        &unit_audit,
    )?;
    let detected_test_case_count = cases.len() as u64;
    let linked_test_case_count = linked_cases.len() as u64;
    let emitted_relation_count = relations.len() as u64;
    let inventory_failed_file_count = unit_audit
        .values()
        .map(|audit| audit.inventory_failed_file_count)
        .sum();
    let evidence_count = evidence.len() as u64;
    let gap_count = gaps.len() as u64;
    Ok(TestIr {
        snapshot_id: snapshot_id.clone(),
        cases,
        relations,
        evidence,
        gaps,
        unit_audit,
        receipt: TestIrReceipt {
            schema: TEST_IR_SCHEMA,
            snapshot_id: snapshot_id.clone(),
            detected_test_case_count,
            linked_test_case_count,
            emitted_relation_count,
            inventory_failed_file_count,
            evidence_count,
            gap_count,
            content_digest,
        },
    })
}

fn resolve_definition<'a>(
    unit_id: &AnalysisUnitId,
    symbol_id: &ProviderSymbolId,
    local: &'a BTreeMap<(AnalysisUnitId, ProviderSymbolId), &'a IrDefinition>,
    global: &'a BTreeMap<ProviderSymbolId, Vec<&'a IrDefinition>>,
) -> Option<&'a IrDefinition> {
    local
        .get(&(unit_id.clone(), symbol_id.clone()))
        .copied()
        .or_else(|| match global.get(symbol_id).map(Vec::as_slice) {
            Some([definition]) => Some(*definition),
            _ => None,
        })
}

fn span_contains(outer: &SourceSpan, inner: &SourceSpan) -> bool {
    outer.path == inner.path
        && outer.content_digest == inner.content_digest
        && outer.start.byte_offset <= inner.start.byte_offset
        && inner.end.byte_offset <= outer.end.byte_offset
}

fn span_size(span: &SourceSpan) -> u64 {
    span.end.byte_offset.saturating_sub(span.start.byte_offset)
}

fn canonicalize_gaps(gaps: &mut Vec<AnalysisGap>) {
    gaps.sort_by_key(gap_key);
    gaps.dedup_by(|left, right| gap_key(left) == gap_key(right));
}

fn gap_key(gap: &AnalysisGap) -> String {
    serde_json::to_string(gap).expect("validated Test IR gap is serializable")
}

fn bounded_message(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.chars().count() <= 500 {
        return trimmed.to_string();
    }
    let mut result = trimmed.chars().take(497).collect::<String>();
    result.push_str("...");
    result
}

fn test_ir_content_digest(
    snapshot_id: &SnapshotId,
    cases: &[TestCaseRecord],
    relations: &[TestRelationRecord],
    evidence: &[FactEvidence],
    gaps: &[AnalysisGap],
    unit_audit: &BTreeMap<AnalysisUnitId, TestUnitAudit>,
) -> Result<Sha256Digest, String> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct SemanticTestIr<'a> {
        schema: &'static str,
        snapshot_id: &'a SnapshotId,
        cases: &'a [TestCaseRecord],
        relations: &'a [TestRelationRecord],
        evidence: &'a [FactEvidence],
        gaps: &'a [AnalysisGap],
        unit_audit: &'a BTreeMap<AnalysisUnitId, TestUnitAudit>,
    }
    let bytes = serde_json::to_vec(&SemanticTestIr {
        schema: TEST_IR_SCHEMA,
        snapshot_id,
        cases,
        relations,
        evidence,
        gaps,
        unit_audit,
    })
    .map_err(|error| format!("cannot serialize Test IR digest input: {error}"))?;
    Ok(Sha256Digest::of_bytes(&bytes))
}

fn hash_component(hasher: &mut Sha256, value: &[u8]) {
    hasher.update((value.len() as u64).to_le_bytes());
    hasher.update(value);
}
