use serde::{Deserialize, Serialize};

/// 코드 단위가 도메인과 맺는 소속 관계다.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum MembershipKind {
    Primary,
    Shared,
    Referenced,
    Unknown,
}

/// 코드 단위 하나의 도메인 소속 결과다.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DomainMembership {
    pub unit_id: String,
    pub domain_id: Option<String>,
    /// 점수가 충분히 가까워 공통 코드로 분류된 모든 도메인 ID다.
    #[serde(default)]
    pub domain_ids: Vec<String>,
    pub kind: MembershipKind,
    pub score: u32,
}
