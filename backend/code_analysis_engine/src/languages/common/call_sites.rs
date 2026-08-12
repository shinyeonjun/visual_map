//! 언어 공통 호출 사이트 사실 추출기.
//!
//! 프레임워크 adapter는 `router.GET("/users", handler)`처럼 호출의 대상,
//! 인자, 대입된 로컬 이름을 필요로 한다. 기존 walker는 호출 관계만 만들었기
//! 때문에 Python 외 언어에서는 이 정보를 adapter가 재구성할 수 없었다. 이
//! 모듈은 Tree-sitter의 공통 필드 계약을 우선 사용하고, 문법별 필드가 없는
//! 경우에도 안전하게 빈 값으로 남긴다.

use crate::facts::{CallSiteFact, Evidence, FactBundle};
use crate::languages::common::metadata::{node_span, node_text, stable_id};
use crate::model::{FileEntry, Language};
use tree_sitter::Node;

use super::references::{call_target_name, is_call_node};
use super::unit_index::UnitSpanIndex;

/// Python은 자체 AST 보강기가 더 정확한 decorator·assignment 정보를 만들기
/// 때문에 중복 CallSiteFact를 만들지 않는다.
pub(super) fn extract(
    language: Language,
    root: Node<'_>,
    source: &[u8],
    file: &FileEntry,
    bundle: &mut FactBundle,
    unit_index: &UnitSpanIndex,
) {
    if language == Language::Python {
        return;
    }

    for node in walk_nodes(root) {
        if !is_call_node(node.kind()) {
            continue;
        }
        let Some(callee) = call_target_name(node, source) else {
            continue;
        };
        let arguments = call_arguments(node, source);
        let assigned_name = assigned_name(node, source);
        let span = node_span(node, file);
        bundle.call_sites.push(CallSiteFact {
            id: stable_id(
                "call_site",
                &format!("{}:{}:{}", file.file_id, node.start_byte(), node.end_byte()),
            ),
            source_unit_id: unit_index.unit_for_line(node.start_position().row as u32 + 1),
            callee,
            arguments,
            assigned_name,
            evidence: vec![Evidence::new("callSite", node_text(node, source), span)],
        });
    }
}

fn walk_nodes(root: Node<'_>) -> Vec<Node<'_>> {
    let mut nodes = Vec::new();
    let mut pending = vec![root];
    while let Some(node) = pending.pop() {
        let mut children = node.named_children(&mut node.walk()).collect::<Vec<_>>();
        children.reverse();
        pending.extend(children);
        nodes.push(node);
    }
    nodes
}

fn call_arguments(node: Node<'_>, source: &[u8]) -> Vec<String> {
    let arguments = [
        "arguments",
        "argument_list",
        "actual_parameters",
        "parameters",
    ]
    .iter()
    .find_map(|field| node.child_by_field_name(field));
    arguments
        .map(|arguments| split_arguments(&node_text(arguments, source)))
        .unwrap_or_default()
}

fn assigned_name(node: Node<'_>, source: &[u8]) -> Option<String> {
    let mut ancestor = node.parent();
    for _ in 0..=4 {
        let Some(parent) = ancestor else {
            break;
        };
        let is_rhs = ["right", "value", "initializer"].iter().any(|field| {
            parent
                .child_by_field_name(field)
                .is_some_and(|rhs| contains_node(rhs, node))
        });
        if is_rhs {
            let left = ["left", "name", "pattern", "declarator", "variable"]
                .iter()
                .find_map(|field| parent.child_by_field_name(field));
            if let Some(value) = left
                .map(|left| compact(node_text(left, source)))
                .filter(|value| is_identifier_path(value))
            {
                return Some(value);
            }
        }
        ancestor = parent.parent();
    }
    None
}

fn contains_node(parent: Node<'_>, child: Node<'_>) -> bool {
    parent.start_byte() <= child.start_byte() && parent.end_byte() >= child.end_byte()
}

fn is_identifier_path(value: &str) -> bool {
    !value.is_empty()
        && value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '_' | '.' | ':')
        })
}

fn split_arguments(text: &str) -> Vec<String> {
    let text = text.trim();
    let text = text
        .strip_prefix('(')
        .and_then(|value| value.strip_suffix(')'))
        .unwrap_or(text);
    let mut arguments = Vec::new();
    let mut current = String::new();
    let mut depth = 0_i32;
    let mut quote = None;
    let mut escaped = false;

    for character in text.chars() {
        if let Some(active_quote) = quote {
            current.push(character);
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == active_quote {
                quote = None;
            }
            continue;
        }

        match character {
            '\'' | '"' | '`' => {
                quote = Some(character);
                current.push(character);
            }
            '(' | '[' | '{' | '<' => {
                depth += 1;
                current.push(character);
            }
            ')' | ']' | '}' | '>' => {
                depth -= 1;
                current.push(character);
            }
            ',' if depth == 0 => push_argument(&mut arguments, &mut current),
            _ => current.push(character),
        }
    }
    push_argument(&mut arguments, &mut current);
    arguments
}

fn push_argument(arguments: &mut Vec<String>, current: &mut String) {
    let value = compact(current.clone());
    if !value.is_empty() {
        arguments.push(value);
    }
    current.clear();
}

fn compact(value: String) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}
