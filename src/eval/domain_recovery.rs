//! Anchor/Ownership 기반 Domain Recovery production path gold evaluation.

use super::gold::EvalGold;
use super::score::{count_findings_by_kind, score_gold, EvalOutcome, EvalReport};
use super::snapshot::snapshot_from_domain_recovery;
use super::EvalError;
use crate::domain::DomainSeedDiagnostics;
use crate::model::AnalysisRequest;
use crate::{analyze, AnalysisEngine};
use serde::Serialize;
use std::path::{Path, PathBuf};

pub const HISTORICAL_CLUSTERING_BASELINE_LEGACY_DOMAIN_HITS: usize = 57;
pub const HISTORICAL_CLUSTERING_BASELINE_LEGACY_DOMAIN_EXPECTED: usize = 96;
pub const HISTORICAL_CLUSTERING_BASELINE_STRUCTURAL_DOMAIN_HITS: usize = 53;
pub const HISTORICAL_CLUSTERING_BASELINE_STRUCTURAL_DOMAIN_EXPECTED: usize = 96;
pub const HISTORICAL_CLUSTERING_BASELINE_LEGACY_PASSED: usize = 8;
pub const HISTORICAL_CLUSTERING_BASELINE_STRUCTURAL_PASSED: usize = 8;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DomainRecoveryCatalogReport {
    pub summary: DomainRecoverySummary,
    pub historical_clustering_baseline: HistoricalClusteringBaseline,
    pub projects: Vec<DomainRecoveryProjectReport>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DomainRecoverySummary {
    pub projects: usize,
    pub analyzed: usize,
    pub skipped: usize,
    pub passed: usize,
    pub failed: usize,
    pub domain_hits: usize,
    pub domain_expected: usize,
    pub domain_miss: usize,
    pub feature_hits: usize,
    pub feature_expected: usize,
    pub feature_miss: usize,
    pub flow_hits: usize,
    pub flow_expected: usize,
    pub flow_miss: usize,
    pub wrong_domain: usize,
    pub over_split: usize,
    pub over_merge: usize,
    pub unassigned: usize,
    pub no_candidate: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoricalClusteringBaseline {
    pub note: String,
    pub legacy_domain_hits: usize,
    pub legacy_domain_expected: usize,
    pub legacy_passed: usize,
    pub structural_domain_hits: usize,
    pub structural_domain_expected: usize,
    pub structural_passed: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DomainRecoveryProjectReport {
    pub id: String,
    pub set: String,
    pub skipped: bool,
    pub skip_reason: Option<String>,
    pub passed: bool,
    pub outcome: EvalOutcome,
    pub domain_hits: usize,
    pub domain_expected: usize,
    pub domain_miss: usize,
    pub feature_hits: usize,
    pub feature_expected: usize,
    pub feature_miss: usize,
    pub flow_hits: usize,
    pub flow_expected: usize,
    pub flow_miss: usize,
    pub wrong_domain: usize,
    pub over_split: usize,
    pub over_merge: usize,
    pub unassigned: usize,
    pub no_candidate: usize,
    pub eval_report: Option<EvalReport>,
}

pub fn evaluate_domain_recovery_catalog(
    catalog_path: &Path,
) -> Result<DomainRecoveryCatalogReport, EvalError> {
    let golds = super::load_catalog(catalog_path)?;
    let engine = AnalysisEngine::default();
    let mut projects = Vec::with_capacity(golds.len());

    for gold in &golds {
        projects.push(evaluate_project(&engine, gold)?);
    }

    Ok(DomainRecoveryCatalogReport {
        summary: summarize(&projects),
        historical_clustering_baseline: historical_clustering_baseline(),
        projects,
    })
}

fn evaluate_project(
    engine: &AnalysisEngine,
    gold: &EvalGold,
) -> Result<DomainRecoveryProjectReport, EvalError> {
    let Some(project_path) = gold.project_path.as_deref() else {
        return Ok(skipped_report(gold, "gold에 projectPath가 없어 분석할 수 없습니다.".into()));
    };
    let project_path = PathBuf::from(project_path);
    if !project_path.is_dir() {
        return Ok(skipped_report(
            gold,
            format!("프로젝트 경로가 없습니다: {}", project_path.display()),
        ));
    }

    let request = AnalysisRequest::new(&project_path);
    let result = analyze(request.clone()).map_err(analysis_error)?;
    let overview = result.overview.ok_or(super::EvalError::MissingOverview)?;
    let diagnostics = engine
        .diagnose_domain_seeds(request)
        .map_err(analysis_error)?;
    let assignment_stats = assignment_stats(&diagnostics);
    let snapshot = snapshot_from_domain_recovery(&overview, &diagnostics);
    let eval_report = score_gold(gold, &snapshot);
    Ok(project_report(gold, eval_report, assignment_stats))
}

fn project_report(
    gold: &EvalGold,
    eval_report: EvalReport,
    assignment_stats: AssignmentStats,
) -> DomainRecoveryProjectReport {
    DomainRecoveryProjectReport {
        id: gold.id.clone(),
        set: gold.set.clone(),
        skipped: false,
        skip_reason: None,
        passed: eval_report.passed,
        outcome: eval_report.outcome.clone(),
        domain_hits: eval_report.domain_hits,
        domain_expected: eval_report.domain_expected,
        domain_miss: eval_report.domain_expected.saturating_sub(eval_report.domain_hits),
        feature_hits: eval_report.feature_hits,
        feature_expected: eval_report.feature_expected,
        feature_miss: eval_report
            .feature_expected
            .saturating_sub(eval_report.feature_hits),
        flow_hits: eval_report.flow_hits,
        flow_expected: eval_report.flow_expected,
        flow_miss: eval_report.flow_expected.saturating_sub(eval_report.flow_hits),
        wrong_domain: count_findings_by_kind(&eval_report.findings, "wrongDomain"),
        over_split: count_findings_by_kind(&eval_report.findings, "overSplit"),
        over_merge: count_findings_by_kind(&eval_report.findings, "overMerge"),
        unassigned: assignment_stats.unassigned,
        no_candidate: assignment_stats.no_candidate,
        eval_report: Some(eval_report),
    }
}

fn skipped_report(gold: &EvalGold, reason: String) -> DomainRecoveryProjectReport {
    DomainRecoveryProjectReport {
        id: gold.id.clone(),
        set: gold.set.clone(),
        skipped: true,
        skip_reason: Some(reason),
        passed: false,
        outcome: EvalOutcome::Fail,
        domain_hits: 0,
        domain_expected: gold.must_have_domains.len(),
        domain_miss: gold.must_have_domains.len(),
        feature_hits: 0,
        feature_expected: gold.must_have_features.len(),
        feature_miss: gold.must_have_features.len(),
        flow_hits: 0,
        flow_expected: gold.flow_invariants.len(),
        flow_miss: gold.flow_invariants.len(),
        wrong_domain: 0,
        over_split: 0,
        over_merge: 0,
        unassigned: 0,
        no_candidate: 0,
        eval_report: None,
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct AssignmentStats {
    unassigned: usize,
    no_candidate: usize,
}

fn assignment_stats(diagnostics: &DomainSeedDiagnostics) -> AssignmentStats {
    let assignments = &diagnostics
        .aggregation
        .anchor_capability_graph
        .capability_assignments;
    let mut stats = AssignmentStats::default();
    for assignment in assignments {
        if assignment.retrieved_candidate_count == 0 {
            stats.no_candidate += 1;
        }
        if assignment.assignment_state == "unassigned" {
            stats.unassigned += 1;
        }
    }
    stats
}

fn summarize(projects: &[DomainRecoveryProjectReport]) -> DomainRecoverySummary {
    let mut summary = DomainRecoverySummary {
        projects: projects.len(),
        analyzed: 0,
        skipped: 0,
        passed: 0,
        failed: 0,
        domain_hits: 0,
        domain_expected: 0,
        domain_miss: 0,
        feature_hits: 0,
        feature_expected: 0,
        feature_miss: 0,
        flow_hits: 0,
        flow_expected: 0,
        flow_miss: 0,
        wrong_domain: 0,
        over_split: 0,
        over_merge: 0,
        unassigned: 0,
        no_candidate: 0,
    };

    for project in projects {
        if project.skipped {
            summary.skipped += 1;
            continue;
        }
        summary.analyzed += 1;
        if project.passed {
            summary.passed += 1;
        } else {
            summary.failed += 1;
        }
        summary.domain_hits += project.domain_hits;
        summary.domain_expected += project.domain_expected;
        summary.domain_miss += project.domain_miss;
        summary.feature_hits += project.feature_hits;
        summary.feature_expected += project.feature_expected;
        summary.feature_miss += project.feature_miss;
        summary.flow_hits += project.flow_hits;
        summary.flow_expected += project.flow_expected;
        summary.flow_miss += project.flow_miss;
        summary.wrong_domain += project.wrong_domain;
        summary.over_split += project.over_split;
        summary.over_merge += project.over_merge;
        summary.unassigned += project.unassigned;
        summary.no_candidate += project.no_candidate;
    }

    summary
}

fn historical_clustering_baseline() -> HistoricalClusteringBaseline {
    HistoricalClusteringBaseline {
        note: "historical control from legacyStrictKey vs structuralCrossKey clustering A/B; not Anchor/Ownership Domain Recovery".into(),
        legacy_domain_hits: HISTORICAL_CLUSTERING_BASELINE_LEGACY_DOMAIN_HITS,
        legacy_domain_expected: HISTORICAL_CLUSTERING_BASELINE_LEGACY_DOMAIN_EXPECTED,
        legacy_passed: HISTORICAL_CLUSTERING_BASELINE_LEGACY_PASSED,
        structural_domain_hits: HISTORICAL_CLUSTERING_BASELINE_STRUCTURAL_DOMAIN_HITS,
        structural_domain_expected: HISTORICAL_CLUSTERING_BASELINE_STRUCTURAL_DOMAIN_EXPECTED,
        structural_passed: HISTORICAL_CLUSTERING_BASELINE_STRUCTURAL_PASSED,
    }
}

fn analysis_error(error: crate::EngineError) -> EvalError {
    EvalError::AnalysisFailed(error.to_string())
}
