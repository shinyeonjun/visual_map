//! Python AST 순회와 코드 유닛 위치 매핑을 담당한다.

use tree_sitter::{Node, Point};

pub(super) use crate::languages::common::unit_index::UnitSpanIndex;

pub(super) fn walk_nodes(root: Node<'_>) -> Vec<Node<'_>> {
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

pub(super) fn named_children(node: Node<'_>) -> Vec<Node<'_>> {
    node.named_children(&mut node.walk()).collect()
}

pub(super) fn unit_for_position(index: &UnitSpanIndex, position: Point) -> String {
    index.unit_for_line(position.row as u32 + 1)
}
