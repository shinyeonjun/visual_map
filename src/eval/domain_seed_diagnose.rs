//! domain seed diagnostics eval.

use super::gold::EvalGold;
use super::pair_diagnose::select_golds;
use super::EvalError;
use crate::domain::DomainSeedDiagnostics;
use crate::model::AnalysisRequest;
use crate::AnalysisEngine;
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DomainSeedProjectReport {
    pub id: String,
    pub skipped: bool,
    pub skip_reason: Option<String>,
    pub diagnostics: Option<DomainSeedDiagnostics>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DomainSeedCatalogReport {
    pub projects: Vec<DomainSeedProjectReport>,
}

pub fn diagnose_domain_seed_catalog(
    catalog_path: &Path,
    project_ids: &[String],
) -> Result<DomainSeedCatalogReport, EvalError> {
    let golds = super::load_catalog(catalog_path)?;
    let selected = select_golds(&golds, project_ids);
    let engine = AnalysisEngine::default();
    let mut projects = Vec::with_capacity(selected.len());

    for gold in selected {
        projects.push(diagnose_project(&engine, gold)?);
    }

    Ok(DomainSeedCatalogReport { projects })
}

fn diagnose_project(
    engine: &AnalysisEngine,
    gold: &EvalGold,
) -> Result<DomainSeedProjectReport, EvalError> {
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

    let diagnostics = engine
        .diagnose_domain_seeds(AnalysisRequest::new(project_path))
        .map_err(|error| EvalError::AnalysisFailed(error.to_string()))?;

    Ok(DomainSeedProjectReport {
        id: gold.id.clone(),
        skipped: false,
        skip_reason: None,
        diagnostics: Some(diagnostics),
    })
}

fn skipped_report(gold: &EvalGold, reason: String) -> DomainSeedProjectReport {
    DomainSeedProjectReport {
        id: gold.id.clone(),
        skipped: true,
        skip_reason: Some(reason),
        diagnostics: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::pair_diagnose::DEFAULT_PAIR_DIAGNOSE_IDS;

    #[test]
    fn domain_seed_리포트는_직렬화된다() {
        let report = DomainSeedCatalogReport {
            projects: vec![DomainSeedProjectReport {
                id: "demo".into(),
                skipped: true,
                skip_reason: Some("missing".into()),
                diagnostics: None,
            }],
        };
        let json = serde_json::to_string(&report).expect("serialize");
        assert!(json.contains("projects"));
    }

    #[test]
    fn 기본_진단_프로젝트는_6개다() {
        assert_eq!(DEFAULT_PAIR_DIAGNOSE_IDS.len(), 6);
    }
}
