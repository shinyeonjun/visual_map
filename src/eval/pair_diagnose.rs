//! capability pair rejection diagnostics eval.

use super::gold::EvalGold;
use super::EvalError;
use crate::config::DomainClusteringMode;
use crate::domain::CapabilityPairDiagnostics;
use crate::model::AnalysisRequest;
use crate::AnalysisEngine;
use serde::Serialize;
use std::path::{Path, PathBuf};

pub const DEFAULT_PAIR_DIAGNOSE_IDS: &[&str] = &[
    "fastapi-full-stack-fastapi-template",
    "meeting-overlay-assistant",
    "nestjs-boilerplate",
    "vendure",
    "vaultwarden",
    "serverpod",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairDiagnoseModeSelection {
    Legacy,
    Structural,
    Both,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PairDiagnoseProjectReport {
    pub id: String,
    pub skipped: bool,
    pub skip_reason: Option<String>,
    pub modes: Vec<CapabilityPairDiagnostics>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PairDiagnoseReport {
    pub projects: Vec<PairDiagnoseProjectReport>,
}

pub fn diagnose_pair_catalog(
    catalog_path: &Path,
    project_ids: &[String],
    mode_selection: PairDiagnoseModeSelection,
    top_k: usize,
) -> Result<PairDiagnoseReport, EvalError> {
    let golds = super::load_catalog(catalog_path)?;
    let selected = select_golds(&golds, project_ids);
    let modes = modes_for_selection(mode_selection);
    let engine = AnalysisEngine::default();

    let mut projects = Vec::with_capacity(selected.len());
    for gold in selected {
        projects.push(diagnose_gold(&engine, gold, &modes, top_k)?);
    }

    Ok(PairDiagnoseReport { projects })
}

pub(crate) fn select_golds<'a>(golds: &'a [EvalGold], project_ids: &[String]) -> Vec<&'a EvalGold> {
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
    top_k: usize,
) -> Result<PairDiagnoseProjectReport, EvalError> {
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

    let request = AnalysisRequest::new(project_path);
    let diagnostics = engine
        .diagnose_formation_pairs(request, modes, top_k)
        .map_err(|error| EvalError::AnalysisFailed(error.to_string()))?;

    Ok(PairDiagnoseProjectReport {
        id: gold.id.clone(),
        skipped: false,
        skip_reason: None,
        modes: diagnostics,
    })
}

fn skipped_report(gold: &EvalGold, reason: String) -> PairDiagnoseProjectReport {
    PairDiagnoseProjectReport {
        id: gold.id.clone(),
        skipped: true,
        skip_reason: Some(reason),
        modes: Vec::new(),
    }
}

pub fn parse_mode_selection(raw: Option<&str>) -> PairDiagnoseModeSelection {
    match raw.unwrap_or("both").to_ascii_lowercase().as_str() {
        "legacy" => PairDiagnoseModeSelection::Legacy,
        "structural" => PairDiagnoseModeSelection::Structural,
        _ => PairDiagnoseModeSelection::Both,
    }
}

pub fn parse_project_ids(raw: Option<&str>) -> Vec<String> {
    raw.map(|value| {
        value
            .split(',')
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .map(str::to_string)
            .collect()
    })
    .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 기본_진단_프로젝트는_6개다() {
        assert_eq!(DEFAULT_PAIR_DIAGNOSE_IDS.len(), 6);
    }

    #[test]
    fn mode_selection_파싱() {
        assert_eq!(
            parse_mode_selection(Some("legacy")),
            PairDiagnoseModeSelection::Legacy
        );
        assert_eq!(
            parse_mode_selection(Some("structural")),
            PairDiagnoseModeSelection::Structural
        );
        assert_eq!(parse_mode_selection(None), PairDiagnoseModeSelection::Both);
    }
}
