//! Provider-independent inventory of source declarations needed by the map.
//!
//! A language provider remains the semantic authority for symbol identity.
//! This inventory supplies the independently measured syntax denominator: it
//! answers which explicit, product-relevant declarations exist in a source
//! file and where their names are written. It never resolves a name to a
//! target and never creates a provider symbol.

use codebase_fact_model::analysis::ProviderProtocol;
use codebase_fact_model::fact_graph::{FactNodeKind, Visibility};
use tree_sitter::Node;

use super::definition_metadata::{definition_metadata, DefinitionMetadataInput};
#[cfg(test)]
use super::syntax::parse_tree;
use super::syntax::{
    node_text, node_utf16_range, point_range, range_contains, ranges_equal, utf8_range,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SyntaxDefinition {
    pub(super) kind: FactNodeKind,
    pub(super) name: String,
    pub(super) name_utf8_range: Vec<i32>,
    pub(super) name_utf16_range: Vec<i32>,
    pub(super) declaration_utf8_range: Vec<i32>,
    pub(super) declaration_utf16_range: Vec<i32>,
    pub(super) parent_name_utf8_range: Option<Vec<i32>>,
    pub(super) parent_name_utf16_range: Option<Vec<i32>>,
    pub(super) signature: Option<String>,
    pub(super) visibility: Visibility,
}

impl SyntaxDefinition {
    pub(super) fn name_range(&self, protocol: ProviderProtocol) -> &[i32] {
        match protocol {
            ProviderProtocol::LanguageServerProtocol => &self.name_utf16_range,
            ProviderProtocol::Scip | ProviderProtocol::CompilerApi => &self.name_utf8_range,
        }
    }

    pub(super) fn declaration_range(&self, protocol: ProviderProtocol) -> &[i32] {
        match protocol {
            ProviderProtocol::LanguageServerProtocol => &self.declaration_utf16_range,
            ProviderProtocol::Scip | ProviderProtocol::CompilerApi => &self.declaration_utf8_range,
        }
    }

    pub(super) fn parent_name_range(&self, protocol: ProviderProtocol) -> Option<&[i32]> {
        match protocol {
            ProviderProtocol::LanguageServerProtocol => self.parent_name_utf16_range.as_deref(),
            ProviderProtocol::Scip | ProviderProtocol::CompilerApi => {
                self.parent_name_utf8_range.as_deref()
            }
        }
    }

    pub(super) fn matches_provider_range(&self, range: &[i32], protocol: ProviderProtocol) -> bool {
        let name = self.name_range(protocol);
        ranges_equal(name, range)
            || range_contains(range, name)
            || range_contains(self.declaration_range(protocol), range)
    }
}

#[derive(Clone)]
struct SyntaxOwner {
    name: String,
    kind: FactNodeKind,
    visibility: Visibility,
    member_visibility: Option<Visibility>,
    name_utf8_range: Vec<i32>,
    name_utf16_range: Vec<i32>,
}

#[cfg(test)]
pub(super) fn inventory_definitions(
    language: &str,
    path: &str,
    source: &str,
) -> Result<Vec<SyntaxDefinition>, String> {
    let tree = parse_tree(language, path, source, "definition")?;
    Ok(inventory_definitions_from_root(
        language,
        tree.root_node(),
        source,
    ))
}

pub(super) fn inventory_definitions_from_root(
    language: &str,
    root: Node<'_>,
    source: &str,
) -> Vec<SyntaxDefinition> {
    let mut definitions = Vec::new();
    visit(language, root, source, None, false, &mut definitions);
    definitions.sort_by(|left, right| {
        (
            &left.name_utf8_range,
            left.kind,
            &left.name,
            &left.declaration_utf8_range,
        )
            .cmp(&(
                &right.name_utf8_range,
                right.kind,
                &right.name,
                &right.declaration_utf8_range,
            ))
    });
    definitions.dedup_by(|left, right| {
        left.kind == right.kind
            && left.name == right.name
            && left.name_utf8_range == right.name_utf8_range
            && left.declaration_utf8_range == right.declaration_utf8_range
    });
    definitions
}

fn visit(
    language: &str,
    node: Node<'_>,
    source: &str,
    owner: Option<SyntaxOwner>,
    inside_executable: bool,
    definitions: &mut Vec<SyntaxDefinition>,
) {
    let inherited_owner = if language == "rust" && node.kind() == "impl_item" {
        rust_impl_owner(node, source, definitions).or(owner.clone())
    } else {
        owner.clone()
    };
    let declared_type = type_definition(language, node, source, inside_executable);
    let child_owner = if let Some((kind, name)) = declared_type {
        let visibility = push_definition(
            language,
            definitions,
            kind,
            name,
            node,
            inherited_owner.as_ref(),
            source,
        );
        if is_type_owner_kind(kind) && visibility.is_some() {
            Some(owner_from(
                name,
                kind,
                visibility.unwrap_or(Visibility::Unknown),
                source,
            ))
        } else {
            inherited_owner
        }
    } else {
        inherited_owner
    };

    let callable_owner = if language == "go" && node.kind() == "method_declaration" {
        go_receiver_owner(node, source, definitions).or(child_owner.clone())
    } else {
        child_owner.clone()
    };
    if let Some((kind, name)) = callable_definition(
        language,
        node,
        callable_owner.as_ref(),
        source,
        inside_executable,
    ) {
        push_definition(
            language,
            definitions,
            kind,
            name,
            node,
            callable_owner.as_ref(),
            source,
        );
    }
    collect_fields(
        language,
        node,
        child_owner.as_ref(),
        source,
        inside_executable,
        definitions,
    );

    let child_inside_executable = inside_executable || is_executable_body_node(language, node);
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        visit(
            language,
            child,
            source,
            child_owner.clone(),
            child_inside_executable,
            definitions,
        );
    }
}

fn type_definition<'tree>(
    language: &str,
    node: Node<'tree>,
    source: &str,
    inside_executable: bool,
) -> Option<(FactNodeKind, Node<'tree>)> {
    if inside_executable {
        return None;
    }
    let kind = match (language, node.kind()) {
        ("typescript", "class_declaration" | "abstract_class_declaration")
        | ("javascript", "class_declaration" | "class")
        | ("java", "class_declaration" | "record_declaration")
        | ("csharp", "class_declaration" | "record_declaration")
        | ("cpp", "class_specifier")
        | ("dart", "class_declaration" | "extension_type_declaration") => FactNodeKind::Class,
        ("typescript", "interface_declaration")
        | ("java", "interface_declaration" | "annotation_type_declaration")
        | ("csharp", "interface_declaration") => FactNodeKind::Interface,
        ("go", "type_spec") if has_named_child_kind(node, "interface_type") => {
            FactNodeKind::Interface
        }
        ("rust", "trait_item") => FactNodeKind::Trait,
        ("dart", "mixin_declaration") => FactNodeKind::Trait,
        ("c", "struct_specifier" | "union_specifier")
        | ("cpp", "struct_specifier" | "union_specifier")
        | ("csharp", "struct_declaration") => FactNodeKind::Struct,
        ("go", "type_spec") if has_named_child_kind(node, "struct_type") => FactNodeKind::Struct,
        ("rust", "struct_item" | "union_item") => FactNodeKind::Struct,
        ("typescript", "enum_declaration")
        | ("java", "enum_declaration")
        | ("csharp", "enum_declaration")
        | ("c", "enum_specifier")
        | ("cpp", "enum_specifier")
        | ("rust", "enum_item")
        | ("dart", "enum_declaration") => FactNodeKind::Enum,
        ("typescript", "type_alias_declaration")
        | ("c" | "cpp", "type_definition")
        | ("rust", "type_item")
        | ("dart", "type_alias") => FactNodeKind::TypeAlias,
        ("go", "type_spec") => FactNodeKind::Type,
        ("python", "class_definition") => FactNodeKind::Class,
        _ => return None,
    };
    let name = definition_name_node(language, node, source)?;
    Some((kind, name))
}

fn callable_definition<'tree>(
    language: &str,
    node: Node<'tree>,
    owner: Option<&SyntaxOwner>,
    source: &str,
    inside_executable: bool,
) -> Option<(FactNodeKind, Node<'tree>)> {
    if inside_executable {
        return None;
    }
    let kind = match (language, node.kind()) {
        ("typescript", "function_declaration" | "generator_function_declaration")
        | ("javascript", "function_declaration" | "generator_function_declaration")
        | ("python", "function_definition") => {
            if owner.is_some() {
                FactNodeKind::Method
            } else {
                FactNodeKind::Function
            }
        }
        ("typescript", "method_definition" | "method_signature" | "abstract_method_signature")
        | ("javascript", "method_definition")
        | ("java", "method_declaration")
        | ("csharp", "method_declaration")
        | ("go", "method_declaration" | "method_elem") => FactNodeKind::Method,
        ("typescript", "variable_declarator") | ("javascript", "variable_declarator")
            if owner.is_none() && has_callable_initializer(node) =>
        {
            FactNodeKind::Function
        }
        ("typescript", "public_field_definition") | ("javascript", "field_definition")
            if owner.is_some() && has_callable_initializer(node) =>
        {
            FactNodeKind::Method
        }
        ("java", "constructor_declaration" | "compact_constructor_declaration")
        | ("csharp", "constructor_declaration") => FactNodeKind::Constructor,
        ("c" | "cpp", "function_definition" | "declaration" | "field_declaration")
            if first_descendant_by_kind(node, &["function_declarator"]).is_some() =>
        {
            if owner.is_some() {
                FactNodeKind::Method
            } else {
                FactNodeKind::Function
            }
        }
        ("go", "function_declaration") => FactNodeKind::Function,
        ("rust", "function_item" | "function_signature_item") => {
            if owner.is_some() {
                FactNodeKind::Method
            } else {
                FactNodeKind::Function
            }
        }
        ("dart", "function_declaration") => FactNodeKind::Function,
        ("dart", "declaration")
            if owner.is_some()
                && first_descendant_by_kind(node, &["function_signature"]).is_some() =>
        {
            FactNodeKind::Method
        }
        ("dart", "method_signature")
            if first_descendant_by_kind(
                node,
                &[
                    "constructor_signature",
                    "constant_constructor_signature",
                    "factory_constructor_signature",
                    "redirecting_factory_constructor_signature",
                ],
            )
            .is_some() =>
        {
            return None;
        }
        ("dart", "method_signature")
            if first_descendant_by_kind(node, &["getter_signature"]).is_some() =>
        {
            return None;
        }
        ("dart", "method_signature") => FactNodeKind::Method,
        ("dart", "getter_signature" | "setter_signature") => FactNodeKind::Field,
        (
            "dart",
            "constructor_signature"
            | "constant_constructor_signature"
            | "factory_constructor_signature"
            | "redirecting_factory_constructor_signature",
        ) => FactNodeKind::Constructor,
        _ => return None,
    };
    let name = definition_name_node(language, node, source)?;
    let kind = if owner.is_some()
        && (is_constructor_name(language, name, source)
            || language == "cpp"
                && owner.is_some_and(|owner| owner.name == node_text(name, source)))
    {
        FactNodeKind::Constructor
    } else {
        kind
    };
    Some((kind, name))
}

fn collect_fields(
    language: &str,
    node: Node<'_>,
    owner: Option<&SyntaxOwner>,
    source: &str,
    inside_executable: bool,
    definitions: &mut Vec<SyntaxDefinition>,
) {
    if inside_executable || has_callable_initializer(node) {
        return;
    }
    let Some(owner) = owner else {
        return;
    };
    match (language, node.kind()) {
        ("typescript", "public_field_definition" | "property_signature")
        | ("javascript", "field_definition")
        | ("java", "field_declaration")
        | ("csharp", "field_declaration" | "property_declaration")
        | ("c" | "cpp", "field_declaration")
            if first_descendant_by_kind(node, &["function_declarator"]).is_none() =>
        {
            for name in field_name_nodes(language, node) {
                push_definition(
                    language,
                    definitions,
                    FactNodeKind::Field,
                    name,
                    node,
                    Some(owner),
                    source,
                );
            }
        }
        ("typescript", "public_field_definition" | "property_signature")
        | ("javascript", "field_definition")
        | ("java", "field_declaration")
        | ("csharp", "field_declaration" | "property_declaration")
        | ("go", "field_declaration")
        | ("rust", "field_declaration") => {
            for name in field_name_nodes(language, node) {
                push_definition(
                    language,
                    definitions,
                    FactNodeKind::Field,
                    name,
                    node,
                    Some(owner),
                    source,
                );
            }
        }
        ("python", "assignment") if direct_python_class_assignment(node) => {
            if let Some(name) = node
                .child_by_field_name("left")
                .filter(|name| matches!(name.kind(), "identifier" | "pattern_list"))
            {
                if name.kind() == "identifier" {
                    push_definition(
                        language,
                        definitions,
                        FactNodeKind::Field,
                        name,
                        node,
                        Some(owner),
                        source,
                    );
                }
            }
        }
        ("dart", "initialized_identifier" | "static_final_declaration")
            if dart_class_member(node) =>
        {
            if let Some(name) = node
                .child_by_field_name("name")
                .or_else(|| first_direct_named_child(node, &["identifier"]))
            {
                push_definition(
                    language,
                    definitions,
                    FactNodeKind::Field,
                    name,
                    node,
                    Some(owner),
                    source,
                );
            }
        }
        ("typescript", "required_parameter" | "optional_parameter")
            if typescript_parameter_property(node, source) =>
        {
            if let Some(name) = parameter_name(node) {
                push_definition(
                    language,
                    definitions,
                    FactNodeKind::Field,
                    name,
                    node,
                    Some(owner),
                    source,
                );
            }
        }
        _ => {}
    }
}

fn has_callable_initializer(node: Node<'_>) -> bool {
    node.child_by_field_name("value")
        .or_else(|| node.child_by_field_name("initializer"))
        .is_some_and(|value| {
            matches!(
                value.kind(),
                "arrow_function" | "function_expression" | "generator_function"
            )
        })
}

fn is_executable_body_node(language: &str, node: Node<'_>) -> bool {
    match language {
        "typescript" | "javascript" => node.kind() == "statement_block",
        "python" => {
            node.kind() == "block"
                && node
                    .parent()
                    .is_none_or(|parent| parent.kind() != "class_definition")
        }
        "java" | "csharp" | "go" | "rust" => node.kind() == "block",
        "c" | "cpp" => node.kind() == "compound_statement",
        "dart" => node.kind() == "function_body",
        _ => false,
    }
}

fn definition_name_node<'tree>(
    language: &str,
    node: Node<'tree>,
    source: &str,
) -> Option<Node<'tree>> {
    if let Some(name) = node.child_by_field_name("name") {
        return identifier_leaf(name);
    }
    match (language, node.kind()) {
        ("c" | "cpp", "function_definition" | "declaration" | "field_declaration") => node
            .child_by_field_name("declarator")
            .and_then(declarator_name)
            .or_else(|| {
                first_descendant_by_kind(node, &["function_declarator"]).and_then(declarator_name)
            }),
        ("c" | "cpp", "type_definition") => node
            .child_by_field_name("declarator")
            .and_then(declarator_name)
            .or_else(|| first_descendant_by_kind(node, &["type_identifier"])),
        ("go", "type_spec") => node.child_by_field_name("name").and_then(identifier_leaf),
        (
            "rust",
            "function_item"
            | "function_signature_item"
            | "trait_item"
            | "struct_item"
            | "enum_item"
            | "union_item"
            | "type_item",
        ) => node.child_by_field_name("name").and_then(identifier_leaf),
        ("dart", "function_declaration") => node
            .child_by_field_name("signature")
            .and_then(|signature| signature.child_by_field_name("name"))
            .and_then(identifier_leaf),
        ("dart", "declaration") => first_descendant_by_kind(node, &["function_signature"])
            .and_then(|signature| signature.child_by_field_name("name"))
            .and_then(identifier_leaf),
        ("dart", "method_signature") => first_descendant_by_kind(
            node,
            &[
                "function_signature",
                "getter_signature",
                "setter_signature",
                "operator_signature",
            ],
        )
        .and_then(|signature| {
            signature
                .child_by_field_name("name")
                .and_then(identifier_leaf)
        }),
        ("dart", _) => node
            .child_by_field_name("name")
            .and_then(identifier_leaf)
            .or_else(|| first_descendant_by_kind(node, &["identifier", "type_identifier"])),
        _ => first_direct_named_child(
            node,
            &[
                "identifier",
                "type_identifier",
                "field_identifier",
                "property_identifier",
            ],
        ),
    }
    .filter(|name| !node_text(*name, source).trim().is_empty())
}

fn field_name_nodes<'tree>(language: &str, node: Node<'tree>) -> Vec<Node<'tree>> {
    match language {
        "java" | "csharp" => descendants_by_kind(node, &["variable_declarator"])
            .into_iter()
            .filter_map(|declarator| {
                declarator
                    .child_by_field_name("name")
                    .and_then(identifier_leaf)
                    .or_else(|| first_direct_named_child(declarator, &["identifier"]))
            })
            .collect(),
        "c" | "cpp" => descendants_by_kind(node, &["field_identifier"]),
        "go" => {
            if let Some(name) = node.child_by_field_name("name") {
                vec![name]
            } else {
                descendants_by_kind(node, &["field_identifier"])
            }
        }
        "rust" => node
            .child_by_field_name("name")
            .into_iter()
            .chain(first_direct_named_child(node, &["field_identifier"]))
            .collect(),
        _ => node
            .child_by_field_name("name")
            .and_then(identifier_leaf)
            .into_iter()
            .collect(),
    }
}

fn declarator_name(node: Node<'_>) -> Option<Node<'_>> {
    if matches!(
        node.kind(),
        "identifier" | "field_identifier" | "type_identifier" | "operator_name"
    ) {
        return Some(node);
    }
    for field in ["declarator", "name"] {
        if let Some(child) = node.child_by_field_name(field) {
            if let Some(name) = declarator_name(child) {
                return Some(name);
            }
        }
    }
    let mut cursor = node.walk();
    let found = node.named_children(&mut cursor).find_map(declarator_name);
    found
}

fn identifier_leaf(node: Node<'_>) -> Option<Node<'_>> {
    if matches!(
        node.kind(),
        "identifier"
            | "type_identifier"
            | "field_identifier"
            | "property_identifier"
            | "namespace_identifier"
    ) {
        return Some(node);
    }
    for field in ["name", "declarator"] {
        if let Some(child) = node.child_by_field_name(field) {
            if let Some(name) = identifier_leaf(child) {
                return Some(name);
            }
        }
    }
    first_descendant_by_kind(
        node,
        &[
            "identifier",
            "type_identifier",
            "field_identifier",
            "property_identifier",
        ],
    )
}

fn push_definition(
    language: &str,
    definitions: &mut Vec<SyntaxDefinition>,
    kind: FactNodeKind,
    name: Node<'_>,
    declaration: Node<'_>,
    parent: Option<&SyntaxOwner>,
    source: &str,
) -> Option<Visibility> {
    let text = node_text(name, source).trim();
    if text.is_empty() || !is_definition_name(text) {
        return None;
    }
    let metadata = definition_metadata(DefinitionMetadataInput {
        language,
        kind,
        declaration,
        name,
        owner_kind: parent.map(|owner| owner.kind),
        owner_visibility: parent.map(|owner| owner.visibility),
        owner_member_visibility: parent.and_then(|owner| owner.member_visibility),
        source,
    });
    definitions.push(SyntaxDefinition {
        kind,
        name: text.to_string(),
        name_utf8_range: utf8_range(name),
        name_utf16_range: node_utf16_range(source, name),
        declaration_utf8_range: point_range(
            declaration.start_position(),
            declaration.end_position(),
        ),
        declaration_utf16_range: node_utf16_range(source, declaration),
        parent_name_utf8_range: parent.map(|owner| owner.name_utf8_range.clone()),
        parent_name_utf16_range: parent.map(|owner| owner.name_utf16_range.clone()),
        signature: metadata.signature,
        visibility: metadata.visibility,
    });
    Some(metadata.visibility)
}

fn owner_from(
    name: Node<'_>,
    kind: FactNodeKind,
    visibility: Visibility,
    source: &str,
) -> SyntaxOwner {
    SyntaxOwner {
        name: node_text(name, source).to_string(),
        kind,
        visibility,
        member_visibility: (kind == FactNodeKind::Trait).then_some(visibility),
        name_utf8_range: utf8_range(name),
        name_utf16_range: node_utf16_range(source, name),
    }
}

fn is_constructor_name(language: &str, name: Node<'_>, source: &str) -> bool {
    let name = node_text(name, source);
    (language == "typescript" || language == "javascript") && name == "constructor"
        || language == "python" && name == "__init__"
}

fn is_type_owner_kind(kind: FactNodeKind) -> bool {
    matches!(
        kind,
        FactNodeKind::Type
            | FactNodeKind::Class
            | FactNodeKind::Interface
            | FactNodeKind::Trait
            | FactNodeKind::Struct
            | FactNodeKind::Enum
    )
}

fn owner_for_name(definitions: &[SyntaxDefinition], name: &str) -> Option<SyntaxOwner> {
    let mut matching = definitions
        .iter()
        .filter(|definition| definition.name == name && is_type_owner_kind(definition.kind));
    let first = matching.next()?;
    matching.next().is_none().then(|| SyntaxOwner {
        name: first.name.clone(),
        kind: first.kind,
        visibility: first.visibility,
        member_visibility: (first.kind == FactNodeKind::Trait).then_some(first.visibility),
        name_utf8_range: first.name_utf8_range.clone(),
        name_utf16_range: first.name_utf16_range.clone(),
    })
}

fn go_receiver_owner(
    node: Node<'_>,
    source: &str,
    definitions: &[SyntaxDefinition],
) -> Option<SyntaxOwner> {
    let receiver = node.child_by_field_name("receiver")?;
    let receiver_type = first_descendant_by_kind(receiver, &["type_identifier"])?;
    owner_for_name(definitions, node_text(receiver_type, source))
}

fn rust_impl_owner(
    node: Node<'_>,
    source: &str,
    definitions: &[SyntaxDefinition],
) -> Option<SyntaxOwner> {
    let target = node.child_by_field_name("type")?;
    let target = first_descendant_by_kind(target, &["type_identifier"])?;
    let mut owner = owner_for_name(definitions, node_text(target, source))?;
    if let Some(trait_name) = node
        .child_by_field_name("trait")
        .and_then(|trait_node| first_descendant_by_kind(trait_node, &["type_identifier"]))
        .and_then(|trait_name| owner_for_name(definitions, node_text(trait_name, source)))
        .filter(|trait_owner| trait_owner.kind == FactNodeKind::Trait)
    {
        owner.member_visibility = Some(trait_name.visibility);
    }
    Some(owner)
}

fn typescript_parameter_property(node: Node<'_>, source: &str) -> bool {
    let text = node_text(node, source);
    ["public", "protected", "private", "readonly"]
        .iter()
        .any(|modifier| text.split_whitespace().any(|token| token == *modifier))
}

fn parameter_name(node: Node<'_>) -> Option<Node<'_>> {
    node.child_by_field_name("pattern")
        .or_else(|| node.child_by_field_name("name"))
        .and_then(identifier_leaf)
        .or_else(|| first_descendant_by_kind(node, &["identifier"]))
}

fn direct_python_class_assignment(node: Node<'_>) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    let Some(grandparent) = parent.parent() else {
        return false;
    };
    parent.kind() == "expression_statement" && grandparent.kind() == "block"
}

fn dart_class_member(node: Node<'_>) -> bool {
    node.ancestors()
        .take_while(|ancestor| {
            !matches!(
                ancestor.kind(),
                "function_body" | "block" | "function_declaration"
            )
        })
        .any(|ancestor| ancestor.kind() == "class_body")
}

trait NodeAncestors<'tree> {
    fn ancestors(self) -> Ancestors<'tree>;
}

impl<'tree> NodeAncestors<'tree> for Node<'tree> {
    fn ancestors(self) -> Ancestors<'tree> {
        Ancestors {
            current: Some(self),
        }
    }
}

struct Ancestors<'tree> {
    current: Option<Node<'tree>>,
}

impl<'tree> Iterator for Ancestors<'tree> {
    type Item = Node<'tree>;

    fn next(&mut self) -> Option<Self::Item> {
        let current = self.current?;
        self.current = current.parent();
        Some(current)
    }
}

fn has_named_child_kind(node: Node<'_>, kind: &str) -> bool {
    let mut cursor = node.walk();
    let found = node
        .named_children(&mut cursor)
        .any(|child| child.kind() == kind);
    found
}

fn first_direct_named_child<'tree>(node: Node<'tree>, kinds: &[&str]) -> Option<Node<'tree>> {
    let mut cursor = node.walk();
    let found = node
        .named_children(&mut cursor)
        .find(|child| kinds.contains(&child.kind()));
    found
}

fn first_descendant_by_kind<'tree>(node: Node<'tree>, kinds: &[&str]) -> Option<Node<'tree>> {
    if kinds.contains(&node.kind()) {
        return Some(node);
    }
    let mut cursor = node.walk();
    let found = node
        .named_children(&mut cursor)
        .find_map(|child| first_descendant_by_kind(child, kinds));
    found
}

fn descendants_by_kind<'tree>(node: Node<'tree>, kinds: &[&str]) -> Vec<Node<'tree>> {
    let mut found = Vec::new();
    collect_descendants(node, kinds, &mut found);
    found
}

fn collect_descendants<'tree>(node: Node<'tree>, kinds: &[&str], found: &mut Vec<Node<'tree>>) {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if kinds.contains(&child.kind()) {
            found.push(child);
        } else {
            collect_descendants(child, kinds, found);
        }
    }
}

fn is_definition_name(value: &str) -> bool {
    let mut characters = value.chars();
    characters
        .next()
        .is_some_and(|character| character == '_' || character.is_alphabetic())
        && characters.all(|character| character == '_' || character.is_alphanumeric())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metadata(
        language: &str,
        path: &str,
        source: &str,
        kind: FactNodeKind,
        name: &str,
    ) -> (Visibility, Option<String>) {
        let matches = inventory_definitions(language, path, source)
            .unwrap()
            .into_iter()
            .filter(|definition| definition.kind == kind && definition.name == name)
            .collect::<Vec<_>>();
        assert_eq!(matches.len(), 1, "{language}:{kind:?}:{name}");
        let definition = &matches[0];
        (definition.visibility, definition.signature.clone())
    }

    fn owned_metadata(
        language: &str,
        path: &str,
        source: &str,
        kind: FactNodeKind,
        name: &str,
        owner: &str,
    ) -> (Visibility, Option<String>) {
        let definitions = inventory_definitions(language, path, source).unwrap();
        let matches = definitions
            .iter()
            .filter(|definition| definition.kind == kind && definition.name == name)
            .filter(|definition| {
                definition
                    .parent_name_utf8_range
                    .as_ref()
                    .and_then(|range| {
                        definitions
                            .iter()
                            .find(|candidate| &candidate.name_utf8_range == range)
                    })
                    .is_some_and(|candidate| candidate.name == owner)
            })
            .collect::<Vec<_>>();
        assert_eq!(matches.len(), 1, "{language}:{kind:?}:{owner}.{name}");
        (matches[0].visibility, matches[0].signature.clone())
    }

    fn triples(language: &str, path: &str, source: &str) -> Vec<String> {
        let definitions = inventory_definitions(language, path, source).unwrap();
        definitions
            .iter()
            .map(|definition| {
                let parent = definition
                    .parent_name_utf8_range
                    .as_ref()
                    .and_then(|range| {
                        definitions
                            .iter()
                            .find(|candidate| &candidate.name_utf8_range == range)
                    })
                    .map(|candidate| candidate.name.as_str())
                    .unwrap_or("-");
                format!("{}:{}:{parent}", definition.kind.as_str(), definition.name)
            })
            .collect()
    }

    #[test]
    fn typescript_parameter_properties_are_explicit_fields() {
        let source = "export class Box<T> { constructor(private readonly value: T) {} get(): T { return this.value; } }";
        assert_eq!(
            triples("typescript", "box.ts", source),
            vec![
                "class:Box:-",
                "constructor:constructor:Box",
                "field:value:Box",
                "method:get:Box",
            ]
        );
    }

    #[test]
    fn source_metadata_is_language_defined_and_body_free() {
        let cases = [
            (
                "typescript",
                "main.ts",
                "export function add(a: number, b: number): number { return a + b; }",
                FactNodeKind::Function,
                "add",
                Visibility::Public,
                Some("function add(a: number, b: number): number"),
            ),
            (
                "javascript",
                "main.js",
                "function hidden(value) { return value; }",
                FactNodeKind::Function,
                "hidden",
                Visibility::Internal,
                Some("function hidden(value)"),
            ),
            (
                "python",
                "main.py",
                "def _helper(value: int) -> int:\n    return value\n",
                FactNodeKind::Function,
                "_helper",
                Visibility::Internal,
                Some("def _helper(value: int) -> int"),
            ),
            (
                "java",
                "Service.java",
                "class Service { @Deprecated protected int run(int value) { return value; } }",
                FactNodeKind::Method,
                "run",
                Visibility::Protected,
                Some("protected int run(int value)"),
            ),
            (
                "csharp",
                "Service.cs",
                "internal class Service { protected internal int Run(int value) => value; }",
                FactNodeKind::Method,
                "Run",
                Visibility::Protected,
                Some("protected internal int Run(int value)"),
            ),
            (
                "c",
                "main.c",
                "static int helper(void) { return 1; }",
                FactNodeKind::Function,
                "helper",
                Visibility::Internal,
                Some("static int helper(void)"),
            ),
            (
                "cpp",
                "service.cpp",
                "class Service { public: int run(int value) { return value; } };",
                FactNodeKind::Method,
                "run",
                Visibility::Public,
                Some("int run(int value)"),
            ),
            (
                "go",
                "main.go",
                "package main\nfunc Add(left, right int) int { return left + right }\n",
                FactNodeKind::Function,
                "Add",
                Visibility::Public,
                Some("func Add(left, right int) int"),
            ),
            (
                "rust",
                "lib.rs",
                "pub fn run(value: i32) -> i32 { value }",
                FactNodeKind::Function,
                "run",
                Visibility::Public,
                Some("pub fn run(value: i32) -> i32"),
            ),
            (
                "dart",
                "main.dart",
                "int _hidden(int value) => value;",
                FactNodeKind::Function,
                "_hidden",
                Visibility::Private,
                Some("int _hidden(int value)"),
            ),
        ];
        for (language, path, source, kind, name, visibility, signature) in cases {
            assert_eq!(
                metadata(language, path, source, kind, name),
                (visibility, signature.map(str::to_string)),
                "{language}:{name}"
            );
        }
        assert_eq!(
            owned_metadata(
                "cpp",
                "box.cpp",
                "class Box { public: explicit Box(int value) : value_(value) {} private: int value_; };",
                FactNodeKind::Constructor,
                "Box",
                "Box",
            ),
            (
                Visibility::Public,
                Some("explicit Box(int value)".to_string())
            )
        );
    }

    #[test]
    fn implicit_member_visibility_follows_language_rules() {
        let cases = [
            (
                "typescript",
                "box.ts",
                "class Box { constructor(private value: number) {} get(): number { return this.value; } }",
                FactNodeKind::Field,
                "value",
                Visibility::Private,
            ),
            (
                "python",
                "box.py",
                "class Box:\n    def __secret(self):\n        return 1\n",
                FactNodeKind::Method,
                "__secret",
                Visibility::Private,
            ),
            (
                "java",
                "Api.java",
                "interface Api { int read(); }",
                FactNodeKind::Method,
                "read",
                Visibility::Public,
            ),
            (
                "csharp",
                "Api.cs",
                "interface Api { int Read(); }",
                FactNodeKind::Method,
                "Read",
                Visibility::Public,
            ),
            (
                "cpp",
                "box.cpp",
                "class Box { int hidden(); protected: int inherited(); };",
                FactNodeKind::Method,
                "hidden",
                Visibility::Private,
            ),
            (
                "go",
                "main.go",
                "package main\ntype Box struct { hidden int }\n",
                FactNodeKind::Field,
                "hidden",
                Visibility::Internal,
            ),
            (
                "rust",
                "lib.rs",
                "pub trait Api { fn read(&self) -> i32; }",
                FactNodeKind::Method,
                "read",
                Visibility::Public,
            ),
            (
                "dart",
                "box.dart",
                "class Box { int _value = 1; }",
                FactNodeKind::Field,
                "_value",
                Visibility::Private,
            ),
        ];
        for (language, path, source, kind, name, expected) in cases {
            assert_eq!(
                metadata(language, path, source, kind, name).0,
                expected,
                "{language}:{name}"
            );
        }

        let rust_trait_impl = "pub trait Api { fn read(&self) -> i32; } pub struct Service; impl Api for Service { fn read(&self) -> i32 { 1 } }";
        assert_eq!(
            owned_metadata(
                "rust",
                "lib.rs",
                rust_trait_impl,
                FactNodeKind::Method,
                "read",
                "Service",
            )
            .0,
            Visibility::Public,
            "a Rust trait implementation inherits the local trait surface"
        );
    }

    #[test]
    fn locals_and_parameters_are_not_definitions() {
        let source = "class Box { private int value; int Get(int input) { int local = input; return value; } }";
        let definitions = triples("csharp", "Box.cs", source);
        assert!(definitions.contains(&"class:Box:-".to_string()));
        assert!(definitions.contains(&"field:value:Box".to_string()));
        assert!(definitions.contains(&"method:Get:Box".to_string()));
        assert!(!definitions.iter().any(|value| value.contains("input")));
        assert!(!definitions.iter().any(|value| value.contains("local")));
    }

    #[test]
    fn typescript_callable_bindings_are_functions_or_methods_not_fields() {
        let source = r#"
export const normalize = (value: number): number => value;
export class Handler {
  execute = (value: number): number => value;
  run(): number {
    const local = (): number => 1;
    function nested(): number { return local(); }
    return this.execute(1) + nested();
  }
}
"#;
        assert_eq!(
            triples("typescript", "handlers.ts", source),
            vec![
                "function:normalize:-",
                "class:Handler:-",
                "method:execute:Handler",
                "method:run:Handler",
            ]
        );
    }

    #[test]
    fn definitions_inside_executable_bodies_are_not_product_definitions() {
        let cases = [
            (
                "javascript",
                "scope.js",
                "function outer() { function hidden() {} class Local {} } class Global {}",
                vec!["function:outer:-", "class:Global:-"],
            ),
            (
                "python",
                "scope.py",
                "def outer():\n    def hidden():\n        pass\n    class Local:\n        pass\n\nclass Global:\n    def run(self):\n        return 1\n",
                vec!["function:outer:-", "class:Global:-", "method:run:Global"],
            ),
            (
                "java",
                "Scope.java",
                "class Global { void run() { class Local { void hidden() {} } } }",
                vec!["class:Global:-", "method:run:Global"],
            ),
            (
                "csharp",
                "Scope.cs",
                "class Global { void Run() { int Local(int value) => value; } }",
                vec!["class:Global:-", "method:Run:Global"],
            ),
            (
                "rust",
                "scope.rs",
                "fn outer() { fn hidden() {} struct Local; } struct Global;",
                vec!["function:outer:-", "struct:Global:-"],
            ),
            (
                "dart",
                "scope.dart",
                "int outer() { int hidden() => 1; return hidden(); } class Global { int run() => 1; }",
                vec!["function:outer:-", "class:Global:-", "method:run:Global"],
            ),
        ];
        for (language, path, source, expected) in cases {
            assert_eq!(triples(language, path, source), expected, "{language}");
        }
    }

    #[test]
    fn dart_abstract_method_signature_is_a_method_definition() {
        let source = "class Payload {} class ResultValue {} abstract class Contract { ResultValue execute(Payload input); }";
        assert_eq!(
            triples("dart", "contract.dart", source,),
            vec![
                "class:Payload:-",
                "class:ResultValue:-",
                "class:Contract:-",
                "method:execute:Contract",
            ]
        );
    }

    #[test]
    fn reviewed_fixture_inventory_matches_the_frozen_source_denominator() {
        #[derive(serde::Deserialize)]
        struct TruthContract {
            schema: String,
            projects: Vec<TruthProject>,
        }

        #[derive(serde::Deserialize)]
        struct TruthProject {
            fixture: String,
            languages: Vec<TruthLanguage>,
        }

        #[derive(serde::Deserialize)]
        struct TruthLanguage {
            language: String,
            files: Vec<TruthFile>,
        }

        #[derive(serde::Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct TruthFile {
            path: String,
            definitions: Vec<String>,
            #[serde(default)]
            forbidden_names: Vec<String>,
        }

        let contract_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tests/ground_truth/definitions.v1.json");
        let contract: TruthContract =
            serde_json::from_str(&std::fs::read_to_string(contract_path).unwrap()).unwrap();
        assert_eq!(
            contract.schema,
            "codebase-workspace.definition-ground-truth.v1"
        );

        let fixture_root =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures");
        for project in contract.projects {
            for language in project.languages {
                for file in language.files {
                    let relative = format!("{}/{}", project.fixture, file.path);
                    let source = std::fs::read_to_string(fixture_root.join(&relative)).unwrap();
                    let actual =
                        inventory_definitions(&language.language, &file.path, &source).unwrap();
                    assert_eq!(
                        triples(&language.language, &file.path, &source),
                        file.definitions,
                        "reviewed definition denominator drifted for {}:{relative}",
                        language.language
                    );
                    for forbidden in file.forbidden_names {
                        assert!(
                            !actual.iter().any(|definition| definition.name == forbidden),
                            "forbidden local/parameter/type variable {forbidden} became a definition in {}:{relative}",
                            language.language
                        );
                    }
                }
            }
        }
    }
}
