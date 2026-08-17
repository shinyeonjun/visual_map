//! legacy vs structural cross-key clustering A/B 비교.

use super::gold::EvalGold;
use super::score::{count_findings_by_kind, score_gold, EvalReport};
use super::snapshot::snapshot_from_overview;
use super::EvalError;
use crate::config::DomainClusteringMode;
use crate::domain::DomainFormationDiagnostics;
use crate::analyze;
use crate::model::AnalysisRequest;
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClusteringAbProjectReport {
    pub id: String,
    pub skipped: bool,
    pub skip_reason: Option<String>,
    pub legacy: Option<ClusteringAbModeReport>,
    pub structural: Option<ClusteringAbModeReport>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClusteringAbModeReport {
    pub passed: bool,
    pub domain_hits: usize,
    pub domain_expected: usize,
    pub feature_hits: usize,
    pub feature_expected: usize,
    pub flow_hits: usize,
    pub flow_expected: usize,
    pub over_split: usize,
    pub wrong_domain: usize,
    pub over_merge: usize,
    pub formation_diagnostics: DomainFormationDiagnostics,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClusteringAbSummary {
    pub projects: usize,
    pub analyzed: usize,
    pub skipped: usize,
    pub legacy_passed: usize,
    pub structural_passed: usize,
    pub legacy_domain_hits: usize,
    pub structural_domain_hits: usize,
    pub legacy_domain_expected: usize,
    pub structural_domain_expected: usize,
    pub legacy_feature_hits: usize,
    pub structural_feature_hits: usize,
    pub legacy_feature_expected: usize,
    pub structural_feature_expected: usize,
    pub legacy_over_split: usize,
    pub structural_over_split: usize,
    pub legacy_wrong_domain: usize,
    pub structural_wrong_domain: usize,
    pub legacy_over_merge: usize,
    pub structural_over_merge: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClusteringAbReport {
    pub summary: ClusteringAbSummary,
    pub projects: Vec<ClusteringAbProjectReport>,
}

pub fn compare_clustering_modes(catalog_path: &Path) -> Result<ClusteringAbReport, EvalError> {
    let golds = super::load_catalog(catalog_path)?;
    let mut projects = Vec::with_capacity(golds.len());

    for gold in &golds {
        projects.push(compare_project(gold)?);
    }

    Ok(ClusteringAbReport {
        summary: summarize_ab(&projects),
        projects,
    })
}

fn compare_project(gold: &EvalGold) -> Result<ClusteringAbProjectReport, EvalError> {
    let Some(project_path) = gold.project_path.as_deref() else {
        return Ok(skipped_report(
            gold,
            "gold에 projectPath가 없어 분석할 수 없습니다.".into(),
        ));
    };
    let project_path = PathBuf::from(project_path);
    if !project_path.is_dir() {
        return Ok(skipped_report(
            gold,
            format!("프로젝트 경로가 없습니다: {}", project_path.display()),
        ));
    }

    let legacy = analyze_with_mode(gold, &project_path, DomainClusteringMode::LegacyStrictKey)?;
    let structural =
        analyze_with_mode(gold, &project_path, DomainClusteringMode::StructuralCrossKey)?;

    Ok(ClusteringAbProjectReport {
        id: gold.id.clone(),
        skipped: false,
        skip_reason: None,
        legacy: Some(legacy),
        structural: Some(structural),
    })
}

fn skipped_report(gold: &EvalGold, reason: String) -> ClusteringAbProjectReport {
    ClusteringAbProjectReport {
        id: gold.id.clone(),
        skipped: true,
        skip_reason: Some(reason),
        legacy: None,
        structural: None,
    }
}

fn analyze_with_mode(
    gold: &EvalGold,
    project_path: &Path,
    mode: DomainClusteringMode,
) -> Result<ClusteringAbModeReport, EvalError> {
    let mut request = AnalysisRequest::new(project_path);
    request.options.config.domains.domain_clustering_mode = mode;
    let result = analyze(request).map_err(analysis_error)?;
    let overview = result.overview.ok_or(super::EvalError::MissingOverview)?;
    let report = score_gold(gold, &snapshot_from_overview(&overview));
    Ok(mode_report(report, overview.formation_diagnostics))
}

fn mode_report(report: EvalReport, diagnostics: DomainFormationDiagnostics) -> ClusteringAbModeReport {
    ClusteringAbModeReport {
        passed: report.passed,
        domain_hits: report.domain_hits,
        domain_expected: report.domain_expected,
        feature_hits: report.feature_hits,
        feature_expected: report.feature_expected,
        flow_hits: report.flow_hits,
        flow_expected: report.flow_expected,
        over_split: count_findings_by_kind(&report.findings, "overSplit"),
        wrong_domain: count_findings_by_kind(&report.findings, "wrongDomain"),
        over_merge: count_findings_by_kind(&report.findings, "overMerge"),
        formation_diagnostics: diagnostics,
    }
}

fn analysis_error(error: crate::EngineError) -> EvalError {
    EvalError::AnalysisFailed(error.to_string())
}

fn summarize_ab(projects: &[ClusteringAbProjectReport]) -> ClusteringAbSummary {
    let mut summary = ClusteringAbSummary {
        projects: projects.len(),
        analyzed: 0,
        skipped: 0,
        legacy_passed: 0,
        structural_passed: 0,
        legacy_domain_hits: 0,
        structural_domain_hits: 0,
        legacy_domain_expected: 0,
        structural_domain_expected: 0,
        legacy_feature_hits: 0,
        structural_feature_hits: 0,
        legacy_feature_expected: 0,
        structural_feature_expected: 0,
        legacy_over_split: 0,
        structural_over_split: 0,
        legacy_wrong_domain: 0,
        structural_wrong_domain: 0,
        legacy_over_merge: 0,
        structural_over_merge: 0,
    };

    for project in projects {
        if project.skipped {
            summary.skipped += 1;
            continue;
        }
        summary.analyzed += 1;
        if let Some(legacy) = &project.legacy {
            if legacy.passed {
                summary.legacy_passed += 1;
            }
            summary.legacy_domain_hits += legacy.domain_hits;
            summary.legacy_domain_expected += legacy.domain_expected;
            summary.legacy_feature_hits += legacy.feature_hits;
            summary.legacy_feature_expected += legacy.feature_expected;
            summary.legacy_over_split += legacy.over_split;
            summary.legacy_wrong_domain += legacy.wrong_domain;
            summary.legacy_over_merge += legacy.over_merge;
        }
        if let Some(structural) = &project.structural {
            if structural.passed {
                summary.structural_passed += 1;
            }
            summary.structural_domain_hits += structural.domain_hits;
            summary.structural_domain_expected += structural.domain_expected;
            summary.structural_feature_hits += structural.feature_hits;
            summary.structural_feature_expected += structural.feature_expected;
            summary.structural_over_split += structural.over_split;
            summary.structural_wrong_domain += structural.wrong_domain;
            summary.structural_over_merge += structural.over_merge;
        }
    }

    summary
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clustering_ab_리포트_구조가_직렬화된다() {
        let report = ClusteringAbReport {
            summary: ClusteringAbSummary {
                projects: 1,
                analyzed: 0,
                skipped: 1,
                legacy_passed: 0,
                structural_passed: 0,
                legacy_domain_hits: 0,
                structural_domain_hits: 0,
                legacy_domain_expected: 0,
                structural_domain_expected: 0,
                legacy_feature_hits: 0,
                structural_feature_hits: 0,
                legacy_feature_expected: 0,
                structural_feature_expected: 0,
                legacy_over_split: 0,
                structural_over_split: 0,
                legacy_wrong_domain: 0,
                structural_wrong_domain: 0,
                legacy_over_merge: 0,
                structural_over_merge: 0,
            },
            projects: vec![ClusteringAbProjectReport {
                id: "sample".into(),
                skipped: true,
                skip_reason: Some("missing".into()),
                legacy: None,
                structural: None,
            }],
        };
        let json = serde_json::to_string(&report).expect("직렬화");
        assert!(json.contains("legacyDomainHits"));
    }

    #[test]
    fn project_path_없는_gold는_ab에서_skip된다() {
        let gold = EvalGold {
            id: "no-path".into(),
            ..EvalGold::default()
        };
        let item = compare_project(&gold).expect("비교");
        assert!(item.skipped);
    }
}
