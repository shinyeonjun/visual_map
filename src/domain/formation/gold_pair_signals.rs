//! gold 라벨 pair에 대한 similarity/rejection/evidence 분석.

use crate::config::{DomainClusteringMode, PathPolicy};
use super::gold_pair_resolution::resolve_capability_key;
use crate::facts::FactStore;
use serde::{Deserialize, Serialize};

use super::capability_evidence::{build_capability_evidence, CapabilityEvidence};
use super::diagnostics::clustering_mode_label;
use super::pair_diagnostics::{classify_pair_with_context, PairSimilarityBreakdown};
use super::pair_context::CapabilityPairContext;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SignalAverages {
    pub combined: f64,
    pub http_match: f64,
    pub call: f64,
    pub flow: f64,
    pub resource: f64,
    pub path: f64,
    pub lexical: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoldLabeledPairSignals {
    pub left_key: String,
    pub right_key: String,
    pub kind: String,
    pub source: String,
    pub found: bool,
    pub left_resolution: CapabilityKeyMatch,
    pub right_resolution: CapabilityKeyMatch,
    pub similarity: Option<PairSimilarityBreakdown>,
    pub rejection: Option<String>,
    pub merge_gate: Option<String>,
    pub left_raw_resource_ids: Option<Vec<String>>,
    pub right_raw_resource_ids: Option<Vec<String>>,
    pub left_evidence: Option<CapabilityEvidence>,
    pub right_evidence: Option<CapabilityEvidence>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityKeyMatch {
    pub label_key: String,
    pub resolved_key: Option<String>,
    pub matched: bool,
    pub unmatched_reason: Option<String>,
    pub candidate_actual_keys: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoldPairSignalModeReport {
    pub clustering_mode: String,
    pub positive_count: usize,
    pub negative_count: usize,
    pub matched_positive: usize,
    pub matched_negative: usize,
    pub unmatched_positive: usize,
    pub unmatched_negative: usize,
    pub positive_avg: SignalAverages,
    pub negative_avg: SignalAverages,
    pub pairs: Vec<GoldLabeledPairSignals>,
}

#[derive(Debug, Clone)]
pub struct GoldPairLabelInput {
    pub left_key: String,
    pub right_key: String,
    pub positive: bool,
    pub source: String,
}

pub(crate) fn analyze_gold_pair_signals(
    context: &CapabilityPairContext,
    store: &FactStore,
    path_policy: &PathPolicy,
    mode: DomainClusteringMode,
    labels: &[GoldPairLabelInput],
    include_evidence: bool,
) -> GoldPairSignalModeReport {
    let actual_keys: Vec<String> = context
        .capabilities
        .iter()
        .map(|capability| capability.key.clone())
        .collect();
    let positive_count = labels.iter().filter(|label| label.positive).count();
    let negative_count = labels.len().saturating_sub(positive_count);
    let mut report = GoldPairSignalModeReport {
        clustering_mode: clustering_mode_label(mode).into(),
        positive_count,
        negative_count,
        matched_positive: 0,
        matched_negative: 0,
        unmatched_positive: 0,
        unmatched_negative: 0,
        positive_avg: SignalAverages::default(),
        negative_avg: SignalAverages::default(),
        pairs: Vec::with_capacity(labels.len()),
    };

    let mut positive_sums = SignalAverages::default();
    let mut negative_sums = SignalAverages::default();

    for label in labels {
        let pair = analyze_labeled_pair(
            context,
            store,
            path_policy,
            mode,
            label,
            include_evidence,
            &actual_keys,
        );
        if pair.found {
            if let Some(sim) = &pair.similarity {
                if label.positive {
                    report.matched_positive += 1;
                    accumulate_signal(&mut positive_sums, sim);
                } else {
                    report.matched_negative += 1;
                    accumulate_signal(&mut negative_sums, sim);
                }
            }
        } else if label.positive {
            report.unmatched_positive += 1;
        } else {
            report.unmatched_negative += 1;
        }
        report.pairs.push(pair);
    }

    if report.matched_positive > 0 {
        report.positive_avg = average_signal(&positive_sums, report.matched_positive);
    }
    if report.matched_negative > 0 {
        report.negative_avg = average_signal(&negative_sums, report.matched_negative);
    }
    report
}

fn analyze_labeled_pair(
    context: &CapabilityPairContext,
    store: &FactStore,
    path_policy: &PathPolicy,
    mode: DomainClusteringMode,
    label: &GoldPairLabelInput,
    include_evidence: bool,
    actual_keys: &[String],
) -> GoldLabeledPairSignals {
    let left_resolution = resolution_from_label(&label.left_key, actual_keys);
    let right_resolution = resolution_from_label(&label.right_key, actual_keys);
    let Some(left_index) = left_resolution
        .resolved_key
        .as_deref()
        .and_then(|key| context.key_index.get(key))
    else {
        return missing_pair(label, left_resolution, right_resolution);
    };
    let Some(right_index) = right_resolution
        .resolved_key
        .as_deref()
        .and_then(|key| context.key_index.get(key))
    else {
        return missing_pair(label, left_resolution, right_resolution);
    };

    let left = &context.capabilities[*left_index];
    let right = &context.capabilities[*right_index];
    let sim = context.matrix.get(*left_index, *right_index);
    let (reason, merge_gate) = classify_pair_with_context(
        left,
        right,
        sim,
        store,
        path_policy,
        mode,
        context.merge_threshold,
    );

    let resource_zero = sim.resource == 0.0;
    let (left_evidence, right_evidence) = if include_evidence {
        (
            Some(build_capability_evidence(
                left,
                &context.capability_data,
                *left_index,
                &context.terms[*left_index],
                store,
            )),
            Some(build_capability_evidence(
                right,
                &context.capability_data,
                *right_index,
                &context.terms[*right_index],
                store,
            )),
        )
    } else {
        (None, None)
    };

    GoldLabeledPairSignals {
        left_key: label.left_key.clone(),
        right_key: label.right_key.clone(),
        kind: if label.positive {
            "positive".into()
        } else {
            "negative".into()
        },
        source: label.source.clone(),
        found: true,
        left_resolution,
        right_resolution,
        similarity: Some(PairSimilarityBreakdown::from(sim)),
        rejection: Some(reason.label().to_string()),
        merge_gate: merge_gate.map(str::to_string),
        left_raw_resource_ids: resource_zero.then(|| left.resource_ids.clone()),
        right_raw_resource_ids: resource_zero.then(|| right.resource_ids.clone()),
        left_evidence,
        right_evidence,
    }
}

fn resolution_from_label(label_key: &str, actual_keys: &[String]) -> CapabilityKeyMatch {
    let resolution = resolve_capability_key(label_key, actual_keys);
    CapabilityKeyMatch {
        label_key: label_key.to_string(),
        resolved_key: resolution.resolved_key,
        matched: resolution.matched,
        unmatched_reason: resolution.unmatched_reason,
        candidate_actual_keys: resolution.candidate_actual_keys,
    }
}

fn missing_pair(
    label: &GoldPairLabelInput,
    left_resolution: CapabilityKeyMatch,
    right_resolution: CapabilityKeyMatch,
) -> GoldLabeledPairSignals {
    GoldLabeledPairSignals {
        left_key: label.left_key.clone(),
        right_key: label.right_key.clone(),
        kind: if label.positive {
            "positive".into()
        } else {
            "negative".into()
        },
        source: label.source.clone(),
        found: false,
        left_resolution,
        right_resolution,
        similarity: None,
        rejection: None,
        merge_gate: None,
        left_raw_resource_ids: None,
        right_raw_resource_ids: None,
        left_evidence: None,
        right_evidence: None,
    }
}

fn accumulate_signal(target: &mut SignalAverages, sim: &PairSimilarityBreakdown) {
    target.combined += sim.combined;
    target.http_match += sim.http_match;
    target.call += sim.call;
    target.flow += sim.flow;
    target.resource += sim.resource;
    target.path += sim.path;
    target.lexical += sim.lexical;
}

fn average_signal(sums: &SignalAverages, count: usize) -> SignalAverages {
    let count = count as f64;
    SignalAverages {
        combined: sums.combined / count,
        http_match: sums.http_match / count,
        call: sums.call / count,
        flow: sums.flow / count,
        resource: sums.resource / count,
        path: sums.path / count,
        lexical: sums.lexical / count,
    }
}
