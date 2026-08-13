use crate::facts::evidence::Evidence;
use serde::{Deserialize, Serialize};

/// 코드로 들어오는 외부 진입점의 종류다.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum EntrypointKind {
    Http,
    WebSocket,
    Rpc,
    Event,
    Cli,
    Job,
    Main,
    Callback,
}

/// HTTP·이벤트·CLI·배치 등 외부에서 코드로 들어오는 진입점이다.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entrypoint {
    pub id: String,
    pub unit_id: String,
    pub kind: EntrypointKind,
    pub name: String,
    pub method: Option<String>,
    pub path: Option<String>,
    pub framework_id: Option<String>,
    pub evidence: Vec<Evidence>,
}
