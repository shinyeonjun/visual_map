use serde::{Deserialize, Serialize};

/// 프레임워크가 제공하는 분석 능력의 종류다.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum FrameworkCapability {
    HttpEntrypoint,
    RpcEntrypoint,
    EventHandler,
    Component,
    DependencyInjection,
    ResourceAccess,
    AsyncBoundary,
    NativeBridge,
}

/// 프레임워크의 기술적 역할이다.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum FrameworkKind {
    Web,
    Ui,
    Runtime,
    Rpc,
    Desktop,
    Library,
}
