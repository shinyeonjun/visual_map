use crate::facts::Evidence;
use serde::{Deserialize, Serialize};

/// 프레임워크 감지기가 공통 Facts에 남기는 기술 사실이다.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameworkFact {
    pub id: String,
    pub display_name: String,
    pub kind: String,
    pub capabilities: Vec<String>,
    pub parent: Option<String>,
    pub confidence: f32,
    pub evidence: Vec<Evidence>,
}
