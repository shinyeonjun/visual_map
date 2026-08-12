use crate::domain::{DomainGroup, DomainRelation};
use crate::frameworks::registry::detector::FrameworkDetection;
use serde::{Deserialize, Serialize};

/// Codex에 전달하는 하나의 분할 컨텍스트다.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticChunk {
    /// 0부터 시작하는 청크 순서다.
    pub index: usize,
    /// 전체 청크 개수다.
    pub count: usize,
    pub context: SemanticContext,
}

/// Codex에 전달할 축약 도메인 컨텍스트.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticContext {
    pub domains: Vec<DomainGroup>,
    pub relations: Vec<DomainRelation>,
    pub frameworks: Vec<FrameworkDetection>,
}

/// [DEV ONLY] Codex에 전달하기 직전의 축약 컨텍스트를 저장하는 디버깅 산출물이다.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticContextArtifact {
    pub schema_version: &'static str,
    pub max_input_bytes: usize,
    pub max_context_bytes: usize,
    pub chunk_count: usize,
    pub chunks: Vec<SemanticChunk>,
}

/// Codex 후보정 컨텍스트를 만드는 내부 단계별 시간이다.
#[derive(Debug, Clone, Copy, Default)]
pub struct SemanticContextTimings {
    pub domain_compaction_ms: u64,
    pub chunk_sizing_ms: u64,
    pub chunk_materialization_ms: u64,
}
