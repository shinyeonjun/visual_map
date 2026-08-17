use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum EvalMode {
    #[default]
    Service,
    Library,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct EvalGold {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub set: String,
    #[serde(default)]
    pub mode: EvalMode,
    #[serde(default)]
    pub project_path: Option<String>,
    #[serde(default)]
    pub must_have_domains: Vec<String>,
    #[serde(default)]
    pub domain_aliases: Vec<DomainAlias>,
    #[serde(default)]
    pub must_not_merge: Vec<Vec<String>>,
    #[serde(default)]
    pub must_not_split: Vec<String>,
    #[serde(default)]
    pub cross_cutting: Vec<String>,
    #[serde(default)]
    pub forbidden_domain_keys: Vec<String>,
    #[serde(default)]
    pub must_have_features: Vec<FeatureGold>,
    #[serde(default)]
    pub flow_invariants: Vec<FlowInvariantGold>,
    /// 인라인 카탈로그 JSON에서만 사용한다. `catalog.json`의 `files`와는 별개다.
    #[serde(default)]
    pub entries: Vec<EvalGold>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct EvalCatalog {
    #[serde(default)]
    pub files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DomainAlias {
    pub key: String,
    #[serde(default)]
    pub aliases: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct FeatureGold {
    pub contract: String,
    #[serde(default)]
    pub domain: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct FlowInvariantGold {
    pub feature_contract: String,
    #[serde(default)]
    pub must_have_edge_kinds: Vec<String>,
}
