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
    pub test_suffixes: Vec<String>,
}

impl Default for PathPolicy {
    fn default() -> Self {
        default_section("paths")
    }
}

impl PathPolicy {
    pub fn is_test_path(&self, path: &str) -> bool {
        let normalized = path.replace('\\', "/").to_ascii_lowercase();
        normalized
            .split('/')
            .any(|part| self.test_directory_names.iter().any(|name| name == part))
            || self
                .test_suffixes
                .iter()
                .any(|suffix| normalized.ends_with(suffix))
    }
}

/// 도메인 후보 점수와 그룹화 기준이다.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DomainPolicy {
    pub minimum_token_length: usize,
    pub minimum_repeated_symbol_units: usize,
    pub maximum_candidate_evidence: usize,
    pub path_weight: u32,
    pub symbol_weight: u32,
    pub entrypoint_weight: u32,
    pub resource_weight: u32,
    pub framework_weight: u32,
    pub reference_weight: u32,
    pub confirmed_minimum_signal_families: usize,
    pub confirmed_minimum_score: u32,
    pub shared_ratio_numerator: u32,
    pub shared_ratio_denominator: u32,
    pub shared_minimum_score: u32,
    pub generic_tokens: BTreeSet<String>,
    pub cross_cutting_keys: BTreeSet<String>,
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
    pub internal_catalog_markers: Vec<String>,
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
    pub codex_executable: String,
    /// Codex CLI에 전달할 모델이다. 비어 있으면 CLI 기본 모델을 사용한다.
    pub codex_model: Option<String>,
    pub codex_timeout_ms: u64,
    pub codex_max_input_bytes: usize,
    /// 누락된 의미 항목을 작은 재요청으로 보완할 최대 횟수다.
    pub missing_item_retries: usize,
    pub maximum_label_length: usize,
    pub maximum_summary_length: usize,
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
