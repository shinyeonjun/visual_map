//! Visual Map 코드 분석 엔진의 실행 기반이다.
//!
//! 파일 인벤토리, 10개 언어의 공통 Facts, 프레임워크 감지, 도메인 후보 그룹화,
//! Codex 의미 보정, 프론트엔드 Overview projection을 제공한다.

pub mod analysis;
pub mod config;
pub mod diagnostics;
pub mod domain;
pub mod engine;
pub mod facts;
pub mod flow;
pub mod frameworks;
pub mod graph;
pub mod languages;
pub mod model;
pub mod pipeline;
pub mod postprocess;
pub mod project;
pub mod semantic;
pub mod views;

pub use analysis::{analyze, AnalysisEngine};
pub use model::{AnalysisOptions, AnalysisRequest, AnalysisResult};

/// 엔진 실행 중 발생한 치명적 오류다.
#[derive(Debug)]
pub enum EngineError {
    /// 프로젝트 경로가 존재하지 않는다.
    ProjectNotFound(std::path::PathBuf),
    /// 프로젝트 경로가 디렉터리가 아니다.
    ProjectIsNotDirectory(std::path::PathBuf),
    /// 프로젝트 경로를 절대 경로로 확정하지 못했다.
    Canonicalize {
        path: std::path::PathBuf,
        source: std::io::Error,
    },
    /// 파일 시스템을 읽지 못했다.
    Io {
        path: std::path::PathBuf,
        source: std::io::Error,
    },
    /// [DEV ONLY] Codex 중간 산출물 파일을 저장하지 못했다.
    ///
    /// 현재는 개발용 컨텍스트 덤프에서만 사용한다. 해당 덤프를 제거할 때
    /// 이 오류 변형도 함께 제거하거나 운영용 출력 오류와 분리한다.
    OutputWrite {
        path: std::path::PathBuf,
        source: std::io::Error,
    },
    /// 결과 JSON을 만들지 못했다.
    Serialization(serde_json::Error),
}

impl std::fmt::Display for EngineError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ProjectNotFound(path) => {
                write!(
                    formatter,
                    "프로젝트 경로를 찾을 수 없습니다: {}",
                    path.display()
                )
            }
            Self::ProjectIsNotDirectory(path) => {
                write!(
                    formatter,
                    "프로젝트 경로가 디렉터리가 아닙니다: {}",
                    path.display()
                )
            }
            Self::Canonicalize { path, source } => {
                write!(
                    formatter,
                    "프로젝트 경로를 확정하지 못했습니다 ({}): {source}",
                    path.display()
                )
            }
            Self::Io { path, source } => {
                write!(
                    formatter,
                    "파일 시스템을 읽지 못했습니다 ({}): {source}",
                    path.display()
                )
            }
            Self::OutputWrite { path, source } => {
                write!(
                    formatter,
                    "중간 산출물을 저장하지 못했습니다 ({}): {source}",
                    path.display()
                )
            }
            Self::Serialization(error) => {
                write!(formatter, "분석 결과를 직렬화하지 못했습니다: {error}")
            }
        }
    }
}

impl std::error::Error for EngineError {}
