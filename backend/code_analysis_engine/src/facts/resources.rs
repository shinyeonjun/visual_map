use crate::facts::evidence::Evidence;
use serde::{Deserialize, Serialize};

/// 코드가 접근하는 외부 자원의 종류다.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ResourceKind {
    Table,
    Collection,
    Cache,
    ExternalApi,
    Network,
    WebSocket,
    Process,
    Environment,
    EventTopic,
    File,
    Unknown,
}

/// 자원 접근 방식이다.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum AccessMode {
    Read,
    Write,
    ReadWrite,
    Unknown,
}

/// 테이블·컬렉션·Redis·외부 API·이벤트 토픽 등 자원 접근 사실이다.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceAccess {
    pub id: String,
    pub unit_id: String,
    pub kind: ResourceKind,
    pub name: String,
    pub mode: AccessMode,
    pub evidence: Vec<Evidence>,
}
