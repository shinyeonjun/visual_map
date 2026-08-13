use crate::diagnostics::Diagnostic;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// 현재 엔진이 파일 확장자로 구분할 수 있는 언어다.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum Language {
    #[serde(rename = "javascript", alias = "javaScript")]
    JavaScript,
    #[serde(rename = "typescript", alias = "typeScript")]
    TypeScript,
    #[serde(rename = "python")]
    Python,
    #[serde(rename = "java")]
    Java,
    #[serde(rename = "c")]
    C,
    #[serde(rename = "cpp")]
    Cpp,
    #[serde(rename = "csharp", alias = "cSharp")]
    CSharp,
    #[serde(rename = "go")]
    Go,
    #[serde(rename = "rust")]
    Rust,
    #[serde(rename = "dart")]
    Dart,
    #[serde(rename = "unknown")]
    Unknown,
}

impl Language {
    pub fn from_key(key: &str) -> Option<Self> {
        Some(match key.to_ascii_lowercase().as_str() {
            "javascript" => Self::JavaScript,
            "typescript" => Self::TypeScript,
            "python" => Self::Python,
            "java" => Self::Java,
            "c" => Self::C,
            "cpp" => Self::Cpp,
            "csharp" => Self::CSharp,
            "go" => Self::Go,
            "rust" => Self::Rust,
            "dart" => Self::Dart,
            "unknown" => Self::Unknown,
            _ => return None,
        })
    }

    pub fn from_extension(extension: &str) -> Option<Self> {
        crate::config::LanguageRegistry::default().from_extension(extension)
    }

    pub fn key(&self) -> &'static str {
        match self {
            Self::JavaScript => "javascript",
            Self::TypeScript => "typescript",
            Self::Python => "python",
            Self::Java => "java",
            Self::C => "c",
            Self::Cpp => "cpp",
            Self::CSharp => "csharp",
            Self::Go => "go",
            Self::Rust => "rust",
            Self::Dart => "dart",
            Self::Unknown => "unknown",
        }
    }
}

/// 분석 엔진에 전달하는 프로젝트 분석 요청이다.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisRequest {
    pub root_path: PathBuf,
    #[serde(default)]
    pub options: AnalysisOptions,
}

impl AnalysisRequest {
    pub fn new(root_path: impl Into<PathBuf>) -> Self {
        Self {
            root_path: root_path.into(),
            options: AnalysisOptions::default(),
        }
    }
}

/// 프로젝트 스캔 동작을 조절하는 옵션이다.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AnalysisOptions {
    pub config: crate::config::AnalysisConfig,
    pub profile: bool,
    /// 파일 Facts 캐시를 사용하지 않고 매 파일을 다시 분석할지 정한다.
    pub use_fact_cache: bool,
}

impl Default for AnalysisOptions {
    fn default() -> Self {
        let config = crate::config::AnalysisConfig::default();
        Self {
            config,
            profile: false,
            use_fact_cache: true,
        }
    }
}

/// 엔진이 최종적으로 반환하는 상태다.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum AnalysisStatus {
    Ready,
    Partial,
    Failed,
}

/// 파일 하나의 AST 분석 상태다.
///
/// 파일 유닛은 파싱에 실패해도 진단용으로 생성될 수 있으므로, 유닛 개수만으로
/// 파싱 성공 여부를 추정하지 않는다.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum ParseStatus {
    #[default]
    NotAnalyzed,
    Parsed,
    ParsedWithErrors,
    Failed,
}

/// 프로젝트 식별 정보다. 이후 스냅샷·증분 분석의 기준으로 사용한다.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectContext {
    pub project_id: String,
    pub root_path: String,
    pub snapshot_id: String,
}

/// 프로젝트 안에서 발견된 파일 하나의 안정적인 메타데이터다.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    pub file_id: String,
    pub relative_path: String,
    pub language: Language,
    pub size_bytes: u64,
    pub line_count: u64,
    pub modified_unix_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
    pub is_test: bool,
    #[serde(default)]
    pub parse_status: ParseStatus,
}

/// 파일 인벤토리 통계다.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSummary {
    pub total_files: usize,
    pub total_bytes: u64,
    pub total_lines: u64,
    pub source_files: usize,
    pub languages: BTreeMap<String, usize>,
}

/// 현재 단계의 분석 결과다. 다음 단계에서 facts와 graph가 이 결과에 추가된다.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisResult {
    pub schema_version: String,
    pub analysis_id: String,
    pub status: AnalysisStatus,
    pub project: ProjectContext,
    pub files: Vec<FileEntry>,
    pub summary: ProjectSummary,
    pub diagnostics: Vec<Diagnostic>,
    pub elapsed_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overview: Option<crate::views::overview::OverviewResponse>,
    /// raw Overview와 분리된 정적 전처리 결과다. 기본 JSON에는 중복을
    /// 만들지 않도록 직렬화하지 않고, API 호출자와 `--prepared-output`이
    /// 선택적으로 사용한다.
    #[serde(skip)]
    pub preprocessed_overview: Option<crate::views::preprocessed::PreparedStaticOverview>,
}
