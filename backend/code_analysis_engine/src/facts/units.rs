use crate::facts::evidence::SourceSpan;
use crate::model::Language;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// 도메인 분석에 사용할 코드 단위의 종류다.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum CodeUnitKind {
    File,
    Module,
    Namespace,
    Package,
    Class,
    Interface,
    Struct,
    Enum,
    Trait,
    Impl,
    Record,
    Mixin,
    Extension,
    TypeAlias,
    Function,
    Method,
    Constructor,
    Property,
    Lambda,
    Entity,
    Repository,
    Unknown,
}

/// 코드 유닛의 접근 제한자다.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum CodeUnitVisibility {
    Public,
    Private,
    Protected,
    Internal,
    Package,
    Crate,
    #[default]
    Unknown,
}

/// 함수·메서드의 매개변수 상세다.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct CodeParameter {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub type_annotation: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_value: Option<String>,
    pub variadic: bool,
}

/// 파일·모듈·클래스·함수 등 공통 코드 단위다.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeUnit {
    pub id: String,
    pub kind: CodeUnitKind,
    pub name: String,
    pub qualified_name: String,
    pub file_id: String,
    pub relative_path: String,
    pub language: Language,
    pub parent_id: Option<String>,
    pub span: SourceSpan,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body_span: Option<SourceSpan>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parameters: Vec<CodeParameter>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub return_type: Option<String>,
    #[serde(default)]
    pub visibility: CodeUnitVisibility,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub modifiers: Vec<String>,
    pub exported: bool,
}

/// 코드 유닛의 부모-자식 관계를 빠르게 조회하기 위한 인덱스다.
#[derive(Debug, Clone, Default)]
pub struct UnitHierarchyIndex {
    children_by_parent: BTreeMap<Option<String>, Vec<String>>,
}

impl UnitHierarchyIndex {
    pub fn build<'a>(units: impl Iterator<Item = &'a CodeUnit>) -> Self {
        let mut children_by_parent: BTreeMap<Option<String>, Vec<String>> = BTreeMap::new();
        for unit in units {
            children_by_parent
                .entry(unit.parent_id.clone())
                .or_default()
                .push(unit.id.clone());
        }
        for children in children_by_parent.values_mut() {
            children.sort();
        }
        Self { children_by_parent }
    }

    pub fn children_of(&self, parent_id: Option<&str>) -> &[String] {
        self.children_by_parent
            .get(&parent_id.map(str::to_owned))
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub fn root_ids(&self) -> &[String] {
        self.children_of(None)
    }
}
