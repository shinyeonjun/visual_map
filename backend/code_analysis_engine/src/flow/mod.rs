//! 함수 내부 CFG와 함수 간 호출 흐름을 담당한다.

mod builder;
mod index;
mod local;
mod local_index;
mod model;

pub use model::{
    ExecutionFlow, ExecutionFlowGraph, FlowEdge, FlowEdgeKind, FlowLink, FlowNode, FlowNodeKind,
};

pub(crate) use builder::build;
