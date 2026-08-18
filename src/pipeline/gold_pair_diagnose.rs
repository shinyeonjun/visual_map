//! gold positive/negative pair 신호 비교 실행.

use crate::config::DomainClusteringMode;
use crate::domain::{
    analyze_gold_pair_signals, build_capability_pair_context, GoldPairLabelInput,
    GoldPairSignalModeReport,
};
use crate::eval::{
    extract_actual_positive_pairs, extract_gold_pair_labels, EvalGold, GoldPairKind, GoldPairLabel,
};
use crate::model::AnalysisRequest;
use crate::EngineError;

use super::DomainAnalysisPipeline;

impl DomainAnalysisPipeline {
    pub fn diagnose_gold_pair_signals(
        &self,
        request: AnalysisRequest,
        gold: &EvalGold,
        modes: &[DomainClusteringMode],
        include_evidence: bool,
    ) -> Result<Vec<GoldPairSignalModeReport>, EngineError> {
        let (positives, negatives) = extract_gold_pair_labels(gold);
        let bundle = self.build_fact_bundle(&request)?;
        let domain_policy = request.options.config.domains.clone();
        let path_policy = request.options.config.paths.clone();
        let context = build_capability_pair_context(
            &bundle.facts,
            &bundle.execution_flows,
            &domain_policy,
            &path_policy,
        );
        let actual_keys: Vec<String> = context
            .capabilities
            .iter()
            .map(|capability| capability.key.clone())
            .collect();
        let actual_positives = extract_actual_positive_pairs(gold, &actual_keys);
        let labels = merge_labels(&positives, &negatives, &actual_positives);

        Ok(modes
            .iter()
            .map(|mode| {
                analyze_gold_pair_signals(
                    &context,
                    &bundle.facts,
                    &path_policy,
                    *mode,
                    &labels,
                    include_evidence,
                )
            })
            .collect())
    }
}

fn merge_labels(
    positives: &[GoldPairLabel],
    negatives: &[GoldPairLabel],
    actual_positives: &[GoldPairLabel],
) -> Vec<GoldPairLabelInput> {
    let mut labels: Vec<GoldPairLabelInput> = positives
        .iter()
        .chain(actual_positives.iter())
        .map(|label| GoldPairLabelInput {
            left_key: label.left_key.clone(),
            right_key: label.right_key.clone(),
            positive: true,
            source: label.source.clone(),
        })
        .chain(negatives.iter().map(|label| GoldPairLabelInput {
            left_key: label.left_key.clone(),
            right_key: label.right_key.clone(),
            positive: label.kind == GoldPairKind::Positive,
            source: label.source.clone(),
        }))
        .collect();
    dedupe_labels(&mut labels);
    labels
}

fn dedupe_labels(labels: &mut Vec<GoldPairLabelInput>) {
    let mut seen = std::collections::BTreeSet::new();
    labels.retain(|label| {
        let left = label.left_key.as_str();
        let right = label.right_key.as_str();
        let (left, right) = if left <= right {
            (left, right)
        } else {
            (right, left)
        };
        let key = (
            left.to_string(),
            right.to_string(),
            label.positive,
            label.source.clone(),
        );
        seen.insert(key)
    });
}
