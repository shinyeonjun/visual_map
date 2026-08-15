//! 각 분석 단계에서 사용하는 정책 타입과 설정 기반 규칙이다.

use crate::model::Language;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

use super::loader::default_section;

/// 파일 인벤토리와 소스 읽기 동작을 조절하는 설정이다.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanPolicy {
    pub compute_hashes: bool,
    pub include_hidden: bool,
    pub max_file_size_bytes: u64,
    /// 동일한 AnalysisEngine 인스턴스에서 재사용할 파일 Facts 최대 개수다.
    pub fact_cache_max_entries: usize,
    /// 확장자가 없지만 프레임워크 DSL로 분석할 파일의 이름이다.
    pub framework_config_file_names: Vec<String>,
}

/// 프로젝트 전체 분석 결과가 메모리와 출력에서 폭발하지 않도록 하는 한도다.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisLimits {
    pub max_files: usize,
    pub max_total_bytes: u64,
    pub max_units: usize,
    pub max_references: usize,
    pub max_bindings: usize,
    pub max_decorators: usize,
    pub max_call_sites: usize,
    pub max_entrypoints: usize,
    pub max_resources: usize,
    pub max_control_flow_facts: usize,
    pub max_execution_flows: usize,
    pub max_flow_nodes: usize,
    pub max_flow_edges: usize,
    pub max_output_bytes: usize,
}

impl Default for AnalysisLimits {
    fn default() -> Self {
        default_section("limits")
    }
}

impl Default for ScanPolicy {
    fn default() -> Self {
        default_section("scan")
    }
}

/// 언어별 확장자와 외부 표시 키를 관리한다.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LanguageRegistry {
    pub extensions: BTreeMap<String, Vec<String>>,
}

impl Default for LanguageRegistry {
    fn default() -> Self {
        default_section("languages")
    }
}

impl LanguageRegistry {
    pub fn from_extension(&self, extension: &str) -> Option<Language> {
        let extension = extension.trim_start_matches('.').to_ascii_lowercase();
        self.extensions.iter().find_map(|(key, extensions)| {
            extensions
                .iter()
                .any(|candidate| candidate.eq_ignore_ascii_case(&extension))
                .then(|| Language::from_key(key))
                .flatten()
        })
    }

    pub fn key(&self, language: Language) -> String {
        language.key().to_string()
    }
}

/// 프로젝트 경로와 테스트 파일 판별 규칙이다.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PathPolicy {
    pub ignored_directories: Vec<String>,
    pub test_directory_names: Vec<String>,
    pub test_file_prefixes: Vec<String>,
    pub test_suffixes: Vec<String>,
    #[serde(default = "default_experiment_directory_names")]
    pub experiment_directory_names: Vec<String>,
    #[serde(default = "default_script_directory_names")]
    pub script_directory_names: Vec<String>,
    #[serde(default = "default_generated_directory_names")]
    pub generated_directory_names: Vec<String>,
    #[serde(default = "default_archived_directory_names")]
    pub archived_directory_names: Vec<String>,
}

/// 코드 경로의 역할 분류다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathRole {
    Production,
    Test,
    Experiment,
    Script,
    Generated,
    Archived,
}

impl Default for PathPolicy {
    fn default() -> Self {
        default_section("paths")
    }
}

impl PathPolicy {
    pub fn path_role(&self, path: &str) -> PathRole {
        if self.matches_test_path(path) {
            return PathRole::Test;
        }
        let normalized = path.replace('\\', "/").to_ascii_lowercase();
        if self.matches_directory_name(&normalized, &self.experiment_directory_names) {
            return PathRole::Experiment;
        }
        if self.matches_directory_name(&normalized, &self.script_directory_names) {
            return PathRole::Script;
        }
        if self.matches_directory_name(&normalized, &self.generated_directory_names) {
            return PathRole::Generated;
        }
        if self.matches_directory_name(&normalized, &self.archived_directory_names) {
            return PathRole::Archived;
        }
        PathRole::Production
    }

    pub fn is_production_path(&self, path: &str) -> bool {
        self.path_role(path) == PathRole::Production
    }

    pub fn is_archived_path(&self, path: &str) -> bool {
        self.path_role(path) == PathRole::Archived
    }

    pub fn is_test_path(&self, path: &str) -> bool {
        self.path_role(path) == PathRole::Test
    }

    fn matches_directory_name(&self, normalized_path: &str, names: &[String]) -> bool {
        normalized_path
            .split('/')
            .any(|part| names.iter().any(|name| name == part))
    }

    fn matches_test_path(&self, path: &str) -> bool {
        let normalized = path.replace('\\', "/").to_ascii_lowercase();
        normalized
            .split('/')
            .any(|part| self.test_directory_names.iter().any(|name| name == part))
            || normalized.rsplit('/').next().is_some_and(|file_name| {
                self.test_file_prefixes
                    .iter()
                    .any(|prefix| file_name.starts_with(prefix))
            })
            || self
                .test_suffixes
                .iter()
                .any(|suffix| normalized.ends_with(suffix))
    }
}

fn default_experiment_directory_names() -> Vec<String> {
    vec![
        "experiments".into(),
        "experiment".into(),
        "sandbox".into(),
        "poc".into(),
    ]
}

fn default_script_directory_names() -> Vec<String> {
    vec!["scripts".into(), "tools".into()]
}

fn default_generated_directory_names() -> Vec<String> {
    vec!["generated".into(), "gen".into()]
}

fn default_archived_directory_names() -> Vec<String> {
    vec![
        "legacy".into(),
        "deprecated".into(),
        "archive".into(),
        "archives".into(),
    ]
}

/// 도메인 후보 점수와 그룹화 기준이다.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DomainPolicy {
    pub minimum_token_length: usize,
    pub minimum_repeated_symbol_units: usize,
    pub maximum_candidate_evidence: usize,
    pub confirmed_minimum_signal_families: usize,
    pub confirmed_minimum_score: u32,
    pub shared_ratio_numerator: u32,
    pub shared_ratio_denominator: u32,
    pub shared_minimum_score: u32,
    pub generic_tokens: BTreeSet<String>,
    pub cross_cutting_keys: BTreeSet<String>,
    #[serde(default = "default_feature_http_match_weight")]
    pub feature_http_match_weight: f64,
    #[serde(default = "default_feature_call_weight")]
    pub feature_call_weight: f64,
    #[serde(default = "default_feature_flow_weight")]
    pub feature_flow_weight: f64,
    #[serde(default = "default_feature_resource_weight")]
    pub feature_resource_weight: f64,
    #[serde(default = "default_feature_path_weight")]
    pub feature_path_weight: f64,
    #[serde(default = "default_feature_lexical_weight")]
    pub feature_lexical_weight: f64,
    #[serde(default = "default_domain_cluster_min")]
    pub domain_cluster_min: usize,
    #[serde(default = "default_domain_cluster_max")]
    pub domain_cluster_max: usize,
    #[serde(default = "default_domain_cluster_merge_threshold")]
    pub domain_cluster_merge_threshold: f64,
}

impl Default for DomainPolicy {
    fn default() -> Self {
        default_section("domains")
    }
}

impl DomainPolicy {
    pub fn is_generic(&self, token: &str) -> bool {
        self.generic_tokens.contains(token)
    }

    pub fn shared_threshold(&self, score: u32) -> u32 {
        (score
            .saturating_mul(self.shared_ratio_numerator)
            .checked_div(self.shared_ratio_denominator.max(1))
            .unwrap_or(0))
        .max(self.shared_minimum_score)
    }
}

fn default_feature_http_match_weight() -> f64 {
    0.40
}

fn default_feature_call_weight() -> f64 {
    0.15
}

fn default_feature_flow_weight() -> f64 {
    0.15
}

fn default_feature_resource_weight() -> f64 {
    0.10
}

fn default_feature_path_weight() -> f64 {
    0.05
}

fn default_feature_lexical_weight() -> f64 {
    0.15
}

fn default_domain_cluster_min() -> usize {
    6
}

fn default_domain_cluster_max() -> usize {
    24
}

fn default_domain_cluster_merge_threshold() -> f64 {
    0.08
}

/// 언어 공통 line fact 추출 규칙이다.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParserPolicy {
    pub dynamic_call_patterns: BTreeMap<String, Vec<String>>,
    pub route_rules: Vec<RouteRule>,
    pub resource_rules: Vec<ResourceRule>,
    pub sql_pattern: String,
    pub default_http_method: String,
    pub default_http_path: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RoutePatternKind {
    MethodAndPath,
    PathOnly,
    Attribute,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteRule {
    pub languages: Vec<String>,
    pub pattern: String,
    pub kind: RoutePatternKind,
}

/// 호출 인자에서 외부 자원 접근을 추출하는 설정 기반 규칙이다.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceRule {
    pub languages: Vec<String>,
    pub callee_patterns: Vec<String>,
    pub kind: String,
    pub mode: String,
    pub argument_index: usize,
    #[serde(default)]
    pub name_source: ResourceNameSource,
    /// qualified callee를 외부 모듈/API로 확정하려면 import binding이
    /// 실제로 존재해야 하는지 정한다.
    #[serde(default)]
    pub requires_import: bool,
}

/// 자원 이름을 호출 인자에서 읽을지 수신 객체에서 읽을지 정한다.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum ResourceNameSource {
    #[default]
    Argument,
    Receiver,
    LiteralOrReceiver,
}

impl Default for ParserPolicy {
    fn default() -> Self {
        default_section("parser")
    }
}

/// 프레임워크 감지 정책이다.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameworkPolicy {
    pub manifests: Vec<String>,
    pub initial_confidence: f32,
    pub confidence_increment: f32,
    pub maximum_confidence: f32,
}

impl Default for FrameworkPolicy {
    fn default() -> Self {
        default_section("frameworks")
    }
}

/// Codex 컨텍스트 축약 정책이다.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticPolicy {
    /// 의미 분석에 사용할 provider다. "codex" 또는 "claude".
    #[serde(default = "default_provider")]
    pub provider: String,
    pub codex_executable: String,
    /// Codex CLI에 전달할 모델이다. 비어 있으면 CLI 기본 모델을 사용한다.
    pub codex_model: Option<String>,
    #[serde(default = "default_claude_executable")]
    pub claude_executable: String,
    #[serde(default)]
    pub claude_model: Option<String>,
    pub codex_timeout_ms: u64,
    pub codex_max_input_bytes: usize,
    /// 누락된 의미 항목을 작은 재요청으로 보완할 최대 횟수다.
    pub missing_item_retries: usize,
    pub maximum_label_length: usize,
    pub maximum_summary_length: usize,
}

fn default_provider() -> String {
    "codex".into()
}

fn default_claude_executable() -> String {
    "claude".into()
}

impl Default for SemanticPolicy {
    fn default() -> Self {
        default_section("semantic")
    }
}

/// 정적 Overview를 Codex 입력 카드로 축약하는 정책이다.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PostprocessPolicy {
    /// Codex에 한 번 전달할 컨텍스트의 직렬화 바이트 예산이다.
    pub target_budget_bytes: usize,
    /// 전역 프로젝트 요약에 먼저 예약하는 바이트 예산이다.
    pub global_summary_reserve_bytes: usize,
    /// 청크에 함께 표시할 인접 도메인 요약의 최대 개수다.
    pub max_adjacent_domains: usize,
    pub max_files_per_domain: usize,
    pub max_entrypoints_per_domain: usize,
    pub max_resources_per_domain: usize,
    pub max_symbols_per_feature: usize,
    pub max_paths_per_feature: usize,
    pub max_evidence_ids_per_domain: usize,
    pub max_flow_nodes: usize,
    pub max_flow_edges: usize,
    pub domain_overlap_percent: u32,
    pub include_cross_cutting_domains: bool,
    pub flow_entrypoint_weight: u32,
    pub flow_resource_weight: u32,
    pub flow_dynamic_weight: u32,
    pub flow_complexity_weight: u32,
    pub feature_entrypoint_weight: u32,
    pub feature_resource_weight: u32,
    pub feature_dynamic_weight: u32,
    pub feature_complexity_weight: u32,
    pub feature_complexity_cap: usize,
    /// 도메인 신호를 계산할 때 사용하는 가중치다. 합계가 100일 필요는
    /// 없으며, 결과는 내부에서 동일한 기준으로 정규화한다.
    pub signal_anchor_weight: u32,
    pub signal_density_weight: u32,
    pub signal_specificity_weight: u32,
    pub signal_confidence_weight: u32,
    /// 청크 분할 시 도메인 관계의 확정도별 가중치다.
    pub partition_confirmed_weight: u32,
    pub partition_candidate_weight: u32,
    pub partition_unknown_weight: u32,
}

impl Default for PostprocessPolicy {
    fn default() -> Self {
        default_section("postprocess")
    }
}

/// 정적 Clean bundle의 저장 단위와 분할 정책이다.
///
/// 이 값은 의미 데이터를 제거하는 기준이 아니라 파일을 나누는 기준이다.
/// 따라서 part 크기가 바뀌어도 Clean 모델의 내용과 ID는 바뀌지 않는다.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanPolicy {
    /// 각 dataset part의 목표 바이트다. 하나의 레코드가 이보다 크면
    /// 레코드를 쪼개지 않고 해당 part 하나에 그대로 저장한다.
    pub part_target_bytes: usize,
}

impl Default for CleanPolicy {
    fn default() -> Self {
        default_section("clean")
    }
}

#[cfg(test)]
mod tests {
    use super::{PathPolicy, PathRole};

    #[test]
    fn legacy_경로는_archived이고_production이_아니다() {
        let policy = PathPolicy::default();
        assert_eq!(
            policy.path_role("legacy/backend/health.py"),
            PathRole::Archived
        );
        assert!(policy.is_archived_path("src/deprecated/old.ts"));
        assert!(!policy.is_production_path("archive/api.py"));
        assert!(policy.is_production_path("server/app.py"));
    }
}
