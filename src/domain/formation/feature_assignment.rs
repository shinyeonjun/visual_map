//! 기능을 도메인 클러스터 멤버십으로 배정한다.

use crate::domain::grouping::DomainGroup;
use crate::views::overview::model::FeatureGroup;
use std::collections::{BTreeSet, HashMap};

/// 기능을 도메인 그룹에 실제로 들어간 진입점·자원 ID로 배정한다.
pub(super) fn assign_features_to_domains(
    features: &mut [FeatureGroup],
    groups: &[DomainGroup],
) {
    let mut entrypoint_to_domains: HashMap<&str, BTreeSet<&str>> = HashMap::new();
    for group in groups {
        for entrypoint_id in &group.entrypoint_ids {
            entrypoint_to_domains
                .entry(entrypoint_id.as_str())
                .or_default()
                .insert(group.id.as_str());
        }
    }
    let mut resource_to_domains: HashMap<&str, BTreeSet<&str>> = HashMap::new();
    for group in groups {
        for resource_id in &group.resource_ids {
            resource_to_domains
                .entry(resource_id.as_str())
                .or_default()
                .insert(group.id.as_str());
        }
    }

    for feature in features {
        let mut domain_ids = BTreeSet::new();
        for entrypoint_id in &feature.entrypoint_ids {
            if let Some(domains) = entrypoint_to_domains.get(entrypoint_id.as_str()) {
                domain_ids.extend(domains.iter().map(|domain_id| (*domain_id).to_string()));
            }
        }
        for resource_id in &feature.resource_ids {
            if let Some(domains) = resource_to_domains.get(resource_id.as_str()) {
                domain_ids.extend(domains.iter().map(|domain_id| (*domain_id).to_string()));
            }
        }
        feature.domain_ids = domain_ids.into_iter().collect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::confidence::{DomainConfidence, DomainStatus};
    use crate::domain::grouping::DomainKind;
    use crate::views::overview::model::{FeatureConfidence, FeatureKind, FeatureStatus, FeatureVisibility};
    use std::collections::BTreeSet;

    fn domain(id: &str, entrypoint_ids: &[&str], resource_ids: &[&str]) -> DomainGroup {
        DomainGroup {
            id: id.into(),
            key: id.into(),
            label: id.into(),
            kind: DomainKind::Business,
            status: DomainStatus::Confirmed,
            confidence: DomainConfidence {
                level: "high".into(),
                score: 100,
                signal_families: BTreeSet::new(),
                cohesion: 1.0,
                separation: 1.0,
                evidence_diversity: 1.0,
                overall: 1.0,
            },
            primary_unit_ids: Vec::new(),
            shared_unit_ids: Vec::new(),
            entrypoint_ids: entrypoint_ids.iter().map(|value| (*value).into()).collect(),
            feature_ids: Vec::new(),
            resource_ids: resource_ids.iter().map(|value| (*value).into()).collect(),
            evidence: Vec::new(),
            summary: None,
        }
    }

    #[test]
    fn 진입점이_들어있는_도메인에_기능을_붙인다() {
        let groups = vec![
            domain("domain-auth", &["ep-login"], &[]),
            domain("domain-schedules", &["ep-schedule"], &["res-schedule"]),
        ];
        let mut features = vec![
            FeatureGroup {
                id: "feature-login".into(),
                key: "login".into(),
                label: "login".into(),
                kind: FeatureKind::Endpoint,
                status: FeatureStatus::Confirmed,
                visibility: FeatureVisibility::UserFacing,
                confidence: FeatureConfidence::default(),
                domain_ids: Vec::new(),
                unit_ids: Vec::new(),
                reachable_unit_count: 0,
                entrypoint_ids: vec!["ep-login".into()],
                flow_ids: Vec::new(),
                resource_ids: Vec::new(),
                dynamic_boundary_ids: Vec::new(),
                evidence: Vec::new(),
                summary: None,
            },
            FeatureGroup {
                id: "feature-schedule".into(),
                key: "schedule".into(),
                label: "schedule".into(),
                kind: FeatureKind::Endpoint,
                status: FeatureStatus::Confirmed,
                visibility: FeatureVisibility::UserFacing,
                confidence: FeatureConfidence::default(),
                domain_ids: Vec::new(),
                unit_ids: Vec::new(),
                reachable_unit_count: 0,
                entrypoint_ids: vec!["ep-schedule".into()],
                flow_ids: Vec::new(),
                resource_ids: vec!["res-schedule".into()],
                dynamic_boundary_ids: Vec::new(),
                evidence: Vec::new(),
                summary: None,
            },
        ];

        assign_features_to_domains(&mut features, &groups);

        assert_eq!(features[0].domain_ids, vec!["domain-auth"]);
        assert_eq!(features[1].domain_ids, vec!["domain-schedules"]);
    }

    #[test]
    fn 공유_진입점은_여러_도메인에_붙는다() {
        let groups = vec![
            domain("domain-auth", &["ep-shared"], &[]),
            domain("domain-audit", &["ep-shared"], &[]),
        ];
        let mut features = vec![FeatureGroup {
            id: "feature-shared".into(),
            key: "shared".into(),
            label: "shared".into(),
            kind: FeatureKind::Endpoint,
            status: FeatureStatus::Confirmed,
            visibility: FeatureVisibility::UserFacing,
            confidence: FeatureConfidence::default(),
            domain_ids: Vec::new(),
            unit_ids: Vec::new(),
            reachable_unit_count: 0,
            entrypoint_ids: vec!["ep-shared".into()],
            flow_ids: Vec::new(),
            resource_ids: Vec::new(),
            dynamic_boundary_ids: Vec::new(),
            evidence: Vec::new(),
            summary: None,
        }];

        assign_features_to_domains(&mut features, &groups);

        assert_eq!(
            features[0].domain_ids,
            vec!["domain-audit", "domain-auth"]
        );
    }
}
