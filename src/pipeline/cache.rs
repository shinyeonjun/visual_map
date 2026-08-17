//! 파일 단위 정적 Facts 증분 캐시다.
//!
//! 캐시는 같은 `AnalysisEngine` 인스턴스 안에서만 유지된다. 파일 내용 해시,
//! 언어, 프로젝트, 분석 설정이 모두 같을 때만 재사용하고, 캐시 적중 이후에도
//! 프로젝트 전체 reference resolution·graph·domain projection은 다시 실행한다.
//! 따라서 변경 파일 때문에 다른 파일의 연결이 바뀌는 일을 놓치지 않는다.

use crate::config::AnalysisConfig;
use crate::facts::FactBundle;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, VecDeque};
use std::sync::RwLock;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct FactCacheKey {
    project_id: String,
    file_id: String,
    language: String,
    content_hash: String,
    config_fingerprint: String,
}

impl FactCacheKey {
    pub(crate) fn new(
        project_id: &str,
        file_id: &str,
        language: &str,
        content_hash: Option<&str>,
        config_fingerprint: &str,
    ) -> Option<Self> {
        Some(Self {
            project_id: project_id.to_string(),
            file_id: file_id.to_string(),
            language: language.to_string(),
            content_hash: content_hash?.to_string(),
            config_fingerprint: config_fingerprint.to_string(),
        })
    }
}

#[derive(Debug, Default)]
struct FactCacheInner {
    entries: HashMap<FactCacheKey, FactBundle>,
    order: VecDeque<FactCacheKey>,
    capacity: usize,
}

/// 파일 Facts LRU 캐시다. 읽기는 `RwLock`으로 병렬 조회를 허용한다.
#[derive(Debug, Default)]
pub(crate) struct FactCache {
    inner: RwLock<FactCacheInner>,
}

impl FactCache {
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.inner
            .read()
            .expect("Facts 캐시 읽기 락이 poisoned되지 않아야 한다")
            .entries
            .len()
    }

    pub(crate) fn ensure_capacity(&self, capacity: usize) {
        let mut inner = self
            .inner
            .write()
            .expect("Facts 캐시 쓰기 락이 poisoned되지 않아야 한다");
        inner.capacity = capacity;
        evict_over_capacity(&mut inner);
    }

    pub(crate) fn insert(&self, key: FactCacheKey, bundle: FactBundle) {
        let mut inner = self
            .inner
            .write()
            .expect("Facts 캐시 쓰기 락이 poisoned되지 않아야 한다");
        if inner.capacity == 0 {
            return;
        }
        inner.entries.remove(&key);
        inner.order.retain(|current| current != &key);
        inner.entries.insert(key.clone(), bundle);
        inner.order.push_back(key);
        evict_over_capacity(&mut inner);
    }

    pub(crate) fn get(&self, key: &FactCacheKey) -> Option<FactBundle> {
        self.inner
            .read()
            .expect("Facts 캐시 읽기 락이 poisoned되지 않아야 한다")
            .entries
            .get(key)
            .cloned()
    }
}

fn evict_over_capacity(inner: &mut FactCacheInner) {
    while inner.entries.len() > inner.capacity {
        let Some(oldest) = inner.order.pop_front() else {
            inner.entries.clear();
            break;
        };
        inner.entries.remove(&oldest);
    }
}

pub(crate) fn config_fingerprint(config: &AnalysisConfig) -> String {
    let serialized = serde_json::to_vec(config).unwrap_or_default();
    let digest = Sha256::digest(serialized);
    let hex = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("config_{}", &hex[..24])
}

#[cfg(test)]
mod tests {
    use super::{config_fingerprint, FactCache, FactCacheKey};
    use crate::config::AnalysisConfig;
    use crate::facts::FactBundle;

    #[test]
    fn 동일한_파일_해시와_설정만_캐시를_재사용한다() {
        let fingerprint = config_fingerprint(&AnalysisConfig::default());
        let key = FactCacheKey::new("project", "file", "python", Some("hash"), &fingerprint)
            .expect("내용 해시가 있는 키여야 한다");
        let cache = FactCache::default();
        cache.ensure_capacity(1);
        cache.insert(key.clone(), FactBundle::default());
        assert!(cache.get(&key).is_some());
        assert!(FactCacheKey::new("project", "file", "python", None, &fingerprint).is_none());
        assert!(
            FactCacheKey::new("project", "file", "python", Some("changed"), &fingerprint)
                .and_then(|changed| cache.get(&changed))
                .is_none()
        );
    }
}
