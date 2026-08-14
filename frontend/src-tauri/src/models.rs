use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AnalyzeRequest {
    pub(crate) project_path: String,
    pub(crate) engine_path: String,
    pub(crate) config_path: String,
    pub(crate) model: String,
    #[serde(default = "default_provider")]
    pub(crate) provider: String,
}

fn default_provider() -> String {
    "codex".into()
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClaudeStatus {
    pub(crate) version: String,
    pub(crate) executable: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CodexModelOption {
    pub(crate) slug: String,
    pub(crate) display_name: String,
    pub(crate) description: String,
    pub(crate) default_reasoning_level: Option<String>,
    pub(crate) supported_reasoning_levels: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClaudeModelCatalog {
    pub(crate) models: Vec<CodexModelOption>,
    pub(crate) selected_model: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CodexModelCatalog {
    pub(crate) executable: String,
    pub(crate) version: String,
    pub(crate) source: String,
    pub(crate) selected_model: Option<String>,
    pub(crate) models: Vec<CodexModelOption>,
    pub(crate) saved_provider: String,
    pub(crate) saved_claude_model: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AiSettings {
    pub(crate) executable: String,
    pub(crate) model: String,
    pub(crate) cli_version: String,
    pub(crate) catalog_source: String,
    #[serde(default)]
    pub(crate) provider: String,
    #[serde(default)]
    pub(crate) claude_model: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawCodexModelCatalog {
    #[serde(default)]
    pub(crate) models: Vec<RawCodexModel>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawCodexModel {
    pub(crate) slug: String,
    #[serde(default)]
    pub(crate) display_name: String,
    #[serde(default)]
    pub(crate) description: String,
    #[serde(default)]
    pub(crate) default_reasoning_level: Option<String>,
    #[serde(default)]
    pub(crate) supported_reasoning_levels: Vec<RawReasoningLevel>,
    #[serde(default)]
    pub(crate) visibility: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawReasoningLevel {
    pub(crate) effort: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MapDomain {
    pub(crate) domain_id: String,
    pub(crate) name: String,
    pub(crate) summary: String,
    pub(crate) status: String,
    pub(crate) confidence: u32,
    pub(crate) units: usize,
    pub(crate) features: usize,
    pub(crate) entrypoints: usize,
    pub(crate) dependencies: Vec<String>,
    pub(crate) signals: Vec<String>,
    pub(crate) feature_items: Vec<MapFeature>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MapFeature {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) summary: Option<String>,
    pub(crate) kind: String,
    pub(crate) entrypoints: usize,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MapStats {
    pub(crate) files: usize,
    pub(crate) units: usize,
    pub(crate) features: usize,
    pub(crate) flows: usize,
    pub(crate) resources: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AnalyzeResponse {
    pub(crate) project_path: String,
    pub(crate) workspace_path: String,
    pub(crate) semantic_result_path: String,
    pub(crate) domains: Vec<MapDomain>,
    pub(crate) stats: MapStats,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AnalysisProgress {
    pub(crate) phase: String,
    pub(crate) label: String,
    pub(crate) detail: String,
    pub(crate) percent: u8,
    pub(crate) step: u8,
    pub(crate) total_steps: u8,
    pub(crate) current: Option<usize>,
    pub(crate) total: Option<usize>,
    pub(crate) indeterminate: bool,
    pub(crate) elapsed_ms: u128,
}

#[derive(Debug)]
pub(crate) struct WorkspacePaths {
    pub(crate) directory: PathBuf,
    pub(crate) config: PathBuf,
    pub(crate) static_output: PathBuf,
    pub(crate) clean_output: PathBuf,
    pub(crate) context_output: PathBuf,
    pub(crate) semantic_output: PathBuf,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WorkspaceMetadata {
    pub(crate) project_path: String,
    pub(crate) workspace_key: String,
    pub(crate) model: String,
    pub(crate) environment: String,
    pub(crate) updated_at_ms: u128,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SemanticDomain {
    pub(crate) domain_id: String,
    #[serde(default)]
    pub(crate) source_domain_ids: Vec<String>,
    pub(crate) name: String,
    pub(crate) summary: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SemanticResult {
    #[serde(default)]
    pub(crate) domains: Vec<SemanticDomain>,
}
