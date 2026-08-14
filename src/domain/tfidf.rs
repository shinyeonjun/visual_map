//! Feature 단위 TF-IDF 용어 추출과 코사인 유사도 계산.

use crate::config::DomainPolicy;
use crate::facts::FactStore;
use std::collections::{HashMap, HashSet};

use super::signals::tokenize_with_generic;

/// 하나의 Feature에서 추출한 용어 빈도 벡터다.
pub(super) struct FeatureTerms {
    pub term_frequencies: HashMap<String, f64>,
}

/// 전체 Feature 집합의 IDF를 포함한 용어 인덱스다.
#[allow(dead_code)]
pub(super) struct TermIndex {
    idf: HashMap<String, f64>,
}

/// Feature별 TF-IDF 가중 용어 벡터를 추출한다.
pub(super) fn extract_terms(
    feature_unit_ids: &[Vec<String>],
    store: &FactStore,
    policy: &DomainPolicy,
) -> (Vec<FeatureTerms>, TermIndex) {
    let feature_count = feature_unit_ids.len() as f64;
    let mut document_frequency: HashMap<String, usize> = HashMap::new();

    let raw_terms: Vec<HashMap<String, usize>> = feature_unit_ids
        .iter()
        .map(|unit_ids| {
            let mut term_counts: HashMap<String, usize> = HashMap::new();
            let mut seen_terms = HashSet::new();
            for unit_id in unit_ids {
                let Some(unit) = store.unit(unit_id) else {
                    continue;
                };
                for token in tokenize_with_generic(&unit.name, policy) {
                    *term_counts.entry(token.clone()).or_insert(0) += 1;
                    seen_terms.insert(token);
                }
                for token in tokenize_with_generic(&unit.qualified_name, policy) {
                    *term_counts.entry(token.clone()).or_insert(0) += 1;
                    seen_terms.insert(token);
                }
                let path_directory = unit
                    .relative_path
                    .replace('\\', "/")
                    .rsplit_once('/')
                    .map(|(dir, _)| dir.to_string())
                    .unwrap_or_default();
                for token in tokenize_with_generic(&path_directory, policy) {
                    *term_counts.entry(token.clone()).or_insert(0) += 1;
                    seen_terms.insert(token);
                }
            }
            for term in &seen_terms {
                *document_frequency.entry(term.clone()).or_insert(0) += 1;
            }
            term_counts
        })
        .collect();

    let idf: HashMap<String, f64> = document_frequency
        .iter()
        .map(|(term, df)| {
            let idf_value = ((feature_count + 1.0) / (*df as f64 + 1.0)).ln();
            (term.clone(), idf_value)
        })
        .collect();

    let feature_terms: Vec<FeatureTerms> = raw_terms
        .into_iter()
        .map(|counts| {
            let total: usize = counts.values().sum();
            let max_count = counts.values().copied().max().unwrap_or(1).max(1) as f64;
            let term_frequencies: HashMap<String, f64> = counts
                .into_iter()
                .filter_map(|(term, count)| {
                    let tf = count as f64 / max_count;
                    let idf_value = idf.get(&term)?;
                    if total == 0 {
                        return None;
                    }
                    Some((term, tf * idf_value))
                })
                .collect();
            FeatureTerms { term_frequencies }
        })
        .collect();

    (feature_terms, TermIndex { idf })
}

/// 두 Feature의 TF-IDF 벡터 코사인 유사도를 계산한다.
pub(super) fn cosine_similarity(a: &FeatureTerms, b: &FeatureTerms) -> f64 {
    if a.term_frequencies.is_empty() || b.term_frequencies.is_empty() {
        return 0.0;
    }
    let (smaller, larger) = if a.term_frequencies.len() <= b.term_frequencies.len() {
        (&a.term_frequencies, &b.term_frequencies)
    } else {
        (&b.term_frequencies, &a.term_frequencies)
    };

    let mut dot_product = 0.0;
    for (term, weight_a) in smaller {
        if let Some(weight_b) = larger.get(term) {
            dot_product += weight_a * weight_b;
        }
    }

    let magnitude_a: f64 = a.term_frequencies.values().map(|v| v * v).sum::<f64>().sqrt();
    let magnitude_b: f64 = b.term_frequencies.values().map(|v| v * v).sum::<f64>().sqrt();
    if magnitude_a == 0.0 || magnitude_b == 0.0 {
        return 0.0;
    }
    (dot_product / (magnitude_a * magnitude_b)).clamp(0.0, 1.0)
}

/// 용어 인덱스에서 가장 특이한(IDF가 높은) 용어를 반환한다.
#[allow(dead_code)]
pub(super) fn most_specific_term(terms: &FeatureTerms, index: &TermIndex) -> Option<String> {
    terms
        .term_frequencies
        .iter()
        .max_by(|a, b| {
            let idf_a = index.idf.get(a.0).unwrap_or(&0.0);
            let idf_b = index.idf.get(b.0).unwrap_or(&0.0);
            idf_a.partial_cmp(idf_b).unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(term, _)| term.clone())
}
