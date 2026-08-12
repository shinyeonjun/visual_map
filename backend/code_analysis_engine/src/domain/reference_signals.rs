//! 참조 이름을 기존 도메인 후보와 연결하는 신호 생성 책임.

use crate::config::DomainPolicy;
use crate::facts::{FactStore, ResolutionStatus};
use std::collections::{HashMap, HashSet};

use super::signals::{tokenize, DomainSignal, DomainSignalKind};

/// 참조 토큰을 도메인 후보로 역색인해 참조 신호를 추가한다.
pub(super) fn append(
    store: &FactStore,
    domain_policy: &DomainPolicy,
    known_keys: &[String],
    signals: &mut Vec<DomainSignal>,
) {
    let mut domain_keys_by_token: HashMap<String, Vec<String>> = HashMap::new();
    for key in known_keys {
        domain_keys_by_token
            .entry(key.clone())
            .or_default()
            .push(key.clone());
    }

    for reference in &store.references {
        // 외부·동적·미해결 이름은 프로젝트 도메인을 증명하지 않는다.
        // 이름만으로 `get`, `helper`, 라이브러리 API가 도메인 점수를
        // 부풀리지 않도록 내부 확정 참조만 보강 신호로 사용한다.
        if reference.status != ResolutionStatus::Confirmed || reference.target_unit_id.is_none() {
            continue;
        }
        let mut seen_tokens = HashSet::new();
        for token in tokenize(&reference.target_name, domain_policy) {
            if !seen_tokens.insert(token.clone()) {
                continue;
            }
            if let Some(keys) = domain_keys_by_token.get(&token) {
                for key in keys {
                    signals.push(DomainSignal {
                        domain_key: key.clone(),
                        unit_id: reference.source_unit_id.clone(),
                        kind: DomainSignalKind::Reference,
                        weight: domain_policy.reference_weight,
                        evidence: reference.evidence.clone(),
                    });
                }
            }
        }
    }
}
