//! 언어 분석기와 프레임워크 adapter 사이의 원시 의미 사실이다.
//!
//! 이 모델은 특정 프레임워크를 모른다. Python 분석기는 구문 구조만 기록하고,
//! FastAPI·Flask·ORM adapter가 이 사실을 공통 entrypoint/resource로 보강한다.

use crate::facts::Evidence;
use serde::{Deserialize, Serialize};

/// 로컬 이름이 어떤 외부 심볼에 연결되는지 나타내는 binding 종류다.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum BindingKind {
    Import,
    ImportAlias,
    Assignment,
    Parameter,
}

/// Python import·alias·대입으로 만들어진 이름 연결이다.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SymbolBinding {
    pub id: String,
    pub source_unit_id: String,
    pub local_name: String,
    pub target_name: String,
    pub kind: BindingKind,
    pub evidence: Vec<Evidence>,
}

/// decorator의 프레임워크 비의존 원시 표현이다.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DecoratorFact {
    pub id: String,
    pub unit_id: String,
    pub receiver: Option<String>,
    pub name: String,
    pub arguments: Vec<String>,
    pub expression: String,
    pub evidence: Vec<Evidence>,
}

/// 함수 호출의 프레임워크 비의존 원시 표현이다.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CallSiteFact {
    pub id: String,
    pub source_unit_id: String,
    pub callee: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub receiver: Option<String>,
    pub arguments: Vec<String>,
    pub assigned_name: Option<String>,
    pub evidence: Vec<Evidence>,
}
