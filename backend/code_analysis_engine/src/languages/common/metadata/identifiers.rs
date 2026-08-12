//! 코드 유닛 이름, 계층 이름, 위치, 안정 ID를 추출한다.

use crate::facts::{CodeUnitKind, SourceSpan};
use crate::model::FileEntry;
use sha2::{Digest, Sha256};
use tree_sitter::Node;

pub(crate) fn node_name(node: Node<'_>, source: &[u8], kind: &CodeUnitKind) -> Option<String> {
    if let Some(name) = node.child_by_field_name("name") {
        let text = node_text(name, source).trim().to_string();
        if !text.is_empty() {
            return Some(text);
        }
    }
    if let Some(declarator) = node.child_by_field_name("declarator") {
        let name = if matches!(
            kind,
            CodeUnitKind::Function | CodeUnitKind::Method | CodeUnitKind::Constructor
        ) {
            // C++ qualified declarator는 `todos::getTodo`처럼 부모 타입과
            // 실제 함수 이름을 함께 가진다. 첫 식별자를 사용하면 모든
            // 메서드가 `todos`로 뭉개져 route handler와 호출 대상을 잃는다.
            declarator_function_name(declarator, source)
                .or_else(|| last_identifier(declarator, source))
                .or_else(|| first_identifier(declarator, source))
        } else {
            first_identifier(declarator, source)
        };
        if let Some(name) = name {
            return Some(name);
        }
    }

    if matches!(kind, CodeUnitKind::Lambda) {
        return Some(format!(
            "<lambda@{}:{}>",
            node.start_position().row + 1,
            node.start_position().column + 1
        ));
    }

    if matches!(
        kind,
        CodeUnitKind::Class
            | CodeUnitKind::Interface
            | CodeUnitKind::Struct
            | CodeUnitKind::Enum
            | CodeUnitKind::Trait
            | CodeUnitKind::Impl
            | CodeUnitKind::Record
            | CodeUnitKind::Mixin
            | CodeUnitKind::Extension
            | CodeUnitKind::Module
            | CodeUnitKind::Namespace
            | CodeUnitKind::Package
            | CodeUnitKind::TypeAlias
            | CodeUnitKind::Property
    ) {
        return first_identifier(node, source);
    }

    None
}

/// 부모의 qualified name을 이어 붙여 사람이 읽을 수 있는 계층 이름을 만든다.
///
/// 호출자가 부모 이름을 인덱스에서 먼저 조회하도록 해 선언 수가 많은
/// 파일에서도 전체 유닛 목록을 반복 검색하지 않는다.
pub(crate) fn qualified_name(parent_qualified_name: Option<&str>, name: &str) -> String {
    parent_qualified_name
        .map(|parent| format!("{parent}::{name}"))
        .unwrap_or_else(|| name.to_string())
}

pub(crate) fn file_module_name(relative_path: &str) -> String {
    let normalized = relative_path.replace('\\', "/");
    let mut parts: Vec<String> = normalized
        .split('/')
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect();
    if let Some(last) = parts.last_mut() {
        if let Some((stem, _)) = last.rsplit_once('.') {
            *last = stem.to_string();
        }
    }
    if parts.is_empty() {
        relative_path.to_string()
    } else {
        parts.join("::")
    }
}

fn first_identifier(node: Node<'_>, source: &[u8]) -> Option<String> {
    if matches!(
        node.kind(),
        "identifier"
            | "field_identifier"
            | "type_identifier"
            | "namespace_identifier"
            | "property_identifier"
    ) {
        let text = node_text(node, source).trim().to_string();
        if !text.is_empty() {
            return Some(text);
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if let Some(name) = first_identifier(child, source) {
            return Some(name);
        }
    }
    None
}

fn last_identifier(node: Node<'_>, source: &[u8]) -> Option<String> {
    let mut last = None;
    if matches!(
        node.kind(),
        "identifier"
            | "field_identifier"
            | "type_identifier"
            | "namespace_identifier"
            | "property_identifier"
    ) {
        let text = node_text(node, source).trim().to_string();
        if !text.is_empty() {
            last = Some(text);
        }
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if let Some(name) = last_identifier(child, source) {
            last = Some(name);
        }
    }
    last
}

fn declarator_function_name(node: Node<'_>, source: &[u8]) -> Option<String> {
    let mut current = node;
    while let Some(declarator) = current.child_by_field_name("declarator") {
        current = declarator;
    }
    last_identifier(current, source)
}

pub(crate) fn node_text(node: Node<'_>, source: &[u8]) -> String {
    node.utf8_text(source).unwrap_or_default().to_string()
}

pub(crate) fn node_span(node: Node<'_>, file: &FileEntry) -> SourceSpan {
    let start = node.start_position();
    let end = node.end_position();
    SourceSpan::new(
        file.file_id.clone(),
        file.relative_path.clone(),
        start.row as u32 + 1,
        start.column as u32 + 1,
        end.row as u32 + 1,
        end.column as u32 + 1,
    )
}

pub(crate) fn kind_key(kind: &CodeUnitKind) -> &'static str {
    match kind {
        CodeUnitKind::File => "file",
        CodeUnitKind::Module => "module",
        CodeUnitKind::Namespace => "namespace",
        CodeUnitKind::Package => "package",
        CodeUnitKind::Class => "class",
        CodeUnitKind::Interface => "interface",
        CodeUnitKind::Struct => "struct",
        CodeUnitKind::Enum => "enum",
        CodeUnitKind::Trait => "trait",
        CodeUnitKind::Impl => "impl",
        CodeUnitKind::Record => "record",
        CodeUnitKind::Mixin => "mixin",
        CodeUnitKind::Extension => "extension",
        CodeUnitKind::TypeAlias => "type_alias",
        CodeUnitKind::Function => "function",
        CodeUnitKind::Method => "method",
        CodeUnitKind::Constructor => "constructor",
        CodeUnitKind::Property => "property",
        CodeUnitKind::Lambda => "lambda",
        CodeUnitKind::Entity => "entity",
        CodeUnitKind::Repository => "repository",
        CodeUnitKind::Unknown => "unknown",
    }
}

pub(crate) fn stable_id(prefix: &str, value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    let digest = hasher.finalize();
    let hex = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("{prefix}_{}", &hex[..24])
}
