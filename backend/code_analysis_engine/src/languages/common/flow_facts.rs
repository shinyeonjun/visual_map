//! Tree-sitter 제어 구문을 공통 ControlFlowFact로 변환한다.

use crate::facts::{ControlFlowFact, ControlFlowKind};
use crate::languages::common::metadata::{node_span, node_text, stable_id};
use crate::model::FileEntry;
use tree_sitter::Node;

pub(super) fn extract(
    node: Node<'_>,
    source: &[u8],
    file: &FileEntry,
    owner_unit_id: Option<&str>,
) -> Option<ControlFlowFact> {
    let owner_unit_id = owner_unit_id?;
    let kind = kind_for(node.kind())?;
    let span = node_span(node, file);
    let body_span = child_span(node, file, &["body", "consequence", "then"]);
    let alternative_span = child_span(
        node,
        file,
        &["alternative", "else", "catch", "handler", "handlers"],
    );
    let condition = child_text(
        node,
        source,
        &["condition", "test", "scrutinee", "selector"],
    );

    Some(ControlFlowFact {
        id: stable_id(
            "flow_fact",
            &format!(
                "{}:{}:{}:{}",
                file.file_id,
                owner_unit_id,
                node.start_byte(),
                node.end_byte()
            ),
        ),
        owner_unit_id: owner_unit_id.to_string(),
        kind,
        span,
        condition,
        body_span,
        alternative_span,
    })
}

fn kind_for(node_kind: &str) -> Option<ControlFlowKind> {
    Some(match node_kind {
        "if_statement" | "if_expression" | "conditional_expression" => ControlFlowKind::Condition,
        "switch_statement" | "switch_expression" | "match_expression" => ControlFlowKind::Switch,
        "for_statement"
        | "for_in_statement"
        | "for_of_statement"
        | "while_statement"
        | "do_statement"
        | "loop_expression"
        | "while_let_statement" => ControlFlowKind::Loop,
        "return_statement" | "return_expression" => ControlFlowKind::Return,
        "throw_statement" | "throw_expression" => ControlFlowKind::Throw,
        "break_statement" | "break_expression" => ControlFlowKind::Break,
        "continue_statement" | "continue_expression" => ControlFlowKind::Continue,
        "try_statement" | "try_expression" => ControlFlowKind::Try,
        "catch_clause" | "except_clause" => ControlFlowKind::Catch,
        _ => return None,
    })
}

fn child_span(
    node: Node<'_>,
    file: &FileEntry,
    fields: &[&str],
) -> Option<crate::facts::SourceSpan> {
    fields
        .iter()
        .find_map(|field| node.child_by_field_name(field))
        .map(|child| node_span(child, file))
}

fn child_text(node: Node<'_>, source: &[u8], fields: &[&str]) -> Option<String> {
    fields
        .iter()
        .find_map(|field| node.child_by_field_name(field))
        .map(|child| node_text(child, source).trim().to_string())
        .filter(|value| !value.is_empty())
}
