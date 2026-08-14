//! 정적 분석 결과를 AI 입력용 카드로 변환하는 후처리 영역.
//!
//! 이 모듈은 프로젝트를 다시 분석하지 않는다. 이미 생성된
//! `AnalysisResult`의 정적 Overview를 읽어 도메인·기능·대표 실행 흐름만
//! 남긴다. 원본 결과와 프론트엔드용 Overview는 변경하지 않는다.

mod cards;
mod context_chunk;
mod context_metadata;
mod context_profile;
mod domains;
mod features;
mod flows;
mod from_clean;
mod indexes;
pub mod model;
mod partition;
mod selection;

use crate::config::AnalysisConfig;
use crate::model::AnalysisResult;
pub use model::{AiContextBundle, AiSemanticContext};

#[derive(Debug)]
pub enum PostprocessError {
    MissingOverview,
    CleanBundle(String),
}

impl std::fmt::Display for PostprocessError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingOverview => write!(formatter, "분석 결과에 overview가 없습니다."),
            Self::CleanBundle(message) => {
                write!(formatter, "Clean bundle 로드 실패: {message}")
            }
        }
    }
}

impl std::error::Error for PostprocessError {}

pub fn build_ai_context(
    result: &AnalysisResult,
    config: &AnalysisConfig,
) -> Result<AiContextBundle, PostprocessError> {
    cards::build_bundle(result, config)
}

pub fn build_ai_context_from_clean(
    clean_dir: &std::path::Path,
    config: &AnalysisConfig,
) -> Result<AiContextBundle, PostprocessError> {
    let bundle = crate::clean::load(clean_dir)
        .map_err(|error| PostprocessError::CleanBundle(error.to_string()))?;
    let result = from_clean::to_analysis_result(&bundle);
    cards::build_bundle(&result, config)
}
