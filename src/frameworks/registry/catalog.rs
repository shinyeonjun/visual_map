use super::capabilities::{FrameworkCapability, FrameworkKind};
use serde::{Deserialize, Serialize};

/// 외부 JSON 레지스트리에서 읽은 프레임워크 하나의 정의다.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameworkSpec {
    pub id: String,
    pub display_name: String,
    pub languages: Vec<String>,
    pub kind: FrameworkKind,
    pub capabilities: Vec<FrameworkCapability>,
    pub parent: Option<String>,
    pub markers: Vec<String>,
}

/// 컴파일 시 포함한 외부 레지스트리를 파싱한다.
///
/// 정의 데이터는 Rust 로직과 분리된 `frameworks.json`에서 관리한다.
pub fn supported_frameworks() -> Vec<FrameworkSpec> {
    serde_json::from_str(include_str!("frameworks.json"))
        .expect("내장 프레임워크 레지스트리가 올바른 JSON이어야 합니다")
}
