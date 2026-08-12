//! 언어별 AST에서 공통 실행 흐름 사실로 정규화한 모델이다.

use crate::facts::SourceSpan;
use serde::{Deserialize, Serialize};

/// 함수 내부에서 실행 흐름에 영향을 주는 구문 종류다.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ControlFlowKind {
    Condition,
    Switch,
    Loop,
    Return,
    Throw,
    Break,
    Continue,
    Try,
    Catch,
}

/// Tree-sitter 노드를 언어 공통 실행 흐름 사실로 변환한 결과다.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ControlFlowFact {
    pub id: String,
    pub owner_unit_id: String,
    pub kind: ControlFlowKind,
    pub span: SourceSpan,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body_span: Option<SourceSpan>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alternative_span: Option<SourceSpan>,
}
