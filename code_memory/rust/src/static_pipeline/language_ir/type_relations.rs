//! Provider-independent inventory of explicit type hierarchy sites.
//!
//! Providers prove symbol identity and the actual source/target pair. This
//! inventory proves what the source syntax says about that pair: `extends`,
//! `implements`, a Dart mixin application, or Rust trait conformance. It never
//! resolves a name by itself and therefore cannot create a relation without a
//! provider/compiler target.

use codebase_fact_model::analysis::{ProgrammingLanguage, ProviderProtocol};
use codebase_fact_model::language_ir::LanguageRelationKind;
use tree_sitter::Node;

use super::definition_inventory::{inventory_definitions_from_root, SyntaxDefinition};
use super::syntax::{
    node_text, node_utf16_range, parse_tree, range_contains, ranges_equal, utf8_range,
};

/// C# uses one `:` list for a base class and implemented interfaces. The
/// provider-resolved target kind is required before this intent can become a
/// canonical relation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TypeRelationIntent {
    Exact(LanguageRelationKind),
    CSharpBase,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SyntaxTypeRelationSite {
    pub(crate) intent: TypeRelationIntent,
    pub(crate) source_name: String,
    pub(crate) source_utf8_range: Vec<i32>,
    pub(crate) source_utf16_range: Vec<i32>,
    pub(crate) target_name: String,
    pub(crate) target_utf8_range: Vec<i32>,
    pub(crate) target_utf16_range: Vec<i32>,
}

/// One explicit, declaration-bound type reference that is useful to the
/// product map. Local variables, casts, constructor expressions and hierarchy
/// clauses are deliberately outside this inventory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SyntaxTypeUseSite {
    pub(crate) source_name: String,
    pub(crate) source_name_utf8_range: Vec<i32>,
    pub(crate) source_name_utf16_range: Vec<i32>,
    pub(crate) source_declaration_utf8_range: Vec<i32>,
    pub(crate) source_declaration_utf16_range: Vec<i32>,
    pub(crate) target_name: String,
    pub(crate) target_utf8_range: Vec<i32>,
    pub(crate) target_utf16_range: Vec<i32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SyntaxTypeInventory {
    pub(crate) relations: Vec<SyntaxTypeRelationSite>,
    pub(crate) uses: Vec<SyntaxTypeUseSite>,
}

impl SyntaxTypeUseSite {
    pub(crate) fn source_name_range(&self, protocol: ProviderProtocol) -> &[i32] {
        match protocol {
            ProviderProtocol::LanguageServerProtocol => &self.source_name_utf16_range,
            ProviderProtocol::Scip | ProviderProtocol::CompilerApi => &self.source_name_utf8_range,
        }
    }

    pub(crate) fn source_declaration_range(&self, protocol: ProviderProtocol) -> &[i32] {
        match protocol {
            ProviderProtocol::LanguageServerProtocol => &self.source_declaration_utf16_range,
            ProviderProtocol::Scip | ProviderProtocol::CompilerApi => {
                &self.source_declaration_utf8_range
            }
        }
    }

    pub(crate) fn target_range(&self, protocol: ProviderProtocol) -> &[i32] {
        match protocol {
            ProviderProtocol::LanguageServerProtocol => &self.target_utf16_range,
            ProviderProtocol::Scip | ProviderProtocol::CompilerApi => &self.target_utf8_range,
        }
    }

    pub(crate) fn matches_target_range(&self, range: &[i32], protocol: ProviderProtocol) -> bool {
        let target = self.target_range(protocol);
        ranges_equal(target, range)
            || range_contains(range, target)
            || range_contains(target, range)
    }
}

impl SyntaxTypeRelationSite {
    pub(crate) fn source_range(&self, protocol: ProviderProtocol) -> &[i32] {
        match protocol {
            ProviderProtocol::LanguageServerProtocol => &self.source_utf16_range,
            ProviderProtocol::Scip | ProviderProtocol::CompilerApi => &self.source_utf8_range,
        }
    }

    pub(crate) fn target_range(&self, protocol: ProviderProtocol) -> &[i32] {
        match protocol {
            ProviderProtocol::LanguageServerProtocol => &self.target_utf16_range,
            ProviderProtocol::Scip | ProviderProtocol::CompilerApi => &self.target_utf8_range,
        }
    }
}

#[cfg(test)]
pub(crate) fn inventory_type_relation_sites(
    language: ProgrammingLanguage,
    path: &str,
    source: &str,
) -> Result<Vec<SyntaxTypeRelationSite>, String> {
    let tree = parse_tree(language.as_str(), path, source, "type-relation")?;
    Ok(inventory_type_relation_sites_from_root(
        language,
        tree.root_node(),
        source,
    ))
}

pub(super) fn inventory_type_relation_sites_from_root(
    language: ProgrammingLanguage,
    root: Node<'_>,
    source: &str,
) -> Vec<SyntaxTypeRelationSite> {
    let mut sites = Vec::new();
    visit(language, root, source, &mut sites);
    sites.sort_by(|left, right| {
        (
            &left.source_utf8_range,
            intent_rank(left.intent),
            &left.target_utf8_range,
            &left.target_name,
        )
            .cmp(&(
                &right.source_utf8_range,
                intent_rank(right.intent),
                &right.target_utf8_range,
                &right.target_name,
            ))
    });
    sites.dedup();
    sites
}

#[cfg(test)]
pub(crate) fn inventory_type_use_sites(
    language: ProgrammingLanguage,
    path: &str,
    source: &str,
) -> Result<Vec<SyntaxTypeUseSite>, String> {
    let tree = parse_tree(language.as_str(), path, source, "type-use")?;
    let definitions = inventory_definitions_from_root(language.as_str(), tree.root_node(), source);
    let relations = inventory_type_relation_sites_from_root(language, tree.root_node(), source);
    Ok(inventory_type_use_sites_from_root(
        language,
        tree.root_node(),
        source,
        &definitions,
        &relations,
    ))
}

pub(super) fn inventory_type_use_sites_from_root(
    language: ProgrammingLanguage,
    root: Node<'_>,
    source: &str,
    definitions: &[SyntaxDefinition],
    relations: &[SyntaxTypeRelationSite],
) -> Vec<SyntaxTypeUseSite> {
    let definition_index = DefinitionNameRangeIndex::new(definitions);
    let hierarchy_targets = relations
        .iter()
        .map(|site| site.target_utf8_range.clone())
        .collect::<Vec<_>>();
    let mut targets = Vec::new();
    visit_type_uses(language, root, source, false, &mut targets);
    targets.sort_by_key(|left| utf8_range(*left));
    targets.dedup_by(|left, right| utf8_range(*left) == utf8_range(*right));

    let mut sites = targets
        .into_iter()
        .filter(|target| {
            let range = utf8_range(*target);
            !hierarchy_targets
                .iter()
                .any(|hierarchy| ranges_equal(hierarchy, &range))
        })
        .filter_map(|target| type_use_site(target, source, definitions, &definition_index))
        .collect::<Vec<_>>();
    sites.sort_by(|left, right| {
        (
            &left.target_utf8_range,
            &left.source_name_utf8_range,
            &left.target_name,
        )
            .cmp(&(
                &right.target_utf8_range,
                &right.source_name_utf8_range,
                &right.target_name,
            ))
    });
    sites.dedup();
    sites
}

pub(crate) fn inventory_type_syntax(
    language: ProgrammingLanguage,
    path: &str,
    source: &str,
) -> Result<SyntaxTypeInventory, String> {
    let tree = parse_tree(language.as_str(), path, source, "type")?;
    let definitions = inventory_definitions_from_root(language.as_str(), tree.root_node(), source);
    let relations = inventory_type_relation_sites_from_root(language, tree.root_node(), source);
    let uses = inventory_type_use_sites_from_root(
        language,
        tree.root_node(),
        source,
        &definitions,
        &relations,
    );
    Ok(SyntaxTypeInventory { relations, uses })
}

fn type_use_site(
    target: Node<'_>,
    source: &str,
    definitions: &[SyntaxDefinition],
    definition_index: &DefinitionNameRangeIndex<'_>,
) -> Option<SyntaxTypeUseSite> {
    let target_range = utf8_range(target);
    let source_definition = nearest_context_definition(target, definition_index).or_else(|| {
        definitions
            .iter()
            .filter(|definition| {
                range_contains(&definition.declaration_utf8_range, &target_range)
                    && !ranges_equal(&definition.name_utf8_range, &target_range)
            })
            .min_by_key(|definition| range_size(&definition.declaration_utf8_range))
    })?;
    let target_name = node_text(target, source).trim();
    if target_name.is_empty() || target_name == source_definition.name {
        return None;
    }
    Some(SyntaxTypeUseSite {
        source_name: source_definition.name.clone(),
        source_name_utf8_range: source_definition.name_utf8_range.clone(),
        source_name_utf16_range: source_definition.name_utf16_range.clone(),
        source_declaration_utf8_range: source_definition.declaration_utf8_range.clone(),
        source_declaration_utf16_range: source_definition.declaration_utf16_range.clone(),
        target_name: target_name.to_string(),
        target_utf8_range: target_range,
        target_utf16_range: node_utf16_range(source, target),
    })
}

fn nearest_context_definition<'a>(
    target: Node<'_>,
    definitions: &'a DefinitionNameRangeIndex<'a>,
) -> Option<&'a SyntaxDefinition> {
    let target_range = utf8_range(target);
    let mut ancestor = target.parent();
    while let Some(current) = ancestor {
        let current_range = utf8_range(current);
        if let Some(definition) = definitions.nearest_in(&current_range, &target_range) {
            return Some(definition);
        }
        ancestor = current.parent();
    }
    None
}

struct DefinitionNameRangeIndex<'a> {
    by_start: Vec<(usize, &'a SyntaxDefinition)>,
}

impl<'a> DefinitionNameRangeIndex<'a> {
    fn new(definitions: &'a [SyntaxDefinition]) -> Self {
        let mut by_start = definitions.iter().enumerate().collect::<Vec<_>>();
        by_start.sort_by_key(|(ordinal, definition)| {
            (range_start(&definition.name_utf8_range), *ordinal)
        });
        Self { by_start }
    }

    fn nearest_in(&self, container: &[i32], target: &[i32]) -> Option<&'a SyntaxDefinition> {
        let container_start = range_start(container);
        let container_end = range_end(container);
        let first = self.by_start.partition_point(|(_, definition)| {
            range_start(&definition.name_utf8_range) < container_start
        });
        self.by_start[first..]
            .iter()
            .take_while(|(_, definition)| range_start(&definition.name_utf8_range) <= container_end)
            .filter(|(_, definition)| {
                !ranges_equal(&definition.name_utf8_range, target)
                    && range_contains(container, &definition.name_utf8_range)
            })
            .min_by_key(|(ordinal, definition)| {
                (
                    range_distance(&definition.name_utf8_range, target),
                    range_size(&definition.declaration_utf8_range),
                    definition.name_utf8_range.clone(),
                    *ordinal,
                )
            })
            .map(|(_, definition)| *definition)
    }
}

fn range_distance(left: &[i32], right: &[i32]) -> i64 {
    (range_start(left) - range_start(right)).abs()
}

fn range_start(range: &[i32]) -> i64 {
    match range {
        [line, column, ..] => i64::from(*line) * 1_000_000 + i64::from(*column),
        _ => i64::MAX / 2,
    }
}

fn range_end(range: &[i32]) -> i64 {
    match range {
        [line, _start, end] => i64::from(*line) * 1_000_000 + i64::from(*end),
        [_start_line, _start, end_line, end] => i64::from(*end_line) * 1_000_000 + i64::from(*end),
        _ => i64::MIN / 2,
    }
}

fn range_size(range: &[i32]) -> i64 {
    match range {
        [_line, start, end] => i64::from(end - start),
        [start_line, start, end_line, end] => {
            i64::from(end_line - start_line) * 1_000_000 + i64::from(end - start)
        }
        _ => i64::MAX,
    }
}

fn visit_type_uses<'tree>(
    language: ProgrammingLanguage,
    node: Node<'tree>,
    source: &str,
    inside_executable: bool,
    targets: &mut Vec<Node<'tree>>,
) {
    if !inside_executable {
        collect_type_fields(language, node, source, targets);
    }
    let child_inside_executable = inside_executable || is_executable_body_node(language, node);
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        visit_type_uses(language, child, source, child_inside_executable, targets);
    }
}

fn collect_type_fields<'tree>(
    language: ProgrammingLanguage,
    node: Node<'tree>,
    source: &str,
    targets: &mut Vec<Node<'tree>>,
) {
    if language == ProgrammingLanguage::Rust
        && matches!(node.kind(), "constrained_type_parameter" | "type_parameter")
        && inside_ancestor(node, "impl_item")
    {
        return;
    }
    if language == ProgrammingLanguage::Go
        && node.kind() == "parameter_declaration"
        && go_receiver_parameter(node)
    {
        return;
    }
    let fields: &[&str] = match language {
        ProgrammingLanguage::TypeScript => match node.kind() {
            "public_field_definition"
            | "property_signature"
            | "required_parameter"
            | "optional_parameter"
            | "rest_pattern" => &["type"],
            "function_declaration"
            | "generator_function_declaration"
            | "method_definition"
            | "method_signature"
            | "abstract_method_signature"
            | "function_signature" => &["return_type"],
            "type_parameter" => &["constraint"],
            _ => &[],
        },
        ProgrammingLanguage::JavaScript => &[],
        ProgrammingLanguage::Python => match node.kind() {
            "assignment" | "typed_parameter" => &["type"],
            "function_definition" => &["return_type"],
            _ => &[],
        },
        ProgrammingLanguage::Java => match node.kind() {
            "field_declaration" | "formal_parameter" | "spread_parameter"
            | "method_declaration" => &["type"],
            "type_parameter" => &["type_bound"],
            _ => &[],
        },
        ProgrammingLanguage::CSharp => match node.kind() {
            "variable_declaration"
            | "property_declaration"
            | "parameter"
            | "method_declaration"
            | "delegate_declaration" => &["type", "returns"],
            "type_parameter_constraints_clause" => &["constraints"],
            _ => &[],
        },
        ProgrammingLanguage::C | ProgrammingLanguage::Cpp => match node.kind() {
            "field_declaration" | "parameter_declaration" | "function_definition" => &["type"],
            _ => &[],
        },
        ProgrammingLanguage::Go => match node.kind() {
            "field_declaration" | "parameter_declaration" | "type_parameter_declaration" => {
                &["type"]
            }
            "function_declaration"
            | "method_declaration"
            | "method_elem"
            | "method_spec"
            | "function_type" => &["result"],
            _ => &[],
        },
        ProgrammingLanguage::Rust => match node.kind() {
            "field_declaration" | "parameter" => &["type"],
            "function_item" | "function_signature_item" => &["return_type"],
            "constrained_type_parameter" | "type_parameter" => &["bounds"],
            _ => &[],
        },
        ProgrammingLanguage::Dart => match node.kind() {
            "initialized_identifier"
            | "static_final_declaration"
            | "formal_parameter"
            | "normal_formal_parameter"
            | "field_formal_parameter"
            | "method_signature"
            | "function_signature" => &["type", "return_type"],
            "type_parameter" => &["bound"],
            _ => &[],
        },
    };
    for field in fields {
        if let Some(value) = node.child_by_field_name(field) {
            collect_type_names(value, source, targets);
        }
    }

    // The C-family grammars expose declaration specifiers as direct children
    // instead of a stable `type` field. Restrict this fallback to declaration
    // headers; executable-body filtering above keeps locals out.
    if matches!(language, ProgrammingLanguage::C | ProgrammingLanguage::Cpp)
        && matches!(
            node.kind(),
            "field_declaration" | "parameter_declaration" | "function_definition"
        )
        && !fields
            .iter()
            .any(|field| node.child_by_field_name(field).is_some())
    {
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            if matches!(
                child.kind(),
                "type_identifier"
                    | "primitive_type"
                    | "sized_type_specifier"
                    | "qualified_identifier"
                    | "template_type"
            ) {
                collect_type_names(child, source, targets);
            }
            if matches!(child.kind(), "function_declarator" | "compound_statement") {
                break;
            }
        }
    }
    if language == ProgrammingLanguage::Dart
        && matches!(
            node.kind(),
            "declaration" | "formal_parameter" | "function_signature" | "method_signature"
        )
    {
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            if child.kind() == "type" {
                collect_type_names(child, source, targets);
            }
        }
    }
}

fn inside_ancestor(node: Node<'_>, kind: &str) -> bool {
    let mut ancestor = node.parent();
    while let Some(current) = ancestor {
        if current.kind() == kind {
            return true;
        }
        ancestor = current.parent();
    }
    false
}

fn go_receiver_parameter(node: Node<'_>) -> bool {
    let mut ancestor = node.parent();
    while let Some(current) = ancestor {
        if current.kind() == "method_declaration" {
            return current
                .child_by_field_name("receiver")
                .is_some_and(|receiver| range_contains(&utf8_range(receiver), &utf8_range(node)));
        }
        if matches!(
            current.kind(),
            "function_declaration" | "method_declaration"
        ) {
            break;
        }
        ancestor = current.parent();
    }
    false
}

fn collect_type_names<'tree>(node: Node<'tree>, source: &str, targets: &mut Vec<Node<'tree>>) {
    if is_type_use_name(node.kind()) {
        if !node_text(node, source).trim().is_empty() {
            targets.push(node);
        }
        return;
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        collect_type_names(child, source, targets);
    }
}

fn is_type_use_name(kind: &str) -> bool {
    matches!(kind, "identifier" | "type_identifier")
}

fn is_executable_body_node(language: ProgrammingLanguage, node: Node<'_>) -> bool {
    match language {
        ProgrammingLanguage::TypeScript | ProgrammingLanguage::JavaScript => {
            node.kind() == "statement_block"
        }
        ProgrammingLanguage::Python => {
            node.kind() == "block"
                && node
                    .parent()
                    .is_none_or(|parent| parent.kind() != "class_definition")
        }
        ProgrammingLanguage::Java
        | ProgrammingLanguage::CSharp
        | ProgrammingLanguage::Go
        | ProgrammingLanguage::Rust => node.kind() == "block",
        ProgrammingLanguage::C | ProgrammingLanguage::Cpp => node.kind() == "compound_statement",
        ProgrammingLanguage::Dart => node.kind() == "function_body",
    }
}

fn visit(
    language: ProgrammingLanguage,
    node: Node<'_>,
    source: &str,
    sites: &mut Vec<SyntaxTypeRelationSite>,
) {
    match language {
        ProgrammingLanguage::TypeScript => visit_typescript(node, source, sites),
        ProgrammingLanguage::JavaScript => visit_javascript(node, source, sites),
        ProgrammingLanguage::Python => visit_python(node, source, sites),
        ProgrammingLanguage::Java => visit_java(node, source, sites),
        ProgrammingLanguage::CSharp => visit_csharp(node, source, sites),
        ProgrammingLanguage::C => {}
        ProgrammingLanguage::Cpp => visit_cpp(node, source, sites),
        // Go conformance is structural and has no explicit source clause. It
        // is admitted only from a provider-proven type hierarchy pair.
        ProgrammingLanguage::Go => {}
        ProgrammingLanguage::Rust => visit_rust(node, source, sites),
        ProgrammingLanguage::Dart => visit_dart(node, source, sites),
    }

    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        visit(language, child, source, sites);
    }
}

fn visit_typescript(node: Node<'_>, source: &str, sites: &mut Vec<SyntaxTypeRelationSite>) {
    if !matches!(node.kind(), "class_declaration" | "interface_declaration") {
        return;
    }
    let Some(source_name) = node.child_by_field_name("name") else {
        return;
    };
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        match child.kind() {
            "class_heritage" => {
                let mut heritage_cursor = child.walk();
                for clause in child.named_children(&mut heritage_cursor) {
                    let intent = match clause.kind() {
                        "extends_clause" => {
                            TypeRelationIntent::Exact(LanguageRelationKind::Extends)
                        }
                        "implements_clause" => {
                            TypeRelationIntent::Exact(LanguageRelationKind::Implements)
                        }
                        _ => continue,
                    };
                    push_clause_targets(sites, source_name, clause, intent, source);
                }
            }
            "extends_type_clause" => push_clause_targets(
                sites,
                source_name,
                child,
                TypeRelationIntent::Exact(LanguageRelationKind::Extends),
                source,
            ),
            _ => {}
        }
    }
}

fn visit_javascript(node: Node<'_>, source: &str, sites: &mut Vec<SyntaxTypeRelationSite>) {
    if !matches!(node.kind(), "class_declaration" | "class") {
        return;
    }
    let Some(source_name) = node.child_by_field_name("name") else {
        return;
    };
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() == "class_heritage" {
            push_clause_targets(
                sites,
                source_name,
                child,
                TypeRelationIntent::Exact(LanguageRelationKind::Extends),
                source,
            );
        }
    }
}

fn visit_python(node: Node<'_>, source: &str, sites: &mut Vec<SyntaxTypeRelationSite>) {
    if node.kind() != "class_definition" {
        return;
    }
    let (Some(source_name), Some(superclasses)) = (
        node.child_by_field_name("name"),
        node.child_by_field_name("superclasses"),
    ) else {
        return;
    };
    push_clause_targets(
        sites,
        source_name,
        superclasses,
        TypeRelationIntent::Exact(LanguageRelationKind::Extends),
        source,
    );
}

fn visit_java(node: Node<'_>, source: &str, sites: &mut Vec<SyntaxTypeRelationSite>) {
    if !matches!(node.kind(), "class_declaration" | "interface_declaration") {
        return;
    }
    let Some(source_name) = node.child_by_field_name("name") else {
        return;
    };
    if let Some(superclass) = node.child_by_field_name("superclass") {
        push_clause_targets(
            sites,
            source_name,
            superclass,
            TypeRelationIntent::Exact(LanguageRelationKind::Extends),
            source,
        );
    }
    if let Some(interfaces) = node.child_by_field_name("interfaces") {
        push_clause_targets(
            sites,
            source_name,
            interfaces,
            TypeRelationIntent::Exact(LanguageRelationKind::Implements),
            source,
        );
    }
    if node.kind() == "interface_declaration" {
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            if child.kind() == "extends_interfaces" {
                push_clause_targets(
                    sites,
                    source_name,
                    child,
                    TypeRelationIntent::Exact(LanguageRelationKind::Extends),
                    source,
                );
            }
        }
    }
}

fn visit_csharp(node: Node<'_>, source: &str, sites: &mut Vec<SyntaxTypeRelationSite>) {
    if !matches!(
        node.kind(),
        "class_declaration" | "record_declaration" | "struct_declaration" | "interface_declaration"
    ) {
        return;
    }
    let Some(source_name) = node.child_by_field_name("name") else {
        return;
    };
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() == "base_list" {
            push_clause_targets(
                sites,
                source_name,
                child,
                TypeRelationIntent::CSharpBase,
                source,
            );
        }
    }
}

fn visit_cpp(node: Node<'_>, source: &str, sites: &mut Vec<SyntaxTypeRelationSite>) {
    if !matches!(node.kind(), "class_specifier" | "struct_specifier") {
        return;
    }
    let Some(source_name) = node.child_by_field_name("name") else {
        return;
    };
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() == "base_class_clause" {
            push_clause_targets(
                sites,
                source_name,
                child,
                TypeRelationIntent::Exact(LanguageRelationKind::Extends),
                source,
            );
        }
    }
}

fn visit_rust(node: Node<'_>, source: &str, sites: &mut Vec<SyntaxTypeRelationSite>) {
    if node.kind() != "impl_item" {
        return;
    }
    let (Some(source_type), Some(target_trait)) = (
        node.child_by_field_name("type"),
        node.child_by_field_name("trait"),
    ) else {
        return;
    };
    let (Some(source_name), Some(target_name)) =
        (type_name_leaf(source_type), type_name_leaf(target_trait))
    else {
        return;
    };
    push_site(
        sites,
        source_name,
        target_name,
        TypeRelationIntent::Exact(LanguageRelationKind::Implements),
        source,
    );
}

fn visit_dart(node: Node<'_>, source: &str, sites: &mut Vec<SyntaxTypeRelationSite>) {
    if !matches!(node.kind(), "class_declaration" | "mixin_declaration") {
        return;
    }
    let Some(source_name) = node.child_by_field_name("name") else {
        return;
    };
    if let Some(superclass) = node.child_by_field_name("superclass") {
        let mut type_cursor = superclass.walk();
        for target in superclass.children_by_field_name("type", &mut type_cursor) {
            push_target(
                sites,
                source_name,
                target,
                TypeRelationIntent::Exact(LanguageRelationKind::Extends),
                source,
            );
        }
        let mut cursor = superclass.walk();
        for child in superclass.named_children(&mut cursor) {
            if child.kind() == "mixins" {
                push_clause_targets(
                    sites,
                    source_name,
                    child,
                    TypeRelationIntent::Exact(LanguageRelationKind::MixesIn),
                    source,
                );
            }
        }
    }
    if let Some(interfaces) = node.child_by_field_name("interfaces") {
        push_clause_targets(
            sites,
            source_name,
            interfaces,
            TypeRelationIntent::Exact(LanguageRelationKind::Implements),
            source,
        );
    }
}

fn push_clause_targets(
    sites: &mut Vec<SyntaxTypeRelationSite>,
    source_name: Node<'_>,
    clause: Node<'_>,
    intent: TypeRelationIntent,
    source: &str,
) {
    let mut cursor = clause.walk();
    for target in clause.named_children(&mut cursor) {
        if is_clause_wrapper(target.kind()) {
            push_clause_targets(sites, source_name, target, intent, source);
        } else {
            push_target(sites, source_name, target, intent, source);
        }
    }
}

fn push_target(
    sites: &mut Vec<SyntaxTypeRelationSite>,
    source_name: Node<'_>,
    target: Node<'_>,
    intent: TypeRelationIntent,
    source: &str,
) {
    if let Some(target_name) = type_name_leaf(target) {
        push_site(sites, source_name, target_name, intent, source);
    }
}

fn push_site(
    sites: &mut Vec<SyntaxTypeRelationSite>,
    source_name: Node<'_>,
    target_name: Node<'_>,
    intent: TypeRelationIntent,
    source: &str,
) {
    let source_text = node_text(source_name, source).trim();
    let target_text = node_text(target_name, source).trim();
    if source_text.is_empty() || target_text.is_empty() || source_text == target_text {
        return;
    }
    sites.push(SyntaxTypeRelationSite {
        intent,
        source_name: source_text.to_string(),
        source_utf8_range: utf8_range(source_name),
        source_utf16_range: node_utf16_range(source, source_name),
        target_name: target_text.to_string(),
        target_utf8_range: utf8_range(target_name),
        target_utf16_range: node_utf16_range(source, target_name),
    });
}

fn type_name_leaf(node: Node<'_>) -> Option<Node<'_>> {
    if is_type_name(node.kind()) {
        return Some(node);
    }
    let mut cursor = node.walk();
    let children = node
        .named_children(&mut cursor)
        .filter(|child| !is_type_argument_container(child.kind()))
        .collect::<Vec<_>>();
    children.into_iter().rev().find_map(type_name_leaf)
}

fn is_type_name(kind: &str) -> bool {
    matches!(
        kind,
        "identifier" | "type_identifier" | "field_identifier" | "namespace_identifier"
    )
}

fn is_type_argument_container(kind: &str) -> bool {
    matches!(
        kind,
        "type_arguments"
            | "type_argument_list"
            | "type_parameters"
            | "type_parameter_list"
            | "argument_list"
            | "arguments"
            | "access_specifier"
            | "attribute"
            | "attribute_declaration"
            | "annotation"
            | "marker_annotation"
    )
}

fn is_clause_wrapper(kind: &str) -> bool {
    matches!(
        kind,
        "type_list" | "extends_interfaces" | "interfaces" | "mixins" | "superclass"
    )
}

const fn intent_rank(intent: TypeRelationIntent) -> u8 {
    match intent {
        TypeRelationIntent::Exact(LanguageRelationKind::Extends) => 0,
        TypeRelationIntent::Exact(LanguageRelationKind::Implements) => 1,
        TypeRelationIntent::Exact(LanguageRelationKind::MixesIn) => 2,
        TypeRelationIntent::CSharpBase => 3,
        TypeRelationIntent::Exact(_) => 4,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn relations(language: ProgrammingLanguage, path: &str, source: &str) -> Vec<String> {
        inventory_type_relation_sites(language, path, source)
            .unwrap()
            .into_iter()
            .map(|site| {
                let kind = match site.intent {
                    TypeRelationIntent::Exact(kind) => format!("{kind:?}"),
                    TypeRelationIntent::CSharpBase => "CSharpBase".to_string(),
                };
                format!("{kind}:{}->{}", site.source_name, site.target_name)
            })
            .collect()
    }

    fn type_uses(language: ProgrammingLanguage, path: &str, source: &str) -> Vec<String> {
        inventory_type_use_sites(language, path, source)
            .unwrap()
            .into_iter()
            .map(|site| format!("{}->{}", site.source_name, site.target_name))
            .collect()
    }

    #[test]
    fn explicit_hierarchy_sites_keep_each_language_keyword_meaning() {
        assert_eq!(
            relations(
                ProgrammingLanguage::TypeScript,
                "types.ts",
                "class Child extends Base implements Contract {}\ninterface More extends Contract {}",
            ),
            [
                "Extends:Child->Base",
                "Implements:Child->Contract",
                "Extends:More->Contract",
            ]
        );
        assert_eq!(
            relations(
                ProgrammingLanguage::JavaScript,
                "types.js",
                "class Child extends Base {}",
            ),
            ["Extends:Child->Base"]
        );
        assert_eq!(
            relations(
                ProgrammingLanguage::Python,
                "types.py",
                "class Child(pkg.Base):\n    pass\n",
            ),
            ["Extends:Child->Base"]
        );
        assert_eq!(
            relations(
                ProgrammingLanguage::Java,
                "Child.java",
                "class Child extends Base implements Contract {} interface More extends Contract {}",
            ),
            [
                "Extends:Child->Base",
                "Implements:Child->Contract",
                "Extends:More->Contract",
            ]
        );
        assert_eq!(
            relations(
                ProgrammingLanguage::CSharp,
                "Child.cs",
                "class Child : Base, IContract {} interface IMore : IContract {}",
            ),
            [
                "CSharpBase:Child->Base",
                "CSharpBase:Child->IContract",
                "CSharpBase:IMore->IContract",
            ]
        );
        assert!(relations(
            ProgrammingLanguage::C,
            "types.c",
            "struct Child { int value; };",
        )
        .is_empty());
        assert_eq!(
            relations(
                ProgrammingLanguage::Cpp,
                "types.cpp",
                "class Child : public Base, private Contract {};",
            ),
            ["Extends:Child->Base", "Extends:Child->Contract"]
        );
        assert!(relations(
            ProgrammingLanguage::Go,
            "types.go",
            "package fixture\n\ntype Child struct{}\n",
        )
        .is_empty());
        assert_eq!(
            relations(
                ProgrammingLanguage::Rust,
                "types.rs",
                "impl Contract for Child {}",
            ),
            ["Implements:Child->Contract"]
        );
        assert_eq!(
            relations(
                ProgrammingLanguage::Dart,
                "types.dart",
                "class Child extends Base with Audit, Trace implements Contract {}",
            ),
            [
                "Extends:Child->Base",
                "Implements:Child->Contract",
                "MixesIn:Child->Audit",
                "MixesIn:Child->Trace",
            ]
        );
    }

    #[test]
    fn csharp_generic_base_keeps_the_outer_interface_identity() {
        assert_eq!(
            relations(
                ProgrammingLanguage::CSharp,
                "DbContext.cs",
                "class DbContext : IInfrastructure<IServiceProvider>, IDbContextDependencies {}",
            ),
            [
                "CSharpBase:DbContext->IInfrastructure",
                "CSharpBase:DbContext->IDbContextDependencies",
            ]
        );
    }

    #[test]
    fn declaration_bound_type_uses_exclude_locals_and_hierarchy_tokens() {
        assert_eq!(
            type_uses(
                ProgrammingLanguage::TypeScript,
                "types.ts",
                "class Payload {} class Base {} class Service extends Base { field: Payload; run(input: Payload): Payload { const local: Payload = input; return input; } }",
            ),
            ["field->Payload", "run->Payload", "run->Payload"]
        );
        assert_eq!(
            type_uses(
                ProgrammingLanguage::Python,
                "types.py",
                "class Payload:\n    pass\nclass Base:\n    pass\nclass Service(Base):\n    field: Payload\n    def run(self, item: Payload) -> Payload:\n        local: Payload = item\n        return item\n",
            ),
            ["field->Payload", "run->Payload", "run->Payload"]
        );
        assert_eq!(
            type_uses(
                ProgrammingLanguage::Java,
                "Types.java",
                "class Payload {} class Base {} class Service extends Base { Payload field; Payload run(Payload input) { Payload local = input; return input; } }",
            ),
            ["field->Payload", "run->Payload", "run->Payload"]
        );
        assert_eq!(
            type_uses(
                ProgrammingLanguage::CSharp,
                "Types.cs",
                "class Payload {} class Base {} class Service : Base { Payload field; Payload Run(Payload input) { Payload local = input; return input; } }",
            ),
            ["field->Payload", "Run->Payload", "Run->Payload"]
        );
        assert_eq!(
            type_uses(
                ProgrammingLanguage::C,
                "types.c",
                "typedef struct Payload { int value; } Payload; struct Holder { Payload field; }; Payload run(Payload input) { Payload local = input; return input; }",
            ),
            ["field->Payload", "run->Payload", "run->Payload"]
        );
        assert_eq!(
            type_uses(
                ProgrammingLanguage::Cpp,
                "types.cpp",
                "class Payload {}; class Base {}; class Service : public Base { Payload field; Payload run(Payload input) { Payload local = input; return input; } };",
            ),
            ["field->Payload", "run->Payload", "run->Payload"]
        );
        assert_eq!(
            type_uses(
                ProgrammingLanguage::Go,
                "types.go",
                "package fixture\ntype Payload struct{}\ntype Service struct { Field Payload }\nfunc (s Service) Run(input Payload) Payload { var local Payload = input; return local }\n",
            ),
            ["Field->Payload", "Run->Payload", "Run->Payload"]
        );
        assert_eq!(
            type_uses(
                ProgrammingLanguage::Rust,
                "types.rs",
                "struct Payload; struct Service { field: Payload } impl Service { fn run(&self, input: Payload) -> Payload { let local: Payload = input; local } }",
            ),
            ["field->Payload", "run->Payload", "run->Payload"]
        );
        assert_eq!(
            type_uses(
                ProgrammingLanguage::Dart,
                "types.dart",
                "class Payload {} class Base {} class Service extends Base { Payload field; Payload run(Payload input) { final Payload local = input; return input; } }",
            ),
            ["field->Payload", "run->Payload", "run->Payload"]
        );
        assert!(type_uses(
            ProgrammingLanguage::JavaScript,
            "types.js",
            "class Base {} class Service extends Base { run(value) { const local = new Base(); return value; } }",
        )
        .is_empty());
    }

    #[test]
    fn review_corpus_inventory_keeps_only_declaration_bound_type_sites() {
        let cases: &[(ProgrammingLanguage, &str, &str, &[&str])] = &[
            (
                ProgrammingLanguage::TypeScript,
                "types.ts",
                include_str!("../../../../tests/fixtures/type-relations/typescript/src/types.ts"),
                &[
                    "execute->Payload",
                    "execute->ResultValue",
                    "execute->Payload",
                    "execute->ResultValue",
                    "current->Payload",
                    "constructor->Payload",
                    "execute->Payload",
                    "execute->ResultValue",
                    "Store->Contract",
                    "value->T",
                ],
            ),
            (
                ProgrammingLanguage::JavaScript,
                "types.js",
                include_str!("../../../../tests/fixtures/type-relations/javascript/types.js"),
                &[],
            ),
            (
                ProgrammingLanguage::Python,
                "types.py",
                include_str!("../../../../tests/fixtures/type-relations/python/types.py"),
                &[
                    "execute->Payload",
                    "execute->ResultValue",
                    "current->Payload",
                    "execute->Payload",
                    "execute->ResultValue",
                ],
            ),
            (
                ProgrammingLanguage::Java,
                "Types.java",
                include_str!("../../../../tests/fixtures/type-relations/java/src/main/java/typefixture/Types.java"),
                &[
                    "execute->ResultValue",
                    "execute->Payload",
                    "execute->ResultValue",
                    "execute->Payload",
                    "current->Payload",
                    "Service->Payload",
                    "execute->ResultValue",
                    "execute->Payload",
                ],
            ),
            (
                ProgrammingLanguage::CSharp,
                "Types.cs",
                include_str!("../../../../tests/fixtures/type-relations/csharp/Types.cs"),
                &[
                    "Execute->ResultValue",
                    "Execute->Payload",
                    "Execute->ResultValue",
                    "Execute->Payload",
                    "current->Payload",
                    "Service->Payload",
                    "Execute->ResultValue",
                    "Execute->Payload",
                ],
            ),
            (
                ProgrammingLanguage::C,
                "types.c",
                include_str!("../../../../tests/fixtures/type-relations/c-family/types.c"),
                &["current->Payload", "transform->Payload", "transform->Payload"],
            ),
            (
                ProgrammingLanguage::Cpp,
                "types.cpp",
                include_str!("../../../../tests/fixtures/type-relations/c-family/types.cpp"),
                &[
                    "execute->ResultValue",
                    "execute->Payload",
                    "execute->ResultValue",
                    "execute->Payload",
                    "current->Payload",
                    "Service->Payload",
                    "execute->ResultValue",
                    "execute->Payload",
                ],
            ),
            (
                ProgrammingLanguage::Go,
                "types.go",
                include_str!("../../../../tests/fixtures/type-relations/go/types.go"),
                &[
                    "Execute->Payload",
                    "Execute->ResultValue",
                    "Current->Payload",
                    "Execute->Payload",
                    "Execute->ResultValue",
                ],
            ),
            (
                ProgrammingLanguage::Rust,
                "lib.rs",
                include_str!("../../../../tests/fixtures/type-relations/rust/src/lib.rs"),
                &[
                    "execute->Payload",
                    "execute->ResultValue",
                    "current->Payload",
                    "execute->Payload",
                    "execute->ResultValue",
                    "Store->Contract",
                    "value->T",
                    "value->T",
                ],
            ),
            (
                ProgrammingLanguage::Dart,
                "types.dart",
                include_str!("../../../../tests/fixtures/type-relations/dart/lib/types.dart"),
                &[
                    "execute->ResultValue",
                    "execute->Payload",
                    "execute->ResultValue",
                    "execute->Payload",
                    "current->Payload",
                    "execute->ResultValue",
                    "execute->Payload",
                ],
            ),
        ];
        for (language, path, source, expected) in cases {
            assert_eq!(type_uses(*language, path, source), *expected);
        }
    }
}
