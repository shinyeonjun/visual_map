//! Typed, fail-closed framework boundary records.
//!
//! The framework analyzer emits raw source-backed candidates. This adapter is
//! the only path from those internal candidates into the
//! canonical Fact Graph; framework detection signals alone never become facts.

use crate::frameworks::Analysis;
use crate::static_pipeline::source_evidence::VerifiedSourceFile;
use crate::LANGUAGES;
use codebase_fact_model::analysis::ProgrammingLanguage;
use codebase_fact_model::analysis_plan::AnalysisPlan;
use codebase_fact_model::coverage::{AnalysisCapability, AnalysisGap, AnalysisScope, GapCode};
use codebase_fact_model::evidence::{
    EvidenceKind, EvidenceLocation, EvidenceProducer, EvidenceProducerKind, FactEvidence,
};
use codebase_fact_model::identity::{
    AnalysisUnitId, EvidenceId, ProviderSymbolId, Sha256Digest, SnapshotId,
};
use codebase_fact_model::source::{RepositoryPath, SourceFileKind, SourceFlags};
use codebase_fact_model::source_manifest::{SourceEntryState, SourceManifest, SourceManifestFile};
use codebase_fact_model::validation::Validate;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

const FRAMEWORK_IR_SCHEMA: &str = "codebase-workspace.framework-ir.v1";
const FRAMEWORK_ADAPTER_NAME: &str = "code-memory-framework-route-adapter";
const FRAMEWORK_ADAPTER_VERSION: &str = "1";
const FRAMEWORK_ANALYZER_DIGEST_DOMAIN: &[u8] = b"codebase-workspace.framework-analyzer-set.v1\0";

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FrameworkRouteRecord {
    pub(crate) unit_id: AnalysisUnitId,
    pub(crate) language: ProgrammingLanguage,
    pub(crate) framework: String,
    pub(crate) method: String,
    pub(crate) path: String,
    pub(crate) source_path: RepositoryPath,
    pub(crate) evidence_id: EvidenceId,
    pub(crate) handler_symbol_id: Option<ProviderSymbolId>,
    pub(crate) flags: SourceFlags,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FrameworkUnitAudit {
    pub(crate) candidate_count: u64,
    pub(crate) accepted_route_count: u64,
    pub(crate) rejected_route_count: u64,
    pub(crate) handler_reference_count: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FrameworkIrReceipt {
    pub(crate) schema: &'static str,
    pub(crate) snapshot_id: SnapshotId,
    /// Raw `HTTP_ROUTE` facts reported by the analyzer before exact
    /// duplicate removal or Analysis Plan expansion.
    #[serde(rename = "donorCandidateCount")]
    pub(crate) raw_candidate_count: u64,
    /// Unique unit-scoped registrations. Emitted plus rejected records must
    /// equal this denominator.
    pub(crate) planned_route_record_count: u64,
    pub(crate) emitted_route_record_count: u64,
    pub(crate) rejected_route_record_count: u64,
    pub(crate) handler_reference_count: u64,
    pub(crate) evidence_count: u64,
    pub(crate) gap_count: u64,
    pub(crate) content_digest: Sha256Digest,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FrameworkIr {
    pub(crate) snapshot_id: SnapshotId,
    pub(crate) routes: Vec<FrameworkRouteRecord>,
    pub(crate) evidence: Vec<FactEvidence>,
    pub(crate) gaps: Vec<AnalysisGap>,
    pub(crate) unit_audit: BTreeMap<AnalysisUnitId, FrameworkUnitAudit>,
    pub(crate) receipt: FrameworkIrReceipt,
}

impl FrameworkIr {
    /// Empty execution used by language-only linker tests. Production index
    /// always supplies the executed framework adapter result.
    pub(crate) fn empty(snapshot_id: &SnapshotId, plan: &AnalysisPlan) -> Self {
        let routes = Vec::new();
        let evidence = Vec::new();
        let gaps = Vec::new();
        let content_digest = framework_ir_content_digest(snapshot_id, &routes, &evidence, &gaps)
            .expect("empty Framework IR is serializable");
        Self {
            snapshot_id: snapshot_id.clone(),
            routes,
            evidence,
            gaps,
            unit_audit: plan
                .units
                .iter()
                .map(|unit| (unit.id.clone(), FrameworkUnitAudit::default()))
                .collect(),
            receipt: FrameworkIrReceipt {
                schema: FRAMEWORK_IR_SCHEMA,
                snapshot_id: snapshot_id.clone(),
                raw_candidate_count: 0,
                planned_route_record_count: 0,
                emitted_route_record_count: 0,
                rejected_route_record_count: 0,
                handler_reference_count: 0,
                evidence_count: 0,
                gap_count: 0,
                content_digest,
            },
        }
    }
}

/// Hashes the closed framework pack input plus the adapter contract version.
/// The result joins the provider-set digest before snapshot identity is built,
/// so changing framework rules cannot silently reuse an old snapshot ID.
pub(crate) fn framework_analyzer_set_digest(pack_root: &Path) -> Result<Sha256Digest, String> {
    let framework_root = pack_root.join("packs").join("framework");
    let mut files = Vec::new();
    collect_json_files(&framework_root, &framework_root, &mut files)?;
    files.sort_by(|left, right| left.0.cmp(&right.0));
    let mut hasher = Sha256::new();
    hasher.update(FRAMEWORK_ANALYZER_DIGEST_DOMAIN);
    hash_component(&mut hasher, FRAMEWORK_ADAPTER_NAME.as_bytes());
    hash_component(&mut hasher, FRAMEWORK_ADAPTER_VERSION.as_bytes());
    for (relative, absolute) in files {
        hash_component(&mut hasher, relative.as_bytes());
        let bytes = fs::read(&absolute).map_err(|error| {
            format!("cannot hash framework pack {}: {error}", absolute.display())
        })?;
        hash_component(&mut hasher, &bytes);
    }
    Ok(Sha256Digest::of_bytes(&hasher.finalize()))
}

pub(crate) fn adapt_framework_routes(
    project_root: &Path,
    manifest: &SourceManifest,
    plan: &AnalysisPlan,
    snapshot_id: &SnapshotId,
    analysis: &Analysis,
) -> Result<FrameworkIr, String> {
    manifest
        .validate()
        .map_err(|error| format!("invalid Source Manifest before framework adapter: {error}"))?;
    plan.validate_against(manifest)
        .map_err(|error| format!("invalid Analysis Plan before framework adapter: {error}"))?;

    let manifest_files = manifest
        .files
        .iter()
        .map(|file| (file.path.clone(), file))
        .collect::<BTreeMap<_, _>>();
    let assignments = plan
        .assignments
        .iter()
        .map(|assignment| {
            (
                (assignment.path.clone(), assignment.language),
                assignment.unit_ids.clone(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut sources = BTreeMap::<RepositoryPath, VerifiedSourceFile>::new();
    let mut evidence = BTreeMap::<EvidenceId, FactEvidence>::new();
    let mut routes = BTreeMap::<RouteRecordKey, FrameworkRouteRecord>::new();
    let mut gaps = Vec::new();
    let mut unit_audit = plan
        .units
        .iter()
        .map(|unit| (unit.id.clone(), FrameworkUnitAudit::default()))
        .collect::<BTreeMap<_, _>>();
    let mut raw_candidate_count = 0_u64;

    for framework in &analysis.frameworks {
        let language = language_from_id(&framework.language).ok_or_else(|| {
            format!(
                "framework {} used unsupported language {}",
                framework.id, framework.language
            )
        })?;
        for fact in framework
            .facts
            .iter()
            .filter(|fact| fact.kind == "HTTP_ROUTE")
        {
            raw_candidate_count += 1;
            let source_path = RepositoryPath::parse(fact.source_file.clone()).map_err(|error| {
                format!(
                    "framework {} returned invalid route path {}: {error}",
                    framework.id, fact.source_file
                )
            })?;
            let manifest_file = manifest_files.get(&source_path).ok_or_else(|| {
                format!(
                    "framework {} returned a route outside Source Census: {}",
                    framework.id, source_path
                )
            })?;
            validate_source_owner(manifest_file, language, &framework.id)?;
            let unit_ids = assignments
                .get(&(source_path.clone(), language))
                .ok_or_else(|| {
                    format!(
                        "framework route has no Analysis Plan owner: {}/{}",
                        language.as_str(),
                        source_path
                    )
                })?;
            if !sources.contains_key(&source_path) {
                sources.insert(
                    source_path.clone(),
                    VerifiedSourceFile::load(project_root, manifest_file)?,
                );
            }
            let source = sources.get(&source_path).expect("verified source cache");
            let span = route_span(source, fact.source_line, fact.source_end_line)?;
            let route_evidence = FactEvidence::new(
                EvidenceKind::FrameworkRegistration,
                EvidenceProducer {
                    kind: EvidenceProducerKind::FrameworkAdapter,
                    name: FRAMEWORK_ADAPTER_NAME.to_string(),
                    version: Some(FRAMEWORK_ADAPTER_VERSION.to_string()),
                    strategy: Some(framework.id.clone()),
                },
                EvidenceLocation::Source { span },
                Some(format!(
                    "{} route registration ({})",
                    framework.id,
                    fact.evidence.join(", ")
                )),
            )
            .map_err(|error| format!("invalid framework route evidence: {error}"))?;
            evidence
                .entry(route_evidence.id.clone())
                .or_insert_with(|| route_evidence.clone());

            let method = normalize_http_method(fact.method.as_deref());
            let route_path = validate_route_path(fact.path.as_deref());
            let (method, route_path) = match (method, route_path) {
                (Ok(method), Ok(path)) => (method, path),
                (method, path) => {
                    let method_reason = method
                        .as_ref()
                        .err()
                        .cloned()
                        .unwrap_or_else(|| "valid".to_string());
                    let path_reason = path
                        .as_ref()
                        .err()
                        .cloned()
                        .unwrap_or_else(|| "valid".to_string());
                    for unit_id in unit_ids {
                        gaps.push(AnalysisGap {
                            code: GapCode::RuntimeRegistration,
                            scope: AnalysisScope::File {
                                unit_id: Some(unit_id.clone()),
                                path: source_path.clone(),
                            },
                            capability: Some(AnalysisCapability::FrameworkBindings),
                            evidence_ids: vec![route_evidence.id.clone()],
                            message: format!(
                                "Route registration was not a static method/path pair: method={} path={}",
                                method_reason, path_reason
                            ),
                        });
                    }
                    continue;
                }
            };
            let handler_symbol_id = fact
                .symbol
                .as_ref()
                .map(|symbol| ProviderSymbolId::parse(symbol.clone()))
                .transpose()
                .map_err(|error| {
                    format!(
                        "framework {} returned invalid provider symbol for {} {}: {error}",
                        framework.id, method, route_path
                    )
                })?;

            for unit_id in unit_ids {
                let record = FrameworkRouteRecord {
                    unit_id: unit_id.clone(),
                    language,
                    framework: framework.id.clone(),
                    method: method.clone(),
                    path: route_path.clone(),
                    source_path: source_path.clone(),
                    evidence_id: route_evidence.id.clone(),
                    handler_symbol_id: handler_symbol_id.clone(),
                    flags: flags_for_file_kind(manifest_file.file_kind),
                };
                routes
                    .entry(RouteRecordKey::from(&record))
                    .or_insert(record);
            }
        }
    }

    let routes = routes.into_values().collect::<Vec<_>>();
    let evidence = evidence.into_values().collect::<Vec<_>>();
    canonicalize_gaps(&mut gaps);
    // Audit only the canonicalized unit-scoped records. The analyzer can report
    // the same registration more than once; those duplicates must not inflate
    // either the capability denominator or its covered count.
    for route in &routes {
        let audit = unit_audit
            .get_mut(&route.unit_id)
            .expect("Analysis Plan unit audit");
        audit.candidate_count += 1;
        audit.accepted_route_count += 1;
        if route.handler_symbol_id.is_some() {
            audit.handler_reference_count += 1;
        }
    }
    for gap in &gaps {
        if gap.code != GapCode::RuntimeRegistration
            || gap.capability != Some(AnalysisCapability::FrameworkBindings)
        {
            continue;
        }
        let unit_id = gap
            .scope
            .unit_id()
            .ok_or_else(|| "framework route rejection has no Analysis Plan unit".to_string())?;
        let audit = unit_audit.get_mut(unit_id).ok_or_else(|| {
            format!("framework route rejection references unknown unit {unit_id}")
        })?;
        audit.candidate_count += 1;
        audit.rejected_route_count += 1;
    }
    let planned_route_record_count = unit_audit
        .values()
        .map(|audit| audit.candidate_count)
        .sum::<u64>();
    let rejected_route_record_count = unit_audit
        .values()
        .map(|audit| audit.rejected_route_count)
        .sum::<u64>();
    if planned_route_record_count != routes.len() as u64 + rejected_route_record_count {
        return Err("framework route candidate accounting is inconsistent".to_string());
    }
    for route in &routes {
        if evidence
            .binary_search_by(|item| item.id.cmp(&route.evidence_id))
            .is_err()
        {
            return Err(format!(
                "framework route references missing evidence {}",
                route.evidence_id
            ));
        }
    }
    let content_digest = framework_ir_content_digest(snapshot_id, &routes, &evidence, &gaps)?;
    let handler_reference_count = routes
        .iter()
        .filter(|route| route.handler_symbol_id.is_some())
        .count() as u64;
    let receipt = FrameworkIrReceipt {
        schema: FRAMEWORK_IR_SCHEMA,
        snapshot_id: snapshot_id.clone(),
        raw_candidate_count,
        planned_route_record_count,
        emitted_route_record_count: routes.len() as u64,
        rejected_route_record_count,
        handler_reference_count,
        evidence_count: evidence.len() as u64,
        gap_count: gaps.len() as u64,
        content_digest,
    };
    Ok(FrameworkIr {
        snapshot_id: snapshot_id.clone(),
        routes,
        evidence,
        gaps,
        unit_audit,
        receipt,
    })
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct RouteRecordKey {
    unit_id: AnalysisUnitId,
    framework: String,
    method: String,
    path: String,
    evidence_id: EvidenceId,
    handler_symbol_id: Option<ProviderSymbolId>,
}

impl From<&FrameworkRouteRecord> for RouteRecordKey {
    fn from(record: &FrameworkRouteRecord) -> Self {
        Self {
            unit_id: record.unit_id.clone(),
            framework: record.framework.clone(),
            method: record.method.clone(),
            path: record.path.clone(),
            evidence_id: record.evidence_id.clone(),
            handler_symbol_id: record.handler_symbol_id.clone(),
        }
    }
}

fn validate_source_owner(
    file: &SourceManifestFile,
    language: ProgrammingLanguage,
    framework: &str,
) -> Result<(), String> {
    if file.state != SourceEntryState::Included
        || !file.languages.contains(&language)
        || file.content_digest.is_none()
    {
        return Err(format!(
            "framework {framework} route source is not an included {language:?} census file: {}",
            file.path
        ));
    }
    Ok(())
}

fn route_span(
    source: &VerifiedSourceFile,
    source_line: usize,
    source_end_line: usize,
) -> Result<codebase_fact_model::source::SourceSpan, String> {
    if source_line == 0 || source_end_line < source_line {
        return Err("framework route used an invalid one-based line range".to_string());
    }
    source.whole_lines_span(source_line - 1, source_end_line - 1)
}

fn normalize_http_method(value: Option<&str>) -> Result<String, String> {
    let value = value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "missing".to_string())?
        .to_ascii_uppercase();
    if value.len() > 32
        || !value.bytes().all(|byte| {
            byte.is_ascii_uppercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
        })
    {
        return Err("non-literal-or-invalid-token".to_string());
    }
    Ok(value)
}

fn validate_route_path(value: Option<&str>) -> Result<String, String> {
    let value = value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "missing".to_string())?;
    if value.len() > 2048 || !value.starts_with('/') || value.chars().any(char::is_control) {
        return Err("non-literal-or-invalid-absolute-path".to_string());
    }
    Ok(value.to_string())
}

fn language_from_id(value: &str) -> Option<ProgrammingLanguage> {
    LANGUAGES
        .iter()
        .find(|language| language.id == value)
        .map(|language| language.contract_language)
}

fn flags_for_file_kind(kind: SourceFileKind) -> SourceFlags {
    SourceFlags {
        test: kind == SourceFileKind::Test,
        generated: kind == SourceFileKind::Generated,
        vendor: kind == SourceFileKind::Vendor,
        external: false,
    }
}

fn canonicalize_gaps(gaps: &mut Vec<AnalysisGap>) {
    for gap in gaps.iter_mut() {
        gap.evidence_ids.sort();
        gap.evidence_ids.dedup();
    }
    gaps.sort_by(|left, right| {
        (
            left.code,
            scope_key(&left.scope),
            left.capability,
            &left.evidence_ids,
        )
            .cmp(&(
                right.code,
                scope_key(&right.scope),
                right.capability,
                &right.evidence_ids,
            ))
    });
    gaps.dedup_by(|left, right| {
        left.code == right.code
            && left.scope == right.scope
            && left.capability == right.capability
            && left.evidence_ids == right.evidence_ids
    });
}

fn scope_key(scope: &AnalysisScope) -> String {
    serde_json::to_string(scope).unwrap_or_default()
}

fn framework_ir_content_digest(
    snapshot_id: &SnapshotId,
    routes: &[FrameworkRouteRecord],
    evidence: &[FactEvidence],
    gaps: &[AnalysisGap],
) -> Result<Sha256Digest, String> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct SemanticFrameworkIr<'a> {
        schema: &'static str,
        snapshot_id: &'a SnapshotId,
        routes: &'a [FrameworkRouteRecord],
        evidence: &'a [FactEvidence],
        gaps: &'a [AnalysisGap],
    }
    let bytes = serde_json::to_vec(&SemanticFrameworkIr {
        schema: FRAMEWORK_IR_SCHEMA,
        snapshot_id,
        routes,
        evidence,
        gaps,
    })
    .map_err(|error| format!("cannot serialize Framework IR: {error}"))?;
    Ok(Sha256Digest::of_bytes(&bytes))
}

fn collect_json_files(
    root: &Path,
    current: &Path,
    files: &mut Vec<(String, PathBuf)>,
) -> Result<(), String> {
    let metadata = fs::symlink_metadata(current).map_err(|error| {
        format!(
            "cannot inspect framework analyzer input {}: {error}",
            current.display()
        )
    })?;
    if metadata.file_type().is_symlink() {
        return Err(format!(
            "framework analyzer input may not contain symlinks: {}",
            current.display()
        ));
    }
    if metadata.is_file() {
        if current.extension().and_then(|value| value.to_str()) == Some("json") {
            let relative = current
                .strip_prefix(root)
                .map_err(|error| format!("cannot relativize framework pack path: {error}"))?
                .to_string_lossy()
                .replace('\\', "/");
            files.push((relative, current.to_path_buf()));
        }
        return Ok(());
    }
    let mut children = fs::read_dir(current)
        .map_err(|error| {
            format!(
                "cannot read framework analyzer input {}: {error}",
                current.display()
            )
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("cannot enumerate framework analyzer input: {error}"))?;
    children.sort_by_key(|entry| entry.file_name());
    for child in children {
        collect_json_files(root, &child.path(), files)?;
    }
    Ok(())
}

fn hash_component(hasher: &mut Sha256, value: &[u8]) {
    hasher.update((value.len() as u64).to_be_bytes());
    hasher.update(value);
}
