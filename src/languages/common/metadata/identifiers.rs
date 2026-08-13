//! 코드 유닛 이름, 계층 이름, 위치, 안정 ID를 추출한다.

use crate::facts::{CodeUnitKind, SourceSpan};
use crate::model::FileEntry;
use sha2::{Digest, Sha256};
use tree_sitter::Node;

pub(crate) fn node_name(node: Node<'_>, source: &[u8], kind: &CodeUnitKind) -> Option<String> {
    if *kind == CodeUnitKind::Impl {
        if let Some(target) = first_identifier(node, source) {
            return Some(format!("impl {target}"));
        }
    }

    // C++ 람다의 capture 목록은 `name`처럼 보이는 자식 노드를 가질 수
    // 있다. 이를 선언 이름으로 사용하면 `[value](...) { ... }`가
    // `value` 함수로 오인된다. 람다는 항상 위치 기반 합성 이름을 쓴다.
    if *kind == CodeUnitKind::Lambda {
        return Some(format!(
            "<lambda@{}:{}>",
            node.start_position().row + 1,
            node.start_position().column + 1
        ));
    }

    if matches!(kind, CodeUnitKind::Package | CodeUnitKind::Namespace) {
        if let Some(path) = declared_path(node, source, kind) {
            return Some(path);
        }
    }

    if matches!(kind, CodeUnitKind::Constructor) && node.kind().contains("constructor_signature") {
        if let Some(name) = signature_declaration_name(node, source) {
            return Some(name);
        }
    }

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

    if let Some(signature) = node.child_by_field_name("signature") {
        if let Some(name) = signature_declaration_name(signature, source) {
            return Some(name);
        }
    }

    // Dart의 `method_signature`는 반환 타입·get/set 키워드·이름을 하나의
    // 노드에 담고 name field를 노출하지 않는다. 첫 식별자를 고르면
    // `int get value`가 `int`로 기록되므로 괄호 앞의 마지막 토큰을
    // 선언 이름으로 사용한다. TypeScript처럼 name field가 있는 문법은
    // 위 경로에서 이미 반환된다.
    if node.kind() == "method_signature" {
        if let Some(name) = signature_declaration_name(node, source) {
            return Some(name);
        }
    }

    if matches!(
        kind,
        CodeUnitKind::Function | CodeUnitKind::Method | CodeUnitKind::Constructor
    ) {
        return first_identifier(node, source);
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
    let mut pending = vec![node];
    while let Some(current) = pending.pop() {
        if is_identifier_kind(current.kind()) {
            let text = node_text(current, source).trim().to_string();
            if !text.is_empty() {
                return Some(text);
            }
        }
        let mut children = current.children(&mut current.walk()).collect::<Vec<_>>();
        children.reverse();
        pending.extend(children);
    }
    None
}

fn last_identifier(node: Node<'_>, source: &[u8]) -> Option<String> {
    let mut last = None;
    let mut pending = vec![node];
    while let Some(current) = pending.pop() {
        if is_identifier_kind(current.kind()) {
            let text = node_text(current, source).trim().to_string();
            if !text.is_empty() {
                last = Some(text);
            }
        }
        let mut children = current.children(&mut current.walk()).collect::<Vec<_>>();
        children.reverse();
        pending.extend(children);
    }
    last
}

fn is_identifier_kind(kind: &str) -> bool {
    matches!(
        kind,
        "identifier"
            | "field_identifier"
            | "type_identifier"
            | "namespace_identifier"
            | "property_identifier"
            | "private_property_identifier"
    )
}

fn signature_declaration_name(node: Node<'_>, source: &[u8]) -> Option<String> {
    let text = node_text(node, source);
    let before_parameters = text.split('(').next().unwrap_or(text.as_str());
    let token = before_parameters
        .split_whitespace()
        .last()?
        .trim_matches(|character: char| {
            !character.is_ascii_alphanumeric() && character != '_' && character != '.'
        });
    if token.is_empty() {
        return None;
    }
    Some(token.to_string())
}

fn declared_path(node: Node<'_>, source: &[u8], kind: &CodeUnitKind) -> Option<String> {
    let keyword = match kind {
        CodeUnitKind::Package => "package",
        CodeUnitKind::Namespace => "namespace",
        _ => return None,
    };
    let text = node_text(node, source);
    let (_, remainder) = text.split_once(keyword)?;
    let path = remainder
        .trim()
        .trim_end_matches([';', '{', '}'])
        .split_whitespace()
        .next()?
        .trim();
    (!path.is_empty()).then(|| path.to_string())
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
