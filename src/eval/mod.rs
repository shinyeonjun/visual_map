//! 사람 기준 정답(gold)과 분석 결과를 비교하는 정확도 평가.

mod gold;
mod score;
mod snapshot;

pub use gold::{
    DomainAlias, EvalCatalog, EvalGold, EvalMode, FeatureGold, FlowInvariantGold,
};
pub use score::{score_gold, EvalFinding, EvalReport};
pub use snapshot::{
    snapshot_from_clean, snapshot_from_overview, EvalSnapshot, SnapDomain, SnapFeature,
};

use crate::clean::CleanBundleError;
use crate::model::AnalysisResult;
use crate::views::overview::OverviewResponse;
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub enum EvalError {
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    Serialization(serde_json::Error),
    Clean(CleanBundleError),
    MissingOverview,
    MissingClean(PathBuf),
}

impl std::fmt::Display for EvalError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io { path, source } => {
                write!(
                    formatter,
                    "평가 파일을 읽지 못했습니다 ({}): {source}",
                    path.display()
                )
            }
            Self::Serialization(error) => write!(formatter, "평가 JSON을 해석하지 못했습니다: {error}"),
            Self::Clean(error) => write!(formatter, "{error}"),
            Self::MissingOverview => write!(formatter, "분석 결과에 Overview가 없습니다."),
            Self::MissingClean(path) => {
                write!(
                    formatter,
                    "Clean bundle 디렉터리가 없습니다: {}",
                    path.display()
                )
            }
        }
    }
}

impl std::error::Error for EvalError {}

pub fn load_gold(path: &Path) -> Result<EvalGold, EvalError> {
    let source = read_text(path)?;
    serde_json::from_str(&source).map_err(EvalError::Serialization)
}

pub fn load_catalog(path: &Path) -> Result<Vec<EvalGold>, EvalError> {
    if path.is_dir() {
        let index = path.join("catalog.json");
        if index.is_file() {
            return load_catalog(&index);
        }
        return load_gold_files_in_dir(path);
    }

    let source = read_text(path)?;
    if let Ok(index) = serde_json::from_str::<EvalCatalog>(&source) {
        if !index.files.is_empty() {
            let base = path.parent().unwrap_or(path);
            return index
                .files
                .iter()
                .map(|file| load_gold(&base.join(file)))
                .collect();
        }
    }

    let gold: EvalGold = serde_json::from_str(&source).map_err(EvalError::Serialization)?;
    if !gold.entries.is_empty() {
        return Ok(gold.entries);
    }
    if gold.id.is_empty() {
        return Ok(Vec::new());
    }
    Ok(vec![gold])
}

pub fn evaluate_clean(gold: &EvalGold, clean_dir: &Path) -> Result<EvalReport, EvalError> {
    if !clean_dir.is_dir() {
        return Err(EvalError::MissingClean(clean_dir.to_path_buf()));
    }
    let loaded = crate::clean::load(clean_dir).map_err(EvalError::Clean)?;
    let snapshot = snapshot_from_clean(&loaded.prepared);
    Ok(score_gold(gold, &snapshot))
}

pub fn evaluate_result(gold: &EvalGold, result: &AnalysisResult) -> Result<EvalReport, EvalError> {
    let overview = result.overview.as_ref().ok_or(EvalError::MissingOverview)?;
    Ok(evaluate_overview(gold, overview))
}

pub fn evaluate_overview(gold: &EvalGold, overview: &OverviewResponse) -> EvalReport {
    score_gold(gold, &snapshot_from_overview(overview))
}

pub fn evaluate_analysis_file(gold: &EvalGold, path: &Path) -> Result<EvalReport, EvalError> {
    let source = read_text(path)?;
    let value: serde_json::Value =
        serde_json::from_str(&source).map_err(EvalError::Serialization)?;
    if value.get("overview").is_some() {
        let result: AnalysisResult =
            serde_json::from_value(value).map_err(EvalError::Serialization)?;
        return evaluate_result(gold, &result);
    }
    let overview: OverviewResponse =
        serde_json::from_value(value).map_err(EvalError::Serialization)?;
    Ok(evaluate_overview(gold, &overview))
}

pub fn evaluate_catalog(catalog_path: &Path, clean_root: &Path) -> Result<CatalogReport, EvalError> {
    let golds = load_catalog(catalog_path)?;
    let mut reports = Vec::with_capacity(golds.len());
    for gold in &golds {
        let clean_dir = clean_root.join(clean_dir_name(gold));
        match evaluate_clean(gold, &clean_dir) {
            Ok(report) => reports.push(report),
            Err(EvalError::MissingClean(path)) => reports.push(unavailable_report(
                gold,
                format!("Clean bundle 디렉터리가 없습니다: {}", path.display()),
            )),
            Err(error) => return Err(error),
        }
    }
    Ok(summarize_reports(reports))
}

fn unavailable_report(gold: &EvalGold, message: String) -> EvalReport {
    EvalReport {
        id: gold.id.clone(),
        set: gold.set.clone(),
        passed: false,
        domain_hits: 0,
        domain_expected: gold.must_have_domains.len(),
        feature_hits: 0,
        feature_expected: gold.must_have_features.len(),
        flow_hits: 0,
        flow_expected: gold.flow_invariants.len(),
        findings: vec![EvalFinding {
            layer: "bundle".into(),
            kind: "missingClean".into(),
            message,
        }],
    }
}

pub fn summarize_reports(reports: Vec<EvalReport>) -> CatalogReport {
    let passed = reports.iter().filter(|report| report.passed).count();
    let failed = reports.len() - passed;
    CatalogReport {
        reports,
        passed,
        failed,
    }
}

pub fn clean_dir_name(gold: &EvalGold) -> String {
    gold.project_path
        .as_deref()
        .map(Path::new)
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or(gold.id.as_str())
        .to_string()
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogReport {
    pub reports: Vec<EvalReport>,
    pub passed: usize,
    pub failed: usize,
}

fn read_text(path: &Path) -> Result<String, EvalError> {
    fs::read_to_string(path).map_err(|source| EvalError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn load_gold_files_in_dir(dir: &Path) -> Result<Vec<EvalGold>, EvalError> {
    let mut files: Vec<PathBuf> = fs::read_dir(dir)
        .map_err(|source| EvalError::Io {
            path: dir.to_path_buf(),
            source,
        })?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension().and_then(|ext| ext.to_str()) == Some("json")
                && path.file_name().and_then(|name| name.to_str()) != Some("catalog.json")
        })
        .collect();
    files.sort();
    files.iter().map(|path| load_gold(path)).collect()
}
