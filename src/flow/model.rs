//! 프론트엔드가 실행 흐름을 그릴 때 사용하는 공통 모델이다.

use crate::facts::{ResolutionStatus, SourceSpan};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum FlowNodeKind {
    Entry,
    Exit,
    Call,
    Condition,
    Switch,
    Loop,
    Return,
    Throw,
    Break,
    Continue,
    Catch,
    DynamicBoundary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowNode {
    pub id: String,
    pub owner_unit_id: String,
    pub kind: FlowNodeKind,
    pub span: Option<SourceSpan>,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_unit_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowEdge {
    pub id: String,
    pub source_node_id: String,
    pub target_node_id: String,
    pub kind: FlowEdgeKind,
    pub status: ResolutionStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference_id: Option<String>,
}

/// 함수 흐름 사이의 호출·복귀 관계다.
///
/// 로컬 CFG 엣지는 하나의 flow 안에서만 연결한다. 호출 대상 함수의 entry와
/// exit를 호출자 flow의 노드로 직접 연결하면 서로 다른 그래프의 노드가
/// 섞이고, 노드 한도 적용 때 호출 관계가 조용히 삭제된다. 이 링크는 두
/// flow의 경계를 명시적으로 보존한다.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowLink {
    pub id: String,
    pub reference_id: String,
    pub source_flow_id: String,
    pub source_node_id: String,
    pub target_flow_id: String,
    pub target_entry_node_id: String,
    pub target_exit_node_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub return_node_id: Option<String>,
    pub status: ResolutionStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum FlowEdgeKind {
    Sequential,
    TrueBranch,
    FalseBranch,
    LoopBody,
    LoopBack,
    Call,
    Return,
    Exception,
    Dynamic,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionFlow {
    pub id: String,
    pub owner_unit_id: String,
    pub entry_node_id: String,
    pub exit_node_id: String,
    pub nodes: Vec<FlowNode>,
    pub edges: Vec<FlowEdge>,
    pub dynamic_boundary_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionFlowGraph {
    pub flows: Vec<ExecutionFlow>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<FlowLink>,
}
