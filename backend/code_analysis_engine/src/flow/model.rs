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
}
