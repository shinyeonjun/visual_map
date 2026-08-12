//! Python decorator·호출을 프레임워크 비의존 원시 사실로 변환한다.

use super::ast::{named_children, unit_for_position, UnitSpanIndex};
use crate::facts::{BindingKind, CallSiteFact, DecoratorFact, Evidence, FactBundle, SymbolBinding};
use crate::languages::common::metadata::{node_span, node_text, stable_id};
use crate::model::FileEntry;
use tree_sitter::Node;

pub(super) fn extract_decorator_facts(
    decorated: Node<'_>,
    source: &[u8],
    file: &FileEntry,
    bundle: &mut FactBundle,
) {
    let Some(definition) = decorated.child_by_field_name("definition") else {
        return;
    };
    let Some(name_node) = definition.child_by_field_name("name") else {
        return;
    };
    let Some(unit_id) = bundle
        .units
        .iter()
        .find(|unit| {
            unit.name == node_text(name_node, source).trim()
                && unit.span.start_line == definition.start_position().row as u32 + 1
        })
        .map(|unit| unit.id.clone())
    else {
        return;
    };

    for decorator in named_children(decorated)
        .into_iter()
        .filter(|child| child.kind() == "decorator")
    {
        let expression = node_text(decorator, source)
            .trim()
            .trim_start_matches('@')
            .trim()
            .to_string();
        let (callee, arguments) = split_call_expression(&expression);
        let (receiver, name) = split_receiver_and_name(&callee);
        bundle.decorators.push(DecoratorFact {
            id: stable_id(
                "decorator",
                &format!("{}:{}", file.file_id, decorator.start_byte()),
            ),
            unit_id: unit_id.clone(),
            receiver,
            name,
            arguments,
            expression: expression.clone(),
            evidence: vec![Evidence::new(
                "decorator",
                expression,
                node_span(decorator, file),
            )],
        });
    }
}

pub(super) fn extract_call_site_facts(
    node: Node<'_>,
    source: &[u8],
    file: &FileEntry,
    bundle: &mut FactBundle,
    unit_index: &UnitSpanIndex,
) {
    let Some(function) = node.child_by_field_name("function") else {
        return;
    };
    let callee = compact(node_text(function, source));
    if callee.is_empty() {
        return;
    }
    let arguments = node
        .child_by_field_name("arguments")
        .map(|arguments| split_arguments(&node_text(arguments, source)))
        .unwrap_or_default();
    let receiver = callee
        .rsplit_once('.')
        .map(|(receiver, _)| receiver.to_string())
        .filter(|receiver| !receiver.is_empty());
    let assigned_name = node
        .parent()
        .and_then(|parent| {
            if parent.kind() != "assignment" {
                return None;
            }
            let right = parent.child_by_field_name("right")?;
            if node.start_byte() < right.start_byte() || node.end_byte() > right.end_byte() {
                return None;
            }
            parent
                .child_by_field_name("left")
                .map(|left| compact(node_text(left, source)))
        })
        .filter(|name| !name.is_empty());
    let span = node_span(node, file);
    bundle.call_sites.push(CallSiteFact {
        id: stable_id(
            "call_site",
            &format!("{}:{}:{}", file.file_id, node.start_byte(), node.end_byte()),
        ),
        source_unit_id: unit_for_position(unit_index, node.start_position()),
        callee,
        receiver,
        arguments,
        assigned_name,
        evidence: vec![Evidence::new("callSite", node_text(node, source), span)],
    });
    if let Some(local_name) = bundle
        .call_sites
        .last()
        .and_then(|call| call.assigned_name.clone())
    {
        let evidence = bundle
            .call_sites
            .last()
            .and_then(|call| call.evidence.first())
            .cloned();
        if let Some(evidence) = evidence {
            bundle.bindings.push(SymbolBinding {
                id: stable_id(
                    "binding",
                    &format!("{}:{}:{}", file.file_id, node.start_byte(), local_name),
                ),
                source_unit_id: unit_for_position(unit_index, node.start_position()),
                local_name,
                target_name: bundle
                    .call_sites
                    .last()
                    .map(|call| call.callee.clone())
                    .unwrap_or_default(),
                kind: BindingKind::Assignment,
                evidence: vec![evidence],
            });
        }
    }
}

fn split_call_expression(expression: &str) -> (String, Vec<String>) {
    let Some(open) = expression.find('(') else {
        return (expression.trim().to_string(), Vec::new());
    };
    let callee = expression[..open].trim().to_string();
    let arguments = split_arguments(&expression[open..]);
    (callee, arguments)
}

fn split_receiver_and_name(callee: &str) -> (Option<String>, String) {
    let Some((receiver, name)) = callee.rsplit_once('.') else {
        return (None, callee.to_string());
    };
    (Some(receiver.trim().to_string()), name.trim().to_string())
}

fn split_arguments(text: &str) -> Vec<String> {
    let text = text.trim();
    let text = if text.starts_with('(') && text.ends_with(')') {
        &text[1..text.len() - 1]
    } else {
        text
    };
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
            '\'' | '"' => {
                quote = Some(character);
                current.push(character);
            }
            '(' | '[' | '{' => {
                depth += 1;
                current.push(character);
            }
            ')' | ']' | '}' => {
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
    let value = current
        .trim()
        .trim_matches(|character| character == '(' || character == ')');
    if !value.is_empty() {
        arguments.push(value.trim().to_string());
    }
    current.clear();
}

fn compact(value: String) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}
