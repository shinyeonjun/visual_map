//! 선언 헤더, 매개변수, 반환 타입, 접근 제어 정보를 추출한다.

use crate::facts::{CodeParameter, CodeUnitKind, CodeUnitVisibility};
use tree_sitter::Node;

use super::identifiers::node_text;

/// 선언 노드에서 본문을 제외한 헤더만 추출한다.
pub(crate) fn declaration_header(node: Node<'_>, source: &[u8]) -> String {
    let end_byte = declaration_body(node)
        .map(|body| body.start_byte())
        .unwrap_or_else(|| node.end_byte());
    let text = source
        .get(node.start_byte()..end_byte)
        .and_then(|bytes| std::str::from_utf8(bytes).ok())
        .unwrap_or_default();
    let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
    normalized.chars().take(512).collect()
}

pub(crate) fn declaration_body(node: Node<'_>) -> Option<Node<'_>> {
    [
        "body",
        "block",
        "class_body",
        "declaration_list",
        "field_declaration_list",
    ]
    .iter()
    .find_map(|field| node.child_by_field_name(field))
}

pub(crate) fn extract_parameters(node: Node<'_>, source: &[u8]) -> Vec<CodeParameter> {
    let parameter_node = ["parameters", "formal_parameters", "parameter_list"]
        .iter()
        .find_map(|field| node.child_by_field_name(field))
        .or_else(|| {
            // C# primary constructor의 `parameter_list`는 class_declaration의
            // named child이지만 field로 노출되지 않는다.
            node.named_children(&mut node.walk())
                .find(|child| child.kind() == "parameter_list")
        });
    let Some(parameter_node) = parameter_node else {
        return Vec::new();
    };

    let mut cursor = parameter_node.walk();
    parameter_node
        .named_children(&mut cursor)
        .filter_map(|parameter| parse_parameter(node_text(parameter, source)))
        .collect()
}

fn parse_parameter(raw: String) -> Option<CodeParameter> {
    let mut text = raw.trim().trim_matches(',').trim().to_string();
    if text.is_empty() || matches!(text.as_str(), "(" | ")" | "{" | "}") {
        return None;
    }

    let variadic = text.starts_with("...") || text.starts_with('*') || text.starts_with('&');
    text = text
        .trim_start_matches("...")
        .trim_start_matches('*')
        .trim_start_matches('&')
        .trim()
        .to_string();

    let (text, default_value) = match text.split_once('=') {
        Some((left, right)) => (left.trim().to_string(), Some(right.trim().to_string())),
        None => (text, None),
    };
    let text = text
        .trim_start_matches("mut ")
        .trim_start_matches("ref ")
        .trim_start_matches("final ")
        .trim();

    let (name, type_annotation) = if let Some((name, type_name)) = text.split_once(':') {
        (name.trim().to_string(), Some(type_name.trim().to_string()))
    } else {
        let tokens: Vec<_> = text.split_whitespace().collect();
        if tokens.len() <= 1 {
            (text.to_string(), None)
        } else {
            let name = tokens.last()?.trim_start_matches('*').to_string();
            let type_name = tokens[..tokens.len() - 1].join(" ");
            (name, Some(type_name))
        }
    };

    if name.is_empty() {
        return None;
    }
    Some(CodeParameter {
        name,
        type_annotation,
        default_value,
        variadic,
    })
}

pub(crate) fn extract_return_type(
    node: Node<'_>,
    source: &[u8],
    kind: &CodeUnitKind,
) -> Option<String> {
    if !matches!(
        kind,
        CodeUnitKind::Function
            | CodeUnitKind::Method
            | CodeUnitKind::Constructor
            | CodeUnitKind::Lambda
    ) {
        return None;
    }
    for field in ["return_type", "result_type", "return_annotation"] {
        if let Some(return_node) = node.child_by_field_name(field) {
            let raw_value = node_text(return_node, source);
            let value = raw_value
                .trim()
                .trim_start_matches("->")
                .trim_start_matches(':')
                .trim();
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }

    let header = declaration_header(node, source);
    if let Some((_, value)) = header.rsplit_once("->") {
        let value = value
            .split('{')
            .next()
            .unwrap_or(value)
            .trim()
            .trim_end_matches(':')
            .trim();
        if !value.is_empty() {
            return Some(value.to_string());
        }
    }
    if let Some(close) = header.rfind(')') {
        let suffix = header[close + 1..].trim();
        if let Some(value) = suffix.strip_prefix(':') {
            let value = value.trim().trim_end_matches(';').trim();
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

pub(crate) fn extract_visibility(header: &str) -> CodeUnitVisibility {
    let lower = header.to_ascii_lowercase();
    if lower.contains("pub(crate)") {
        CodeUnitVisibility::Crate
    } else if lower
        .split_whitespace()
        .any(|token| token == "public" || token == "pub")
    {
        CodeUnitVisibility::Public
    } else if lower.split_whitespace().any(|token| token == "private") {
        CodeUnitVisibility::Private
    } else if lower.split_whitespace().any(|token| token == "protected") {
        CodeUnitVisibility::Protected
    } else if lower.split_whitespace().any(|token| token == "internal") {
        CodeUnitVisibility::Internal
    } else if lower.split_whitespace().any(|token| token == "package") {
        CodeUnitVisibility::Package
    } else {
        CodeUnitVisibility::Unknown
    }
}

pub(crate) fn extract_modifiers(header: &str) -> Vec<String> {
    const MODIFIERS: [&str; 19] = [
        "async",
        "static",
        "abstract",
        "final",
        "virtual",
        "override",
        "const",
        "constexpr",
        "readonly",
        "sealed",
        "synchronized",
        "extern",
        "inline",
        "unsafe",
        "default",
        "partial",
        "suspend",
        "operator",
        "generator",
    ];
    let lower = header.to_ascii_lowercase();
    MODIFIERS
        .iter()
        .filter(|modifier| {
            lower
                .split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
                .any(|token| token == **modifier)
        })
        .map(|modifier| (*modifier).to_string())
        .collect()
}

pub(crate) fn is_exported(node: Node<'_>, header: &str, visibility: &CodeUnitVisibility) -> bool {
    node.kind().contains("export")
        || header
            .split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
            .any(|token| token.eq_ignore_ascii_case("export"))
        || matches!(
            visibility,
            CodeUnitVisibility::Public | CodeUnitVisibility::Crate
        )
}
