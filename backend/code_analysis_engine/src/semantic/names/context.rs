//! Codex 이름 분석에 보내는 최소 컨텍스트와 청크 모델.

use crate::views::overview::model::OverviewResponse;
use serde::Serialize;

use super::candidates;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NameDomainContext {
    pub id: String,
    pub candidate_key: String,
    pub current_name: String,
    pub symbols: Vec<String>,
    pub paths: Vec<String>,
    pub entrypoints: Vec<String>,
    pub resources: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NameModuleContext {
    pub id: String,
    pub current_name: String,
    pub domain_ids: Vec<String>,
    pub symbols: Vec<String>,
    pub paths: Vec<String>,
    pub call_targets: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NameContext {
    pub domains: Vec<NameDomainContext>,
    pub modules: Vec<NameModuleContext>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NameChunk {
    pub index: usize,
    pub count: usize,
    pub context: NameContext,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NameContextArtifact {
    pub schema_version: &'static str,
    pub chunk_count: usize,
    pub chunks: Vec<NameChunk>,
}

impl NameContextArtifact {
    #[cfg(test)]
    pub(super) fn single(context: NameContext) -> Self {
        Self {
            schema_version: "codex-name-context.v1",
            chunk_count: 1,
            chunks: vec![NameChunk {
                index: 0,
                count: 1,
                context,
            }],
        }
    }
}

pub(super) fn build_context(overview: &OverviewResponse) -> NameContext {
    NameContext {
        domains: candidates::domains(overview),
        modules: candidates::modules(overview),
    }
}

pub(super) fn chunk_context(context: &NameContext, max_bytes: usize) -> Vec<NameChunk> {
    let max_bytes = max_bytes.max(16_000);
    let mut groups: Vec<NameContext> = Vec::new();
    let mut current = NameContext {
        domains: Vec::new(),
        modules: Vec::new(),
    };

    for domain in &context.domains {
        let mut candidate = current.clone();
        candidate.domains.push(domain.clone());
        if !current.domains.is_empty() && serialized_size(&candidate) > max_bytes {
            groups.push(current);
            current = NameContext {
                domains: Vec::new(),
                modules: Vec::new(),
            };
        }
        current.domains.push(domain.clone());
    }
    for module in &context.modules {
        let mut candidate = current.clone();
        candidate.modules.push(module.clone());
        if (!current.domains.is_empty() || !current.modules.is_empty())
            && serialized_size(&candidate) > max_bytes
        {
            groups.push(current);
            current = NameContext {
                domains: Vec::new(),
                modules: Vec::new(),
            };
        }
        current.modules.push(module.clone());
    }
    if !current.domains.is_empty() || !current.modules.is_empty() || groups.is_empty() {
        groups.push(current);
    }

    let count = groups.len();
    groups
        .into_iter()
        .enumerate()
        .map(|(index, context)| NameChunk {
            index,
            count,
            context,
        })
        .collect()
}

fn serialized_size(context: &NameContext) -> usize {
    serde_json::to_vec(context)
        .map(|bytes| bytes.len())
        .unwrap_or(usize::MAX)
}
