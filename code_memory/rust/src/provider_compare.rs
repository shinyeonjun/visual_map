//! Evidence-level comparison for a candidate SCIP provider.
//!
//! Raw provider symbol strings are intentionally not compared: two correct
//! providers may encode the same definition with different symbol schemes.
//! Workspace facts are instead rebased to exact definition path/range
//! locators, then compared as deterministic sets.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use codebase_fact_model::analysis::ProviderProtocol;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    allowed_document_paths, collect_files, read_scip, required_path, resolve_output_path,
    source_exclusion_reason, write_json, DocumentOutput, OccurrenceOutput, RelationOutput,
    LANGUAGES,
};

const REPORT_SCHEMA: &str = "code-memory.provider-shadow-comparison.v2";
const MAX_DIFF_SAMPLES: usize = 100;

#[derive(Deserialize)]
struct BaselineIndex {
    #[allow(dead_code)]
    schema: String,
    documents: Vec<DocumentOutput>,
    relations: Vec<RelationOutput>,
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase")]
struct DefinitionLocator {
    path: String,
    range: Vec<i32>,
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase")]
struct OccurrenceFact {
    path: String,
    range: Vec<i32>,
    target: DefinitionLocator,
    definition: bool,
    import: bool,
    read: bool,
    write: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase")]
struct RelationFact {
    kind: String,
    path: String,
    range: Vec<i32>,
    source: DefinitionLocator,
    target: DefinitionLocator,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ComparisonReport {
    schema: &'static str,
    language: String,
    project_root: String,
    baseline_path: String,
    baseline_sha256: String,
    candidate_path: String,
    candidate_sha256: String,
    expected_file_count: usize,
    baseline: ProviderSummary,
    candidate: ProviderSummary,
    file_coverage: FileCoverageComparison,
    definitions: SetAgreement<DefinitionLocator>,
    workspace_occurrences: SetAgreement<OccurrenceFact>,
    workspace_relations: SetAgreement<RelationFact>,
    regression: RegressionSummary,
    evaluation: EvaluationDecision,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProviderSummary {
    semantic_fact_digest: String,
    document_count: usize,
    symbol_count: usize,
    occurrence_count: usize,
    definition_count: usize,
    relation_count: usize,
    relation_kind_counts: BTreeMap<String, usize>,
    unique_workspace_symbol_count: usize,
    ambiguous_workspace_symbol_count: usize,
    external_or_unresolved_occurrence_count: usize,
    external_or_unresolved_relation_count: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FileCoverageComparison {
    baseline_document_count: usize,
    candidate_document_count: usize,
    baseline_missing_count: usize,
    candidate_missing_count: usize,
    baseline_missing_sample: Vec<String>,
    candidate_missing_sample: Vec<String>,
    samples_truncated: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SetAgreement<T: Serialize> {
    baseline_count: usize,
    candidate_count: usize,
    intersection_count: usize,
    baseline_only_count: usize,
    candidate_only_count: usize,
    agreement_precision: f64,
    agreement_recall: f64,
    agreement_f1: f64,
    baseline_only_sample: Vec<T>,
    candidate_only_sample: Vec<T>,
    samples_truncated: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BaselineCoverage<T: Serialize> {
    baseline_count: usize,
    covered_count: usize,
    regression_count: usize,
    regression_sample: Vec<T>,
    samples_truncated: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RegressionSummary {
    definitions: BaselineCoverage<DefinitionLocator>,
    workspace_occurrences: BaselineCoverage<OccurrenceFact>,
    workspace_relations: BaselineCoverage<RelationFact>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EvaluationDecision {
    regression_gate_passed: bool,
    eligible_for_ground_truth_evaluation: bool,
    candidate_extensions_require_review: bool,
    production_eligible: bool,
    blockers: Vec<String>,
    required_next_gates: Vec<&'static str>,
    note: &'static str,
}

struct NormalizedProvider {
    document_paths: BTreeSet<String>,
    definitions: BTreeSet<DefinitionLocator>,
    occurrences: BTreeSet<OccurrenceFact>,
    relations: BTreeSet<RelationFact>,
    summary: ProviderSummary,
}

pub(crate) fn compare_scip(args: &[String]) -> Result<(), String> {
    let root = canonical_existing_dir(&required_path(args, "--root")?)?;
    let baseline_path = canonical_existing_file(&required_path(args, "--baseline")?)?;
    let candidate_path = canonical_existing_file(&required_path(args, "--candidate")?)?;
    let out = resolve_output_path(required_path(args, "--out")?)?;
    let language = option_value(args, "--language").unwrap_or_else(|| "python".to_string());
    let spec = LANGUAGES
        .iter()
        .find(|spec| spec.id == language)
        .copied()
        .ok_or_else(|| format!("unsupported --language '{language}'"))?;

    let expected_files = collect_files(&root, spec.extensions)
        .into_iter()
        .filter(|path| source_exclusion_reason(path).is_none())
        .collect::<Vec<_>>();
    if expected_files.is_empty() {
        return Err(format!(
            "no admitted {} source files were found under {}",
            spec.name,
            root.display()
        ));
    }
    let allowed_paths = allowed_document_paths(&root, &expected_files);

    let baseline: BaselineIndex = serde_json::from_slice(
        &fs::read(&baseline_path)
            .map_err(|error| format!("cannot read {}: {error}", baseline_path.display()))?,
    )
    .map_err(|error| {
        format!(
            "invalid baseline index {}: {error}",
            baseline_path.display()
        )
    })?;
    let baseline_documents = baseline
        .documents
        .into_iter()
        .filter(|document| document.language == spec.id && allowed_paths.contains(&document.path))
        .collect::<Vec<_>>();
    let baseline_paths = baseline_documents
        .iter()
        .map(|document| document.path.clone())
        .collect::<HashSet<_>>();
    let baseline_relations = baseline
        .relations
        .into_iter()
        .filter(|relation| baseline_paths.contains(&relation.path))
        .collect::<Vec<_>>();

    // scip-typescript does not mark call occurrences directly. The production
    // pipeline classifies those occurrences with project-model call-site
    // ranges before `read_scip` turns them into CALLS/CONSTRUCTS relations.
    // Reuse only the baseline call-site coordinates here (never endpoints) so
    // a same-provider shadow run is normalized by the same source evidence.
    // A candidate still has to contain a resolved occurrence at that exact
    // range; these coordinates cannot manufacture a missing target.
    let reference_call_ranges = reference_call_ranges(spec.id, &baseline_relations);

    let (candidate_documents, candidate_relations) = read_scip(
        &candidate_path,
        spec.id,
        ProviderProtocol::Scip,
        &root,
        &allowed_paths,
        reference_call_ranges.as_ref(),
    )?;

    let baseline_normalized = normalize_provider(baseline_documents, baseline_relations);
    let candidate_normalized = normalize_provider(candidate_documents, candidate_relations);
    let expected_paths = allowed_paths.into_iter().collect::<BTreeSet<_>>();
    let baseline_missing = expected_paths
        .difference(&baseline_normalized.document_paths)
        .cloned()
        .collect::<Vec<_>>();
    let candidate_missing = expected_paths
        .difference(&candidate_normalized.document_paths)
        .cloned()
        .collect::<Vec<_>>();
    let file_samples_truncated =
        baseline_missing.len() > MAX_DIFF_SAMPLES || candidate_missing.len() > MAX_DIFF_SAMPLES;

    let definition_agreement = compare_sets(
        &baseline_normalized.definitions,
        &candidate_normalized.definitions,
    );
    let occurrence_agreement = compare_sets(
        &baseline_normalized.occurrences,
        &candidate_normalized.occurrences,
    );
    let relation_agreement = compare_sets(
        &baseline_normalized.relations,
        &candidate_normalized.relations,
    );

    let definition_regression = baseline_set_coverage(
        &baseline_normalized.definitions,
        &candidate_normalized.definitions,
    );
    let occurrence_regression = occurrence_baseline_coverage(
        &baseline_normalized.occurrences,
        &candidate_normalized.occurrences,
    );
    let relation_regression = baseline_set_coverage(
        &baseline_normalized.relations,
        &candidate_normalized.relations,
    );

    let mut blockers = Vec::new();
    if !candidate_missing.is_empty() {
        blockers.push(format!(
            "candidate omitted {} admitted source documents",
            candidate_missing.len()
        ));
    }
    append_regression_blocker(&mut blockers, "definitions", &definition_regression);
    append_regression_blocker(
        &mut blockers,
        "workspace occurrences",
        &occurrence_regression,
    );
    append_regression_blocker(&mut blockers, "workspace relations", &relation_regression);
    let candidate_extensions_require_review = definition_agreement.candidate_only_count > 0
        || occurrence_agreement.candidate_only_count > 0
        || relation_agreement.candidate_only_count > 0;
    let regression_gate_passed = blockers.is_empty();

    let report = ComparisonReport {
        schema: REPORT_SCHEMA,
        language,
        project_root: root.to_string_lossy().into_owned(),
        baseline_path: baseline_path.to_string_lossy().into_owned(),
        baseline_sha256: sha256_file(&baseline_path)?,
        candidate_path: candidate_path.to_string_lossy().into_owned(),
        candidate_sha256: sha256_file(&candidate_path)?,
        expected_file_count: expected_paths.len(),
        baseline: baseline_normalized.summary,
        candidate: candidate_normalized.summary,
        file_coverage: FileCoverageComparison {
            baseline_document_count: baseline_normalized.document_paths.len(),
            candidate_document_count: candidate_normalized.document_paths.len(),
            baseline_missing_count: baseline_missing.len(),
            candidate_missing_count: candidate_missing.len(),
            baseline_missing_sample: take_sample(baseline_missing),
            candidate_missing_sample: take_sample(candidate_missing),
            samples_truncated: file_samples_truncated,
        },
        definitions: definition_agreement,
        workspace_occurrences: occurrence_agreement,
        workspace_relations: relation_agreement,
        regression: RegressionSummary {
            definitions: definition_regression,
            workspace_occurrences: occurrence_regression,
            workspace_relations: relation_regression,
        },
        evaluation: EvaluationDecision {
            regression_gate_passed,
            eligible_for_ground_truth_evaluation: regression_gate_passed,
            candidate_extensions_require_review,
            // A provider-to-provider comparison cannot establish absolute
            // correctness. Promotion is deliberately impossible until the
            // independent gates below have been run and recorded.
            production_eligible: false,
            blockers,
            required_next_gates: vec![
                "human-reviewed ground-truth corpus",
                "canonical Language IR and Fact Bundle parity",
                "candidate-only evidence review",
                "determinism, packaging, security, and performance receipts",
            ],
            note: "The regression gate checks preservation of current local facts. Candidate-only facts are not failures, but they are not trusted automatically. Production promotion requires independent ground truth and canonical-output validation.",
        },
    };

    if let Some(parent) = out.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
    }
    let file = fs::File::create(&out)
        .map_err(|error| format!("cannot write {}: {error}", out.display()))?;
    let mut writer = BufWriter::new(file);
    write_json(&mut writer, &report)
        .map_err(|error| format!("cannot serialize comparison report: {error}"))?;
    writer
        .flush()
        .map_err(|error| format!("cannot flush {}: {error}", out.display()))?;
    println!("wrote {}", out.display());
    println!(
        "provider-comparison language={} files={}/{} definitions_f1={:.6} occurrences_f1={:.6} relations_f1={:.6} regression_gate_passed={} production_eligible={}",
        report.language,
        report.file_coverage.candidate_document_count,
        report.expected_file_count,
        report.definitions.agreement_f1,
        report.workspace_occurrences.agreement_f1,
        report.workspace_relations.agreement_f1,
        report.evaluation.regression_gate_passed,
        report.evaluation.production_eligible
    );
    Ok(())
}

fn reference_call_ranges(
    language: &str,
    relations: &[RelationOutput],
) -> Option<HashMap<String, Vec<Vec<i32>>>> {
    if !matches!(language, "typescript" | "javascript") {
        return None;
    }
    let mut ranges = HashMap::<String, Vec<Vec<i32>>>::new();
    for relation in relations.iter().filter(|relation| {
        matches!(relation.kind.as_str(), "CALLS" | "CONSTRUCTS")
            && !relation.path.is_empty()
            && relation.range.len() >= 3
    }) {
        ranges
            .entry(relation.path.clone())
            .or_default()
            .push(relation.range.clone());
    }
    for values in ranges.values_mut() {
        values.sort();
        values.dedup();
    }
    Some(ranges)
}

fn normalize_provider(
    documents: Vec<DocumentOutput>,
    relations: Vec<RelationOutput>,
) -> NormalizedProvider {
    let mut definitions_by_symbol = HashMap::<String, BTreeSet<DefinitionLocator>>::new();
    let mut document_paths = BTreeSet::new();
    let mut symbol_count = 0usize;
    let mut occurrence_count = 0usize;
    let mut definition_count = 0usize;
    for document in &documents {
        document_paths.insert(document.path.clone());
        symbol_count += document.symbols.len();
        occurrence_count += document.occurrences.len();
        for occurrence in document
            .occurrences
            .iter()
            .filter(|occurrence| occurrence.definition && !occurrence.symbol.is_empty())
        {
            definition_count += 1;
            definitions_by_symbol
                .entry(occurrence.symbol.clone())
                .or_default()
                .insert(DefinitionLocator {
                    path: document.path.clone(),
                    range: canonical_range(&occurrence.range),
                });
        }
    }
    let unique_definitions = definitions_by_symbol
        .iter()
        .filter(|(_, definitions)| definitions.len() == 1)
        .map(|(symbol, definitions)| (symbol.clone(), definitions.iter().next().unwrap().clone()))
        .collect::<HashMap<_, _>>();
    let ambiguous_workspace_symbol_count = definitions_by_symbol
        .values()
        .filter(|definitions| definitions.len() > 1)
        .count();
    let definitions = definitions_by_symbol
        .into_values()
        .flatten()
        .collect::<BTreeSet<_>>();

    let mut occurrences = BTreeSet::new();
    let mut external_or_unresolved_occurrence_count = 0usize;
    for document in &documents {
        for occurrence in &document.occurrences {
            let Some(target) = unique_definitions.get(&occurrence.symbol) else {
                if !occurrence.definition && !occurrence.symbol.is_empty() {
                    external_or_unresolved_occurrence_count += 1;
                }
                continue;
            };
            occurrences.insert(occurrence_fact(&document.path, occurrence, target));
        }
    }

    let mut normalized_relations = BTreeSet::new();
    let mut external_or_unresolved_relation_count = 0usize;
    let mut relation_kind_counts = BTreeMap::<String, usize>::new();
    for relation in &relations {
        *relation_kind_counts
            .entry(relation.kind.clone())
            .or_default() += 1;
        let (Some(source), Some(target)) = (
            unique_definitions.get(&relation.from),
            unique_definitions.get(&relation.to),
        ) else {
            external_or_unresolved_relation_count += 1;
            continue;
        };
        normalized_relations.insert(RelationFact {
            kind: relation.kind.clone(),
            path: relation.path.clone(),
            range: canonical_range(&relation.range),
            source: source.clone(),
            target: target.clone(),
        });
    }

    let semantic_fact_digest = normalized_fact_digest(
        &document_paths,
        &definitions,
        &occurrences,
        &normalized_relations,
    );

    NormalizedProvider {
        document_paths,
        definitions,
        occurrences,
        relations: normalized_relations,
        summary: ProviderSummary {
            semantic_fact_digest,
            document_count: documents.len(),
            symbol_count,
            occurrence_count,
            definition_count,
            relation_count: relations.len(),
            relation_kind_counts,
            unique_workspace_symbol_count: unique_definitions.len(),
            ambiguous_workspace_symbol_count,
            external_or_unresolved_occurrence_count,
            external_or_unresolved_relation_count,
        },
    }
}

fn normalized_fact_digest(
    document_paths: &BTreeSet<String>,
    definitions: &BTreeSet<DefinitionLocator>,
    occurrences: &BTreeSet<OccurrenceFact>,
    relations: &BTreeSet<RelationFact>,
) -> String {
    let bytes = serde_json::to_vec(&(document_paths, definitions, occurrences, relations))
        .expect("normalized comparison facts are serializable");
    format!("{:x}", Sha256::digest(bytes))
}

fn occurrence_fact(
    path: &str,
    occurrence: &OccurrenceOutput,
    target: &DefinitionLocator,
) -> OccurrenceFact {
    OccurrenceFact {
        path: path.to_string(),
        range: canonical_range(&occurrence.range),
        target: target.clone(),
        definition: occurrence.definition,
        import: occurrence.import,
        read: occurrence.read,
        write: occurrence.write,
    }
}

fn canonical_range(range: &[i32]) -> Vec<i32> {
    match range {
        [line, start, end] => vec![*line, *start, *line, *end],
        _ => range.to_vec(),
    }
}

fn compare_sets<T>(baseline: &BTreeSet<T>, candidate: &BTreeSet<T>) -> SetAgreement<T>
where
    T: Clone + Ord + Serialize,
{
    let baseline_only = baseline.difference(candidate).cloned().collect::<Vec<_>>();
    let candidate_only = candidate.difference(baseline).cloned().collect::<Vec<_>>();
    let intersection_count = baseline.intersection(candidate).count();
    let precision = ratio(intersection_count, candidate.len());
    let recall = ratio(intersection_count, baseline.len());
    let f1 = if precision + recall == 0.0 {
        if baseline.is_empty() && candidate.is_empty() {
            1.0
        } else {
            0.0
        }
    } else {
        2.0 * precision * recall / (precision + recall)
    };
    let samples_truncated =
        baseline_only.len() > MAX_DIFF_SAMPLES || candidate_only.len() > MAX_DIFF_SAMPLES;
    SetAgreement {
        baseline_count: baseline.len(),
        candidate_count: candidate.len(),
        intersection_count,
        baseline_only_count: baseline_only.len(),
        candidate_only_count: candidate_only.len(),
        agreement_precision: precision,
        agreement_recall: recall,
        agreement_f1: f1,
        baseline_only_sample: take_sample(baseline_only),
        candidate_only_sample: take_sample(candidate_only),
        samples_truncated,
    }
}

fn baseline_set_coverage<T>(baseline: &BTreeSet<T>, candidate: &BTreeSet<T>) -> BaselineCoverage<T>
where
    T: Clone + Ord + Serialize,
{
    let regressions = baseline.difference(candidate).cloned().collect::<Vec<_>>();
    BaselineCoverage {
        baseline_count: baseline.len(),
        covered_count: baseline.len().saturating_sub(regressions.len()),
        regression_count: regressions.len(),
        samples_truncated: regressions.len() > MAX_DIFF_SAMPLES,
        regression_sample: take_sample(regressions),
    }
}

fn occurrence_baseline_coverage(
    baseline: &BTreeSet<OccurrenceFact>,
    candidate: &BTreeSet<OccurrenceFact>,
) -> BaselineCoverage<OccurrenceFact> {
    let regressions = baseline
        .iter()
        .filter(|baseline_fact| {
            !candidate
                .iter()
                .any(|candidate_fact| occurrence_covers(candidate_fact, baseline_fact))
        })
        .cloned()
        .collect::<Vec<_>>();
    BaselineCoverage {
        baseline_count: baseline.len(),
        covered_count: baseline.len().saturating_sub(regressions.len()),
        regression_count: regressions.len(),
        samples_truncated: regressions.len() > MAX_DIFF_SAMPLES,
        regression_sample: take_sample(regressions),
    }
}

/// Provider flags are positive evidence. A candidate may refine an occurrence
/// from unspecified (`false`) to read/write/import without regressing it, but
/// it may never drop a positive property reported by the baseline.
fn occurrence_covers(candidate: &OccurrenceFact, baseline: &OccurrenceFact) -> bool {
    candidate.path == baseline.path
        && candidate.range == baseline.range
        && candidate.target == baseline.target
        && (!baseline.definition || candidate.definition)
        && (!baseline.import || candidate.import)
        && (!baseline.read || candidate.read)
        && (!baseline.write || candidate.write)
}

fn append_regression_blocker<T: Serialize>(
    blockers: &mut Vec<String>,
    label: &str,
    coverage: &BaselineCoverage<T>,
) {
    if coverage.regression_count != 0 {
        blockers.push(format!(
            "candidate omitted {} baseline {label}",
            coverage.regression_count
        ));
    }
}

fn ratio(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        if numerator == 0 {
            1.0
        } else {
            0.0
        }
    } else {
        numerator as f64 / denominator as f64
    }
}

fn take_sample<T>(mut values: Vec<T>) -> Vec<T> {
    values.truncate(MAX_DIFF_SAMPLES);
    values
}

fn option_value(args: &[String], flag: &str) -> Option<String> {
    for (index, value) in args.iter().enumerate() {
        if value == flag {
            return args.get(index + 1).cloned();
        }
        if let Some(value) = value.strip_prefix(&format!("{flag}=")) {
            return Some(value.to_string());
        }
    }
    None
}

fn canonical_existing_dir(path: &Path) -> Result<PathBuf, String> {
    let path = fs::canonicalize(path)
        .map_err(|error| format!("cannot resolve {}: {error}", path.display()))?;
    if !path.is_dir() {
        return Err(format!("not a directory: {}", path.display()));
    }
    Ok(path)
}

fn canonical_existing_file(path: &Path) -> Result<PathBuf, String> {
    let path = fs::canonicalize(path)
        .map_err(|error| format!("cannot resolve {}: {error}", path.display()))?;
    if !path.is_file() {
        return Err(format!("not a file: {}", path.display()));
    }
    Ok(path)
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let bytes = fs::read(path)
        .map_err(|error| format!("cannot read {} for digest: {error}", path.display()))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SymbolOutput;

    fn document(symbol: &str, target: &str) -> DocumentOutput {
        DocumentOutput {
            language: "python".to_string(),
            path: "app.py".to_string(),
            symbols: vec![SymbolOutput {
                symbol: symbol.to_string(),
                kind: "function".to_string(),
                display_name: Some("run".to_string()),
                documentation: Vec::new(),
                signature: None,
                enclosing_symbol: None,
            }],
            occurrences: vec![
                OccurrenceOutput {
                    symbol: symbol.to_string(),
                    range: vec![0, 4, 0, 7],
                    enclosing_range: vec![0, 0, 2, 0],
                    definition: true,
                    import: false,
                    read: false,
                    write: false,
                },
                OccurrenceOutput {
                    symbol: target.to_string(),
                    range: vec![1, 4, 1, 10],
                    enclosing_range: vec![0, 0, 2, 0],
                    definition: false,
                    import: false,
                    read: true,
                    write: false,
                },
                OccurrenceOutput {
                    symbol: target.to_string(),
                    range: vec![3, 4, 3, 10],
                    enclosing_range: vec![3, 0, 4, 0],
                    definition: true,
                    import: false,
                    read: false,
                    write: false,
                },
            ],
        }
    }

    #[test]
    fn normalization_compares_locations_instead_of_provider_symbol_strings() {
        let baseline = normalize_provider(
            vec![document("lsp run", "lsp helper")],
            vec![RelationOutput {
                from: "lsp run".to_string(),
                to: "lsp helper".to_string(),
                kind: "CALLS".to_string(),
                path: "app.py".to_string(),
                range: vec![1, 4, 1, 10],
                confidence: Some(1.0),
                strategy: None,
            }],
        );
        let candidate = normalize_provider(
            vec![document("scip run", "scip helper")],
            vec![RelationOutput {
                from: "scip run".to_string(),
                to: "scip helper".to_string(),
                kind: "CALLS".to_string(),
                path: "app.py".to_string(),
                range: vec![1, 4, 1, 10],
                confidence: Some(1.0),
                strategy: None,
            }],
        );

        assert_eq!(
            compare_sets(&baseline.definitions, &candidate.definitions).agreement_f1,
            1.0
        );
        assert_eq!(
            compare_sets(&baseline.occurrences, &candidate.occurrences).agreement_f1,
            1.0
        );
        assert_eq!(
            compare_sets(&baseline.relations, &candidate.relations).agreement_f1,
            1.0
        );
        assert_eq!(
            baseline.summary.semantic_fact_digest,
            candidate.summary.semantic_fact_digest
        );
    }

    #[test]
    fn normalization_compares_compact_scip_and_lsp_ranges_equally() {
        assert_eq!(canonical_range(&[4, 8, 13]), vec![4, 8, 4, 13]);
        assert_eq!(canonical_range(&[4, 8, 4, 13]), vec![4, 8, 4, 13]);
        assert_eq!(canonical_range(&[]), Vec::<i32>::new());
    }

    #[test]
    fn typescript_shadow_reuses_only_executable_site_coordinates() {
        let relations = vec![
            RelationOutput {
                from: "caller".to_string(),
                to: "callee".to_string(),
                kind: "CALLS".to_string(),
                path: "src/main.ts".to_string(),
                range: vec![4, 8, 4, 11],
                confidence: Some(1.0),
                strategy: None,
            },
            RelationOutput {
                from: "caller".to_string(),
                to: "value".to_string(),
                kind: "REFERENCES".to_string(),
                path: "src/main.ts".to_string(),
                range: vec![5, 8, 5, 13],
                confidence: Some(1.0),
                strategy: None,
            },
        ];

        let ranges = reference_call_ranges("typescript", &relations).unwrap();
        assert_eq!(ranges["src/main.ts"], vec![vec![4, 8, 4, 11]]);
        assert!(reference_call_ranges("rust", &relations).is_none());
    }

    #[test]
    fn missing_relation_is_a_regression_blocker() {
        let baseline = [RelationFact {
            kind: "CALLS".to_string(),
            path: "app.py".to_string(),
            range: vec![1, 4, 1, 10],
            source: DefinitionLocator {
                path: "app.py".to_string(),
                range: vec![0, 4, 0, 7],
            },
            target: DefinitionLocator {
                path: "app.py".to_string(),
                range: vec![3, 4, 3, 10],
            },
        }]
        .into_iter()
        .collect();
        let candidate = BTreeSet::new();
        let coverage = baseline_set_coverage(&baseline, &candidate);
        let mut blockers = Vec::new();
        append_regression_blocker(&mut blockers, "workspace relations", &coverage);

        assert_eq!(coverage.regression_count, 1);
        assert_eq!(coverage.covered_count, 0);
        assert_eq!(blockers.len(), 1);
    }

    #[test]
    fn candidate_only_relation_requires_review_but_is_not_a_regression() {
        let baseline = BTreeSet::new();
        let candidate = [RelationFact {
            kind: "REFERENCES".to_string(),
            path: "app.py".to_string(),
            range: vec![1, 4, 1, 10],
            source: DefinitionLocator {
                path: "app.py".to_string(),
                range: vec![0, 4, 0, 7],
            },
            target: DefinitionLocator {
                path: "app.py".to_string(),
                range: vec![3, 4, 3, 10],
            },
        }]
        .into_iter()
        .collect();
        let agreement = compare_sets(&baseline, &candidate);
        let coverage = baseline_set_coverage(&baseline, &candidate);

        assert_eq!(agreement.candidate_only_count, 1);
        assert_eq!(coverage.regression_count, 0);
    }

    #[test]
    fn positive_occurrence_metadata_refinement_preserves_baseline_fact() {
        let target = DefinitionLocator {
            path: "app.py".to_string(),
            range: vec![3, 4, 3, 10],
        };
        let baseline_fact = OccurrenceFact {
            path: "app.py".to_string(),
            range: vec![1, 4, 1, 10],
            target: target.clone(),
            definition: false,
            import: false,
            read: false,
            write: false,
        };
        let candidate_fact = OccurrenceFact {
            read: true,
            ..baseline_fact.clone()
        };
        let baseline = [baseline_fact].into_iter().collect();
        let candidate = [candidate_fact].into_iter().collect();
        let coverage = occurrence_baseline_coverage(&baseline, &candidate);

        assert_eq!(coverage.covered_count, 1);
        assert_eq!(coverage.regression_count, 0);
    }

    #[test]
    fn dropping_positive_occurrence_metadata_is_a_regression() {
        let target = DefinitionLocator {
            path: "app.py".to_string(),
            range: vec![3, 4, 3, 10],
        };
        let baseline_fact = OccurrenceFact {
            path: "app.py".to_string(),
            range: vec![1, 4, 1, 10],
            target: target.clone(),
            definition: false,
            import: false,
            read: true,
            write: false,
        };
        let candidate_fact = OccurrenceFact {
            read: false,
            ..baseline_fact.clone()
        };
        let baseline = [baseline_fact].into_iter().collect();
        let candidate = [candidate_fact].into_iter().collect();
        let coverage = occurrence_baseline_coverage(&baseline, &candidate);

        assert_eq!(coverage.covered_count, 0);
        assert_eq!(coverage.regression_count, 1);
    }

    #[test]
    fn unresolved_external_occurrences_are_counted_but_not_compared_as_local_truth() {
        let mut input = document("local", "helper");
        input.occurrences.push(OccurrenceOutput {
            symbol: "external".to_string(),
            range: vec![5, 4, 5, 12],
            enclosing_range: vec![0, 0, 2, 0],
            definition: false,
            import: false,
            read: true,
            write: false,
        });
        let normalized = normalize_provider(vec![input], Vec::new());

        assert_eq!(normalized.occurrences.len(), 3);
        assert_eq!(
            normalized.summary.external_or_unresolved_occurrence_count,
            1
        );
    }
}
