//! 분석 정책과 규칙을 한곳에서 관리하는 설정 영역.
//!
//! TOML/JSON 로딩은 `loader`, 각 단계의 정책 타입은 `policies`가 담당한다.
//! 외부 사용자는 기존처럼 `crate::config::SemanticPolicy` 등으로 접근한다.

mod loader;
mod policies;

pub use loader::{AnalysisConfig, ConfigLoadError};
pub use policies::{
    AnalysisLimits, CleanPolicy, DomainClusteringMode, DomainFormationMode, DomainPolicy,
    FrameworkPolicy, LanguageRegistry, ParserPolicy, PathPolicy, PostprocessPolicy,
    ResourceNameSource, ResourceRule, RoutePatternKind, RouteRule, ScanPolicy, SemanticPolicy,
};

#[cfg(test)]
mod tests {
    use super::{AnalysisConfig, LanguageRegistry};

    #[test]
    fn 설정은_부분_override하고_나머지는_기본값을_사용한다() {
        let config = AnalysisConfig::from_json_str(
            r#"{
                "domains": {"confirmedMinimumScore": 99},
                "semantic": {"maximumLabelLength": 90}
            }"#,
        )
        .expect("부분 설정 JSON은 해석되어야 한다");

        assert_eq!(config.domains.confirmed_minimum_score, 99);
        assert_eq!(config.semantic.maximum_label_length, 90);
        assert_eq!(config.semantic.codex_executable, "codex");
        assert!(config.languages.from_extension("ts").is_some());
        assert!(!config.paths.ignored_directories.is_empty());
        assert!(config.paths.is_archived_path("legacy/backend/health.py"));
        assert!(!config.paths.is_production_path("legacy/backend/health.py"));
        assert!(!config
            .paths
            .ignored_directories
            .iter()
            .any(|name| name == "legacy"));
        assert_eq!(config.scan.max_file_size_bytes, 10 * 1024 * 1024);
    }

    #[test]
    fn toml_설정은_부분_override하고_기본값과_병합된다() {
        let config = AnalysisConfig::from_toml_str(
            "[domains]\nconfirmedMinimumScore = 42\n[semantic]\nmaximumSummaryLength = 400\n",
        )
        .expect("부분 설정 TOML은 해석되어야 한다");

        assert_eq!(config.domains.confirmed_minimum_score, 42);
        assert_eq!(config.semantic.maximum_summary_length, 400);
        assert_eq!(config.semantic.codex_executable, "codex");
        assert!(config.languages.from_extension("rs").is_some());
    }

    #[test]
    fn 언어_레지스트리는_확장자를_설정값으로_해석한다() {
        let mut registry = LanguageRegistry::default();
        registry
            .extensions
            .entry("javascript".into())
            .or_default()
            .push("custom-js".into());

        assert_eq!(
            registry.from_extension(".custom-js"),
            Some(crate::model::Language::JavaScript)
        );
    }
}
