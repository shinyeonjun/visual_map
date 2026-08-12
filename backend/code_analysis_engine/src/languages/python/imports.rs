//! Python import 구문을 정규화한다.

use super::ast::{named_children, unit_for_position, UnitSpanIndex};
use crate::facts::{
    BindingKind, Evidence, FactBundle, Reference, ReferenceKind, ResolutionStatus, SymbolBinding,
};
use crate::languages::common::metadata::{node_span, node_text, stable_id};
use crate::model::FileEntry;
use tree_sitter::Node;

pub(super) fn extract_import_references(
    node: Node<'_>,
    source: &[u8],
    file: &FileEntry,
    bundle: &mut FactBundle,
    unit_index: &UnitSpanIndex,
) {
    let source_unit_id = unit_for_position(unit_index, node.start_position());
    let statement = node_text(node, source).trim().to_string();
    let mut targets: Vec<(String, String)> = Vec::new();

    match node.kind() {
        "import_statement" => {
            for child in named_children(node) {
                match child.kind() {
                    "dotted_name" => {
                        let target = node_text(child, source).trim().to_string();
                        let local = target.split('.').next().unwrap_or(&target).to_string();
                        targets.push((target, local));
                    }
                    "aliased_import" => {
                        if let Some(name) = child.child_by_field_name("name") {
                            let target = node_text(name, source).trim().to_string();
                            let local = child
                                .child_by_field_name("alias")
                                .map(|alias| node_text(alias, source).trim().to_string())
                                .unwrap_or_else(|| {
                                    target.split('.').next().unwrap_or(&target).to_string()
                                });
                            targets.push((target, local));
                        }
                    }
                    _ => {}
                }
            }
        }
        "future_import_statement" => {
            for child in named_children(node) {
                if matches!(child.kind(), "dotted_name" | "aliased_import") {
                    let name = child
                        .child_by_field_name("name")
                        .map(|name| node_text(name, source))
                        .unwrap_or_else(|| node_text(child, source));
                    let local = name.trim().to_string();
                    targets.push((format!("__future__.{local}"), local));
                }
            }
        }
        "import_from_statement" => {
            let Some(module) = node.child_by_field_name("module_name") else {
                return;
            };
            let module_name = node_text(module, source).trim().to_string();
            for child in named_children(node) {
                if child.start_byte() <= module.end_byte() {
                    continue;
                }
                let (imported, local_override) = match child.kind() {
                    "dotted_name" => (node_text(child, source).trim().to_string(), None),
                    "aliased_import" => {
                        let imported = child
                            .child_by_field_name("name")
                            .map(|name| node_text(name, source).trim().to_string())
                            .unwrap_or_default();
                        let local = child
                            .child_by_field_name("alias")
                            .map(|alias| node_text(alias, source).trim().to_string());
                        (imported, local)
                    }
                    "wildcard_import" => ("*".to_string(), None),
                    _ => continue,
                };
                if imported.is_empty() {
                    continue;
                }
                let target = if imported == "*" {
                    format!("{module_name}.*")
                } else {
                    format!("{module_name}.{imported}")
                };
                let local = local_override.unwrap_or_else(|| {
                    imported.rsplit('.').next().unwrap_or(&imported).to_string()
                });
                targets.push((target, local));
            }
        }
        _ => return,
    }

    for (target_name, local_name) in targets
        .into_iter()
        .filter(|(target, local)| !target.is_empty() && !local.is_empty())
    {
        let evidence = Evidence::new("import", statement.clone(), node_span(node, file));
        let reference_id = stable_id(
            "reference",
            &format!(
                "{}:python-import:{}:{}",
                file.file_id,
                node.start_byte(),
                target_name
            ),
        );
        bundle.references.push(Reference {
            id: reference_id,
            source_unit_id: source_unit_id.clone(),
            target_unit_id: None,
            candidate_unit_ids: Vec::new(),
            target_name: target_name.clone(),
            kind: ReferenceKind::Import,
            status: ResolutionStatus::Candidate,
            evidence: vec![evidence.clone()],
        });
        bundle.bindings.push(SymbolBinding {
            id: stable_id(
                "binding",
                &format!("{}:{}:{}", file.file_id, node.start_byte(), local_name),
            ),
            source_unit_id: source_unit_id.clone(),
            local_name,
            target_name,
            kind: BindingKind::Import,
            evidence: vec![evidence],
        });
    }
}
