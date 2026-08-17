//! 클러스터 병합 자격을 clustering mode별로 판정한다.

use crate::config::DomainClusteringMode;
use crate::domain::feature_graph::FeatureSimilarity;

const HTTP_MATCH_MIN: f64 = 0.20;
const CALL_MIN: f64 = 0.12;
const FLOW_MIN: f64 = 0.12;
const RESOURCE_MIN: f64 = 0.20;
const HTTP_STRONG: f64 = 0.60;
const RESOURCE_STRONG: f64 = 0.45;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StructuralSignal {
    Http,
    Call,
    Flow,
    Resource,
}

pub(super) fn merge_allowed(
    mode: DomainClusteringMode,
    sim: &FeatureSimilarity,
) -> Option<&'static str> {
    match mode {
        DomainClusteringMode::LegacyStrictKey => legacy_merge_allowed(sim),
        DomainClusteringMode::StructuralCrossKey => structural_merge_allowed(sim),
    }
}

fn legacy_merge_allowed(sim: &FeatureSimilarity) -> Option<&'static str> {
    if sim.http_match > 0.0 {
        return Some("http");
    }
    if sim.call > 0.0 {
        return Some("call");
    }
    if sim.flow > 0.0 {
        return Some("flow");
    }
    None
}

fn structural_merge_allowed(sim: &FeatureSimilarity) -> Option<&'static str> {
    let mut signals = Vec::new();
    if sim.http_match >= HTTP_MATCH_MIN {
        signals.push(StructuralSignal::Http);
    }
    if sim.call >= CALL_MIN {
        signals.push(StructuralSignal::Call);
    }
    if sim.flow >= FLOW_MIN {
        signals.push(StructuralSignal::Flow);
    }
    if sim.resource >= RESOURCE_MIN {
        signals.push(StructuralSignal::Resource);
    }

    if signals.is_empty() {
        return None;
    }

    if signals.len() == 1 && signals[0] == StructuralSignal::Call {
        return None;
    }

    if sim.http_match >= HTTP_STRONG && sim.resource >= RESOURCE_MIN {
        return Some("http+resource");
    }
    if sim.resource >= RESOURCE_STRONG {
        return Some("resource-strong");
    }
    if sim.http_match >= HTTP_STRONG {
        return Some("http-strong");
    }

    if signals.len() >= 2 {
        return Some(structural_pair_label(signals[0], signals[1]));
    }

    None
}

fn structural_pair_label(left: StructuralSignal, right: StructuralSignal) -> &'static str {
    match (left, right) {
        (StructuralSignal::Http, StructuralSignal::Call)
        | (StructuralSignal::Call, StructuralSignal::Http) => "http+call",
        (StructuralSignal::Http, StructuralSignal::Flow)
        | (StructuralSignal::Flow, StructuralSignal::Http) => "http+flow",
        (StructuralSignal::Http, StructuralSignal::Resource)
        | (StructuralSignal::Resource, StructuralSignal::Http) => "http+resource",
        (StructuralSignal::Call, StructuralSignal::Flow)
        | (StructuralSignal::Flow, StructuralSignal::Call) => "call+flow",
        (StructuralSignal::Call, StructuralSignal::Resource)
        | (StructuralSignal::Resource, StructuralSignal::Call) => "call+resource",
        (StructuralSignal::Flow, StructuralSignal::Resource)
        | (StructuralSignal::Resource, StructuralSignal::Flow) => "flow+resource",
        _ => "structural",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::feature_graph::FeatureSimilarity;

    fn sim(
        http_match: f64,
        call: f64,
        flow: f64,
        resource: f64,
        lexical: f64,
    ) -> FeatureSimilarity {
        FeatureSimilarity {
            http_match,
            call,
            flow,
            resource,
            lexical,
            combined: http_match + call + flow + resource + lexical,
        }
    }

    #[test]
    fn structural은_call_단독으로_병합하지_않는다() {
        assert!(structural_merge_allowed(&sim(0.0, 0.5, 0.0, 0.0, 0.0)).is_none());
    }

    #[test]
    fn structural은_lexical_단독으로_병합하지_않는다() {
        assert!(structural_merge_allowed(&sim(0.0, 0.0, 0.0, 0.0, 0.9)).is_none());
    }

    #[test]
    fn structural은_resource와_http가_겹치면_병합한다() {
        assert_eq!(
            structural_merge_allowed(&sim(0.5, 0.0, 0.0, 0.3, 0.0)),
            Some("http+resource")
        );
    }

    #[test]
    fn structural은_call과_flow가_겹치면_병합한다() {
        assert_eq!(
            structural_merge_allowed(&sim(0.0, 0.2, 0.2, 0.0, 0.0)),
            Some("call+flow")
        );
    }
}
