//! 정적 도메인 결과를 Codex 입력 단위로 축약·분할한다.

#[path = "context_compaction.rs"]
mod context_compaction;
#[path = "context_model.rs"]
mod context_model;
#[path = "context_sizing.rs"]
mod context_sizing;

use crate::config::SemanticPolicy;
use crate::domain::{DomainAnalysisOutput, DomainGroup};
use crate::frameworks::registry::detector::FrameworkDetection;
use std::time::Instant;

use context_compaction::{build_context, compact_domain};
use context_sizing::ContextSizeEstimator;

pub use context_model::{
    SemanticChunk, SemanticContext, SemanticContextArtifact, SemanticContextTimings,
};

impl SemanticContext {
    pub fn from_analysis(
        analysis: &DomainAnalysisOutput,
        frameworks: &[FrameworkDetection],
        policy: &SemanticPolicy,
    ) -> Self {
        build_context(
            analysis
                .groups
                .iter()
                .map(|domain| compact_domain(domain, policy))
                .collect(),
            analysis.relations.clone(),
            frameworks,
            policy,
        )
    }

    /// 전체 도메인 컨텍스트를 Codex CLI 입력 한도 안에 들어오도록 나눈다.
    pub fn chunks(
        analysis: &DomainAnalysisOutput,
        frameworks: &[FrameworkDetection],
        max_context_bytes: usize,
        policy: &SemanticPolicy,
    ) -> Vec<SemanticChunk> {
        Self::chunks_with_timings(analysis, frameworks, max_context_bytes, policy).0
    }

    /// 전체 도메인 컨텍스트를 나누면서 후보정 내부 단계별 시간을 함께 반환한다.
    pub fn chunks_with_timings(
        analysis: &DomainAnalysisOutput,
        frameworks: &[FrameworkDetection],
        max_context_bytes: usize,
        policy: &SemanticPolicy,
    ) -> (Vec<SemanticChunk>, SemanticContextTimings) {
        let max_context_bytes = max_context_bytes.max(policy.minimum_context_bytes);
        let compaction_started = Instant::now();
        let mut groups: Vec<_> = analysis
            .groups
            .iter()
            .map(|domain| compact_domain(domain, policy))
            .collect();
        groups.sort_by(|left, right| left.id.cmp(&right.id));
        let domain_compaction_ms = compaction_started.elapsed().as_millis() as u64;

        let sizing_started = Instant::now();
        let size_estimator =
            ContextSizeEstimator::new(&groups, &analysis.relations, frameworks, policy);
        let mut grouped_chunks: Vec<Vec<DomainGroup>> = Vec::new();
        let mut current = Vec::new();

        let mut current_size = size_estimator.state();
        for (group_index, group) in groups.iter().enumerate() {
            current_size.add_domain(group_index);

            if !current.is_empty() && current_size.total_size() > max_context_bytes {
                grouped_chunks.push(current);
                current = Vec::new();
                current_size.reset();
                current_size.add_domain(group_index);
            }
            current.push(group.clone());
        }
        let chunk_sizing_ms = sizing_started.elapsed().as_millis() as u64;

        if !current.is_empty() || grouped_chunks.is_empty() {
            grouped_chunks.push(current);
        }

        let count = grouped_chunks.len();
        let materialization_started = Instant::now();
        let chunks = grouped_chunks
            .into_iter()
            .enumerate()
            .map(|(index, domains)| SemanticChunk {
                index,
                count,
                context: build_context(domains, analysis.relations.clone(), frameworks, policy),
            })
            .collect();
        let chunk_materialization_ms = materialization_started.elapsed().as_millis() as u64;

        (
            chunks,
            SemanticContextTimings {
                domain_compaction_ms,
                chunk_sizing_ms,
                chunk_materialization_ms,
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::SemanticContext;
    use crate::config::SemanticPolicy;
    use crate::domain::confidence::{DomainConfidence, DomainStatus};
    use crate::domain::{DomainAnalysisOutput, DomainGroup, DomainKind};
    use std::collections::BTreeSet;

    fn domain(index: usize) -> DomainGroup {
        DomainGroup {
            id: format!("domain_{index:03}"),
            key: format!("domain_{index:03}"),
            label: format!("도메인 {index}"),
            kind: DomainKind::Business,
            status: DomainStatus::Candidate,
            confidence: DomainConfidence {
                level: "medium".into(),
                score: 5,
                signal_families: BTreeSet::new(),
            },
            primary_unit_ids: (0..200)
                .map(|unit| format!("unit_{index}_{unit}"))
                .collect(),
            shared_unit_ids: Vec::new(),
            entrypoint_ids: Vec::new(),
            feature_ids: Vec::new(),
            resource_ids: Vec::new(),
            evidence: Vec::new(),
            summary: Some("테스트용 도메인".into()),
        }
    }

    #[test]
    fn context_chunks_preserve_domain_order() {
        let analysis = DomainAnalysisOutput {
            groups: (0..12).map(domain).collect(),
            ..DomainAnalysisOutput::default()
        };
        let policy = SemanticPolicy::default();
        let chunks = SemanticContext::chunks(&analysis, &[], 16_000, &policy);

        assert!(chunks.len() > 1);
        for (index, chunk) in chunks.iter().enumerate() {
            assert_eq!(chunk.index, index);
            assert_eq!(chunk.count, chunks.len());
            assert!(chunk
                .context
                .domains
                .windows(2)
                .all(|domains| domains[0].id < domains[1].id));
        }
        let flattened: Vec<_> = chunks
            .iter()
            .flat_map(|chunk| chunk.context.domains.iter().map(|domain| domain.id.clone()))
            .collect();
        let expected: Vec<_> = analysis
            .groups
            .into_iter()
            .map(|domain| domain.id)
            .collect();
        assert_eq!(flattened, expected);
    }
}
