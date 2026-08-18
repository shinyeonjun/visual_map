//! gold positive/negative pair 신호 비교 eval.

use super::gold::EvalGold;
use super::gold_pairs::extract_gold_pair_labels;
use super::pair_diagnose::{
    PairDiagnoseModeSelection, DEFAULT_PAIR_DIAGNOSE_IDS,
};
use super::EvalError;
use crate::domain::GoldPairSignalModeReport;
use crate::config::DomainClusteringMode;
use crate::model::AnalysisRequest;
use crate::AnalysisEngine;
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GoldPairSignalProjectReport {
    pub id: String,
    pub skipped: bool,
    pub skip_reason: Option<String>,
    pub positive_count: usize,
    pub negative_count: usize,
    pub modes: Vec<GoldPairSignalModeReport>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GoldPairSignalReport {
    pub projects: Vec<GoldPairSignalProjectReport>,
}

pub fn diagnose_gold_pair_catalog(
    catalog_path: &Path,
    project_ids: &[String],
    mode_selection: PairDiagnoseModeSelection,
    include_evidence: bool,
) -> Result<GoldPairSignalReport, EvalError> {
    let golds = super::load_catalog(catalog_path)?;
    let selected = select_golds(&golds, project_ids);
    let modes = modes_for_selection(mode_selection);
    let engine = AnalysisEngine::default();

    let mut projects = Vec::with_capacity(selected.len());
    for gold in selected {
        projects.push(diagnose_gold(&engine, gold, &modes, include_evidence)?);
    }

    Ok(GoldPairSignalReport { projects })
}

fn select_golds<'a>(golds: &'a [EvalGold], project_ids: &[String]) -> Vec<&'a EvalGold> {
    if project_ids.is_empty() {
        return golds
            .iter()
            .filter(|gold| DEFAULT_PAIR_DIAGNOSE_IDS.contains(&gold.id.as_str()))
            .collect();
    }
    golds
        .iter()
        .filter(|gold| project_ids.iter().any(|id| id == &gold.id))
        .collect()
}

fn modes_for_selection(selection: PairDiagnoseModeSelection) -> Vec<DomainClusteringMode> {
    match selection {
        PairDiagnoseModeSelection::Legacy => vec![DomainClusteringMode::LegacyStrictKey],
        PairDiagnoseModeSelection::Structural => {
            vec![DomainClusteringMode::StructuralCrossKey]
        }
        PairDiagnoseModeSelection::Both => vec![
            DomainClusteringMode::LegacyStrictKey,
            DomainClusteringMode::StructuralCrossKey,
        ],
    }
}

fn diagnose_gold(
    engine: &AnalysisEngine,
    gold: &EvalGold,
    modes: &[DomainClusteringMode],
    include_evidence: bool,
) -> Result<GoldPairSignalProjectReport, EvalError> {
    let (positives, negatives) = extract_gold_pair_labels(gold);
    let Some(project_path) = gold.project_path.as_deref() else {
        return Ok(skipped_report(
            gold,
            positives.len(),
            negatives.len(),
            "gold에 projectPath가 없어 분석할 수 없습니다.".into(),
        ));
    };
    let project_path = PathBuf::from(project_path);
    if !project_path.is_dir() {
        return Ok(skipped_report(
            gold,
            positives.len(),
            negatives.len(),
            format!("프로젝트 경로가 없습니다: {}", project_path.display()),
        ));
    }

    let request = AnalysisRequest::new(project_path);
    let mode_reports = engine
        .diagnose_gold_pair_signals(request, gold, modes, include_evidence)
        .map_err(|error| EvalError::AnalysisFailed(error.to_string()))?;

    Ok(GoldPairSignalProjectReport {
        id: gold.id.clone(),
        skipped: false,
        skip_reason: None,
        positive_count: positives.len(),
        negative_count: negatives.len(),
        modes: mode_reports,
    })
}

fn skipped_report(
    gold: &EvalGold,
    positive_count: usize,
    negative_count: usize,
    reason: String,
) -> GoldPairSignalProjectReport {
    GoldPairSignalProjectReport {
        id: gold.id.clone(),
        skipped: true,
        skip_reason: Some(reason),
        positive_count,
        negative_count,
        modes: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gold_pair_signal_리포트는_프로젝트별_positive_negative_count를_담는다() {
        let report = GoldPairSignalReport {
            projects: vec![GoldPairSignalProjectReport {
                id: "demo".into(),
                skipped: true,
                skip_reason: Some("missing path".into()),
                positive_count: 3,
                negative_count: 5,
                modes: Vec::new(),
            }],
        };
        assert_eq!(report.projects[0].positive_count, 3);
        assert_eq!(report.projects[0].negative_count, 5);
    }
}
