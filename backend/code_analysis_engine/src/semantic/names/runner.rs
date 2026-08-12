//! 이름 전용 컨텍스트 생성·Codex 호출·검증 오케스트레이션.

use super::context::{build_context, chunk_context, NameContextArtifact};
use super::prompt::build as build_prompt;
use super::response::{NameProposal, NameSuggestion};
use crate::model::AnalysisResult;
use crate::semantic::codex::CodexProvider;
use crate::EngineError;
use serde::Serialize;
use std::collections::BTreeSet;
use std::path::Path;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NamedDomain {
    pub id: String,
    pub current_name: String,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NamedModule {
    pub id: String,
    pub current_name: String,
    pub domain_ids: Vec<String>,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NameAnalysisResult {
    pub schema_version: &'static str,
    pub status: String,
    pub chunk_count: usize,
    pub completed_chunks: usize,
    pub failed_chunks: usize,
    pub context_bytes: usize,
    pub domains: Vec<NamedDomain>,
    pub modules: Vec<NamedModule>,
}

pub struct NameAnalyzer {
    pub provider: CodexProvider,
}

impl NameAnalyzer {
    pub fn from_result(
        &self,
        result: &AnalysisResult,
        max_context_bytes: usize,
    ) -> Result<NameAnalysisResult, EngineError> {
        let overview = result.overview.as_ref().ok_or_else(|| {
            EngineError::Serialization(serde_json::Error::io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "분석 결과에 overview가 없습니다.",
            )))
        })?;
        let context = build_context(overview);
        let context_bytes = serde_json::to_vec(&context)
            .map(|bytes| bytes.len())
            .map_err(EngineError::Serialization)?;
        let chunks = chunk_context(&context, max_context_bytes);
        let mut proposals = Vec::new();
        let mut failed_chunks = 0;
        for chunk in &chunks {
            let prompt = build_prompt(&chunk.context, chunk.index, chunk.count)
                .map_err(EngineError::Serialization)?;
            match self
                .provider
                .review_name_prompt(&prompt, Path::new(&result.project.root_path))
            {
                Ok(proposal) => proposals.push(proposal),
                Err(_) => failed_chunks += 1,
            }
        }

        let domains = merge_domain_names(&context, &proposals);
        let modules = merge_module_names(&context, &proposals);
        let completed_chunks = proposals.len();
        let status = if completed_chunks == 0 && !chunks.is_empty() {
            "failed"
        } else if failed_chunks > 0 {
            "partial"
        } else {
            "completed"
        };

        Ok(NameAnalysisResult {
            schema_version: "codex-name-result.v1",
            status: status.into(),
            chunk_count: chunks.len(),
            completed_chunks,
            failed_chunks,
            context_bytes,
            domains,
            modules,
        })
    }

    pub fn context_from_result(
        &self,
        result: &AnalysisResult,
        max_context_bytes: usize,
    ) -> Result<NameContextArtifact, EngineError> {
        let overview = result.overview.as_ref().ok_or_else(|| {
            EngineError::Serialization(serde_json::Error::io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "분석 결과에 overview가 없습니다.",
            )))
        })?;
        let context = build_context(overview);
        let chunks = super::context::chunk_context(&context, max_context_bytes);
        Ok(NameContextArtifact {
            schema_version: "codex-name-context.v1",
            chunk_count: chunks.len(),
            chunks,
        })
    }
}

fn merge_domain_names(
    context: &super::context::NameContext,
    proposals: &[NameProposal],
) -> Vec<NamedDomain> {
    let known: BTreeSet<_> = context
        .domains
        .iter()
        .map(|item| item.id.as_str())
        .collect();
    let suggestions = collect_suggestions(proposals.iter().flat_map(|proposal| &proposal.domains));
    context
        .domains
        .iter()
        .map(|item| NamedDomain {
            id: item.id.clone(),
            current_name: item.current_name.clone(),
            name: valid_name(suggestions.get(&item.id), &known, &item.id),
        })
        .collect()
}

fn merge_module_names(
    context: &super::context::NameContext,
    proposals: &[NameProposal],
) -> Vec<NamedModule> {
    let known: BTreeSet<_> = context
        .modules
        .iter()
        .map(|item| item.id.as_str())
        .collect();
    let suggestions = collect_suggestions(proposals.iter().flat_map(|proposal| &proposal.modules));
    context
        .modules
        .iter()
        .map(|item| NamedModule {
            id: item.id.clone(),
            current_name: item.current_name.clone(),
            domain_ids: item.domain_ids.clone(),
            name: valid_name(suggestions.get(&item.id), &known, &item.id),
        })
        .collect()
}

fn collect_suggestions<'a>(
    suggestions: impl Iterator<Item = &'a NameSuggestion>,
) -> std::collections::BTreeMap<String, String> {
    let mut result = std::collections::BTreeMap::new();
    for suggestion in suggestions {
        result
            .entry(suggestion.id.clone())
            .or_insert_with(|| suggestion.name.clone());
    }
    result
}

fn valid_name(suggestion: Option<&String>, known_ids: &BTreeSet<&str>, id: &str) -> Option<String> {
    if !known_ids.contains(id) {
        return None;
    }
    suggestion
        .filter(|name| {
            let trimmed = name.trim();
            !trimmed.is_empty() && trimmed.chars().count() <= 120
        })
        .cloned()
}
