//! Tree-sitter AST를 순회하며 선언·참조·제어흐름 Facts를 수집한다.

use crate::facts::{
    CodeUnit, CodeUnitKind, Entrypoint, EntrypointKind, Evidence, FactBundle, Reference,
    ReferenceKind, ResolutionStatus,
};
use crate::model::FileEntry;
use std::collections::{HashMap, HashSet};
use tree_sitter::Node;

use super::declarations::declaration_kind;
use super::flow_facts;
use super::metadata::{
    declaration_body, declaration_header, extract_modifiers, extract_parameters,
    extract_return_type, extract_visibility, is_exported, node_name, node_span, qualified_name,
    stable_id,
};
use super::references::{call_resolution_status, call_target_name, is_call_node, is_import_node};

pub(super) fn walk_tree(
    node: Node<'_>,
    source: &[u8],
    file: &FileEntry,
    current_parent: &str,
    current_flow_owner: Option<&str>,
    bundle: &mut FactBundle,
) {
    // AST 깊이가 큰 generated C/C++ 파일도 처리할 수 있도록 Rust 호출
    // 스택을 사용하지 않는다. 자식은 역순으로 넣어 기존의 preorder 방문
    // 순서와 결정적인 Facts 병합 순서를 유지한다.
    let mut pending = vec![(
        node,
        current_parent.to_string(),
        current_flow_owner.map(str::to_owned),
    )];
    // 선언이 많을수록 `bundle.units.iter().any(...)`와 부모 유닛 검색이
    // 이차 시간으로 증가한다. 파일 분석 중에는 ID와 qualified name을
    // 별도 인덱스로 유지해 두 조회 모두 평균 O(1)로 처리한다.
    let mut seen_unit_ids: HashSet<String> =
        bundle.units.iter().map(|unit| unit.id.clone()).collect();
    let mut qualified_names: HashMap<String, String> = bundle
        .units
        .iter()
        .map(|unit| (unit.id.clone(), unit.qualified_name.clone()))
        .collect();

    while let Some((node, current_parent, current_flow_owner)) = pending.pop() {
        let mut parent_for_children = current_parent.clone();
        let mut flow_owner_for_children = current_flow_owner.clone();

        if let Some(kind) = declaration_kind(node.kind(), &current_parent, bundle) {
            if let Some(name) = node_name(node, source, &kind) {
                let is_flow_unit = matches!(
                    kind,
                    CodeUnitKind::Function
                        | CodeUnitKind::Method
                        | CodeUnitKind::Constructor
                        | CodeUnitKind::Lambda
                );
                let span = node_span(node, file);
                let id = stable_id(
                    "unit",
                    &format!(
                        "{}:{}:{}:{}",
                        file.file_id,
                        super::metadata::kind_key(&kind),
                        node.start_byte(),
                        node.end_byte()
                    ),
                );
                let header = declaration_header(node, source);
                let parent_id = Some(current_parent.clone());
                let qualified_name = qualified_name(
                    qualified_names.get(&current_parent).map(String::as_str),
                    &name,
                );
                let body_span = declaration_body(node).map(|body| node_span(body, file));
                let parameters = extract_parameters(node, source);
                let return_type = extract_return_type(node, source, &kind);
                let visibility = extract_visibility(&header);
                let modifiers = extract_modifiers(&header);
                let exported = is_exported(node, &header, &visibility);
                if seen_unit_ids.insert(id.clone()) {
                    qualified_names.insert(id.clone(), qualified_name.clone());
                    bundle.units.push(CodeUnit {
                        id: id.clone(),
                        kind: kind.clone(),
                        name: name.clone(),
                        qualified_name,
                        file_id: file.file_id.clone(),
                        relative_path: file.relative_path.clone(),
                        language: file.language,
                        parent_id,
                        span,
                        body_span,
                        signature: (!header.is_empty()).then_some(header.clone()),
                        parameters,
                        return_type,
                        visibility,
                        modifiers,
                        exported,
                    });
                }
                parent_for_children = id.clone();
                if is_flow_unit {
                    flow_owner_for_children = Some(id.clone());
                }

                if is_language_main_entrypoint(
                    file.language,
                    &kind,
                    &header,
                    &current_parent,
                    &bundle.units,
                    &name,
                ) {
                    bundle.entrypoints.push(Entrypoint {
                        id: stable_id("entry", &format!("{}:main", file.file_id)),
                        unit_id: id,
                        kind: EntrypointKind::Main,
                        name,
                        method: None,
                        path: None,
                        framework_id: None,
                        evidence: vec![Evidence::new("symbol", "main", node_span(node, file))],
                    });
                }
            }
        }

        if let Some(flow_fact) = flow_facts::extract(
            node,
            source,
            file,
            flow_owner_for_children
                .as_deref()
                .or(current_flow_owner.as_deref()),
        ) {
            bundle.control_flow.push(flow_fact);
        }

        if is_import_node(node.kind()) {
            let text = super::metadata::node_text(node, source);
            bundle.references.push(Reference {
                id: stable_id(
                    "reference",
                    &format!("{}:import:{}:{}", file.file_id, node.start_byte(), text),
                ),
                source_unit_id: current_parent.clone(),
                target_unit_id: None,
                candidate_unit_ids: Vec::new(),
                target_name: super::references::normalize_reference_name(&text),
                kind: if node.kind().contains("include") {
                    ReferenceKind::Include
                } else {
                    ReferenceKind::Import
                },
                status: ResolutionStatus::Candidate,
                evidence: vec![Evidence::new("import", text, node_span(node, file))],
            });
        }

        if is_call_node(node.kind()) {
            if let Some(target_name) = call_target_name(node, source) {
                let status = call_resolution_status(&target_name);
                bundle.references.push(Reference {
                    id: stable_id(
                        "reference",
                        &format!(
                            "{}:call:{}:{}:{}",
                            file.file_id,
                            node.start_byte(),
                            node.end_byte(),
                            target_name
                        ),
                    ),
                    source_unit_id: current_parent.clone(),
                    target_unit_id: None,
                    candidate_unit_ids: Vec::new(),
                    target_name: target_name.clone(),
                    kind: if matches!(
                        node.kind(),
                        "new_expression"
                            | "object_creation_expression"
                            | "object_creation"
                            | "class_instance_creation_expression"
                            | "instance_creation_expression"
                    ) {
                        ReferenceKind::Constructs
                    } else {
                        ReferenceKind::Call
                    },
                    status,
                    evidence: vec![Evidence::new("call", target_name, node_span(node, file))],
                });
            }
        }

        let mut children = node.children(&mut node.walk()).collect::<Vec<_>>();
        children.reverse();
        for child in children {
            pending.push((
                child,
                parent_for_children.clone(),
                flow_owner_for_children.clone(),
            ));
        }
    }
}

fn is_language_main_entrypoint(
    language: crate::model::Language,
    kind: &CodeUnitKind,
    header: &str,
    parent_id: &str,
    units: &[CodeUnit],
    name: &str,
) -> bool {
    if name != "main"
        || !matches!(
            kind,
            CodeUnitKind::Function | CodeUnitKind::Method | CodeUnitKind::Constructor
        )
    {
        return false;
    }

    let parent_kind = units
        .iter()
        .find(|unit| unit.id == parent_id)
        .map(|unit| &unit.kind);
    let top_level = matches!(
        parent_kind,
        Some(
            CodeUnitKind::File
                | CodeUnitKind::Module
                | CodeUnitKind::Package
                | CodeUnitKind::Namespace
        )
    );

    match language {
        crate::model::Language::C
        | crate::model::Language::Cpp
        | crate::model::Language::Go
        | crate::model::Language::Rust
        | crate::model::Language::Dart => top_level,
        crate::model::Language::Java | crate::model::Language::CSharp => {
            !top_level && has_static_modifier(header)
        }
        crate::model::Language::JavaScript
        | crate::model::Language::TypeScript
        | crate::model::Language::Python
        | crate::model::Language::Unknown => false,
    }
}

fn has_static_modifier(header: &str) -> bool {
    header
        .split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
        .any(|token| token.eq_ignore_ascii_case("static"))
}
