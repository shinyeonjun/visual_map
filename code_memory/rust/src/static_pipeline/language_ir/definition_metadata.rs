//! Minimal source-backed metadata for product-relevant definitions.
//!
//! This module deliberately does not retain annotations, documentation, local
//! variables, bodies, or arbitrary modifiers. It emits only the declaration
//! header needed to distinguish/display callables and the language-defined
//! accessibility needed by the canonical relevance gate.

use codebase_fact_model::fact_graph::{FactNodeKind, Visibility};
use tree_sitter::Node;

const MAX_SIGNATURE_BYTES: usize = 8_192;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct DefinitionMetadata {
    pub(super) signature: Option<String>,
    pub(super) visibility: Visibility,
}

pub(super) struct DefinitionMetadataInput<'tree, 'source> {
    pub(super) language: &'source str,
    pub(super) kind: FactNodeKind,
    pub(super) declaration: Node<'tree>,
    pub(super) name: Node<'tree>,
    pub(super) owner_kind: Option<FactNodeKind>,
    pub(super) owner_visibility: Option<Visibility>,
    pub(super) owner_member_visibility: Option<Visibility>,
    pub(super) source: &'source str,
}

pub(super) fn definition_metadata(input: DefinitionMetadataInput<'_, '_>) -> DefinitionMetadata {
    let DefinitionMetadataInput {
        language,
        kind,
        declaration,
        name,
        owner_kind,
        owner_visibility,
        owner_member_visibility,
        source,
    } = input;
    let name_text = source_text(name, source).trim();
    DefinitionMetadata {
        signature: callable_signature(kind, declaration, source),
        visibility: definition_visibility(
            language,
            declaration,
            name,
            name_text,
            owner_kind,
            owner_visibility,
            owner_member_visibility,
            source,
        ),
    }
}

fn callable_signature(kind: FactNodeKind, declaration: Node<'_>, source: &str) -> Option<String> {
    if !matches!(
        kind,
        FactNodeKind::Function | FactNodeKind::Method | FactNodeKind::Constructor
    ) {
        return None;
    }
    let end = callable_body_start(kind, declaration).unwrap_or_else(|| declaration.end_byte());
    let raw = source.get(declaration.start_byte()..end)?;
    let raw = strip_leading_attributes(raw);
    let mut signature = normalize_whitespace(raw);
    if kind == FactNodeKind::Constructor {
        signature = strip_constructor_initializer(&signature).to_string();
    }
    loop {
        let trimmed = signature.trim_end();
        let next = if let Some(value) = trimmed.strip_suffix("=>") {
            value
        } else if let Some(value) = trimmed.strip_suffix(';') {
            value
        } else if let Some(value) = trimmed.strip_suffix(':') {
            value
        } else if let Some(value) = trimmed.strip_suffix('{') {
            value
        } else {
            break;
        };
        signature = next.trim_end().to_string();
    }
    (!signature.is_empty() && signature.len() <= MAX_SIGNATURE_BYTES).then_some(signature)
}

fn strip_constructor_initializer(value: &str) -> &str {
    let Some(open) = value.find('(') else {
        return value;
    };
    let mut depth = 0_u32;
    let mut close = None;
    for (offset, character) in value[open..].char_indices() {
        if character == '(' {
            depth += 1;
        } else if character == ')' {
            let Some(next) = depth.checked_sub(1) else {
                return value;
            };
            depth = next;
            if depth == 0 {
                close = Some(open + offset + character.len_utf8());
                break;
            }
        }
    }
    let Some(close) = close else {
        return value;
    };
    let suffix = &value[close..];
    for (offset, character) in suffix.char_indices() {
        if character != ':' {
            continue;
        }
        let absolute = close + offset;
        let before = value[..absolute].chars().next_back();
        let after = value[absolute + 1..].chars().next();
        if before != Some(':') && after != Some(':') {
            return value[..absolute].trim_end();
        }
    }
    value
}

fn callable_body_start(kind: FactNodeKind, declaration: Node<'_>) -> Option<usize> {
    if let Some(body) = declaration.child_by_field_name("body") {
        return Some(body.start_byte());
    }
    if let Some(value) = declaration
        .child_by_field_name("value")
        .or_else(|| declaration.child_by_field_name("initializer"))
    {
        if matches!(
            value.kind(),
            "arrow_function" | "function_expression" | "generator_function"
        ) {
            if let Some(body) = value.child_by_field_name("body") {
                return Some(body.start_byte());
            }
        }
    }
    let mut boundary_kinds = vec![
        "arrow_expression_clause",
        "function_body",
        "statement_block",
        "compound_statement",
        "block",
    ];
    if kind == FactNodeKind::Constructor {
        boundary_kinds.extend([
            "field_initializer_list",
            "constructor_initializer",
            "initializers",
        ]);
    }
    first_descendant_start(declaration, &boundary_kinds)
}

fn first_descendant_start(node: Node<'_>, kinds: &[&str]) -> Option<usize> {
    let mut found = None;
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if kinds.contains(&child.kind()) {
            found = Some(found.map_or(child.start_byte(), |current: usize| {
                current.min(child.start_byte())
            }));
            continue;
        }
        if let Some(start) = first_descendant_start(child, kinds) {
            found = Some(found.map_or(start, |current| current.min(start)));
        }
    }
    found
}

#[allow(clippy::too_many_arguments)]
fn definition_visibility(
    language: &str,
    declaration: Node<'_>,
    name: Node<'_>,
    name_text: &str,
    owner_kind: Option<FactNodeKind>,
    owner_visibility: Option<Visibility>,
    owner_member_visibility: Option<Visibility>,
    source: &str,
) -> Visibility {
    let prefix = source
        .get(declaration.start_byte()..name.start_byte())
        .unwrap_or_default();
    let explicitly_exported = has_export_ancestor(declaration, source);
    match language {
        "typescript" => typescript_visibility(prefix, name_text, owner_kind, explicitly_exported),
        "javascript" => javascript_visibility(prefix, name_text, owner_kind, explicitly_exported),
        "python" => python_visibility(name_text, owner_kind),
        "java" => java_visibility(prefix, owner_kind),
        "csharp" => csharp_visibility(prefix, owner_kind),
        "c" => c_visibility(prefix),
        "cpp" => cpp_visibility(declaration, prefix, owner_kind, source),
        "go" => go_visibility(name_text),
        "rust" => rust_visibility(
            prefix,
            owner_kind,
            owner_visibility,
            owner_member_visibility,
        ),
        "dart" => dart_visibility(name_text),
        _ => Visibility::Unknown,
    }
}

fn typescript_visibility(
    prefix: &str,
    name: &str,
    owner_kind: Option<FactNodeKind>,
    explicitly_exported: bool,
) -> Visibility {
    if has_word(prefix, "private") || name.starts_with('#') {
        Visibility::Private
    } else if has_word(prefix, "protected") {
        Visibility::Protected
    } else if has_word(prefix, "public")
        || owner_kind.is_some()
        || has_word(prefix, "export")
        || explicitly_exported
    {
        Visibility::Public
    } else {
        Visibility::Internal
    }
}

fn javascript_visibility(
    prefix: &str,
    name: &str,
    owner_kind: Option<FactNodeKind>,
    explicitly_exported: bool,
) -> Visibility {
    if name.starts_with('#') {
        Visibility::Private
    } else if owner_kind.is_some() || has_word(prefix, "export") || explicitly_exported {
        Visibility::Public
    } else {
        // A non-exported top-level binding is lexical to the script/module.
        // CommonJS `module.exports` is represented by a separate export fact;
        // it is not guessed from the declaration alone.
        Visibility::Internal
    }
}

fn has_export_ancestor(node: Node<'_>, source: &str) -> bool {
    let mut child = node;
    let mut current = node.parent();
    while let Some(parent) = current {
        if matches!(parent.kind(), "export_statement" | "export_declaration") {
            let prefix = source
                .get(parent.start_byte()..child.start_byte())
                .unwrap_or_default();
            return has_word(prefix, "export");
        }
        if matches!(
            parent.kind(),
            "program" | "module" | "source_file" | "translation_unit"
        ) {
            break;
        }
        child = parent;
        current = parent.parent();
    }
    false
}

fn python_visibility(name: &str, owner_kind: Option<FactNodeKind>) -> Visibility {
    if owner_kind.is_some() && name.starts_with("__") && !name.ends_with("__") && name.len() > 2 {
        Visibility::Private
    } else if name.starts_with('_') && !is_python_dunder(name) {
        Visibility::Internal
    } else {
        Visibility::Public
    }
}

fn is_python_dunder(name: &str) -> bool {
    name.len() > 4 && name.starts_with("__") && name.ends_with("__")
}

fn java_visibility(prefix: &str, owner_kind: Option<FactNodeKind>) -> Visibility {
    if has_word(prefix, "public") {
        Visibility::Public
    } else if has_word(prefix, "protected") {
        Visibility::Protected
    } else if has_word(prefix, "private") {
        Visibility::Private
    } else if owner_kind == Some(FactNodeKind::Interface) {
        Visibility::Public
    } else {
        Visibility::Package
    }
}

fn csharp_visibility(prefix: &str, owner_kind: Option<FactNodeKind>) -> Visibility {
    if has_word(prefix, "public") {
        Visibility::Public
    } else if has_word(prefix, "protected") {
        // `protected internal` and `private protected` stay coarse-grained as
        // protected. The exact source header remains available as evidence.
        Visibility::Protected
    } else if has_word(prefix, "internal") {
        Visibility::Internal
    } else if has_word(prefix, "private") {
        Visibility::Private
    } else if owner_kind == Some(FactNodeKind::Interface) {
        Visibility::Public
    } else if owner_kind.is_some() {
        Visibility::Private
    } else {
        Visibility::Internal
    }
}

fn c_visibility(prefix: &str) -> Visibility {
    if has_word(prefix, "static") {
        Visibility::Internal
    } else {
        Visibility::Public
    }
}

fn cpp_visibility(
    declaration: Node<'_>,
    prefix: &str,
    owner_kind: Option<FactNodeKind>,
    source: &str,
) -> Visibility {
    if owner_kind.is_some() {
        return cpp_member_visibility(declaration, owner_kind, source);
    }
    if has_word(prefix, "static") || inside_anonymous_namespace(declaration, source) {
        Visibility::Internal
    } else {
        Visibility::Public
    }
}

fn cpp_member_visibility(
    declaration: Node<'_>,
    owner_kind: Option<FactNodeKind>,
    source: &str,
) -> Visibility {
    let mut child = declaration;
    let mut current = declaration.parent();
    while let Some(parent) = current {
        if parent.kind() == "field_declaration_list" {
            let mut access = match owner_kind {
                Some(FactNodeKind::Class) => Visibility::Private,
                _ => Visibility::Public,
            };
            let mut cursor = parent.walk();
            for sibling in parent.named_children(&mut cursor) {
                if sibling.start_byte() >= child.start_byte() {
                    break;
                }
                if sibling.kind() == "access_specifier" {
                    access = access_specifier_visibility(source_text(sibling, source));
                }
            }
            return access;
        }
        child = parent;
        current = parent.parent();
    }
    Visibility::Unknown
}

fn access_specifier_visibility(value: &str) -> Visibility {
    if has_word(value, "public") {
        Visibility::Public
    } else if has_word(value, "protected") {
        Visibility::Protected
    } else if has_word(value, "private") {
        Visibility::Private
    } else {
        Visibility::Unknown
    }
}

fn inside_anonymous_namespace(node: Node<'_>, source: &str) -> bool {
    let mut current = node.parent();
    while let Some(parent) = current {
        if parent.kind() == "namespace_definition" {
            let has_name = parent
                .child_by_field_name("name")
                .is_some_and(|name| !source_text(name, source).trim().is_empty());
            if !has_name {
                return true;
            }
        }
        current = parent.parent();
    }
    false
}

fn go_visibility(name: &str) -> Visibility {
    if name.chars().next().is_some_and(char::is_uppercase) {
        Visibility::Public
    } else {
        Visibility::Internal
    }
}

fn rust_visibility(
    prefix: &str,
    owner_kind: Option<FactNodeKind>,
    owner_visibility: Option<Visibility>,
    owner_member_visibility: Option<Visibility>,
) -> Visibility {
    if has_word(prefix, "pub") {
        let compact = prefix
            .chars()
            .filter(|value| !value.is_whitespace())
            .collect::<String>();
        if compact.contains("pub(") {
            Visibility::Internal
        } else {
            Visibility::Public
        }
    } else if owner_member_visibility.is_some() || owner_kind == Some(FactNodeKind::Trait) {
        owner_member_visibility
            .or(owner_visibility)
            .unwrap_or(Visibility::Unknown)
    } else {
        Visibility::Private
    }
}

fn dart_visibility(name: &str) -> Visibility {
    if name.starts_with('_') {
        Visibility::Private
    } else {
        Visibility::Public
    }
}

fn has_word(value: &str, expected: &str) -> bool {
    value
        .split(|character: char| !(character == '_' || character.is_alphanumeric()))
        .any(|token| token == expected)
}

fn source_text<'a>(node: Node<'_>, source: &'a str) -> &'a str {
    source.get(node.byte_range()).unwrap_or_default()
}

fn normalize_whitespace(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn strip_leading_attributes(mut value: &str) -> &str {
    loop {
        value = value.trim_start();
        if let Some(rest) = strip_balanced_prefix(value, "#[", '[', ']') {
            value = rest;
            continue;
        }
        if let Some(rest) = strip_balanced_prefix(value, "[", '[', ']') {
            value = rest;
            continue;
        }
        if let Some(after_at) = value.strip_prefix('@') {
            let name_end = after_at
                .find(|character: char| {
                    !(character == '_' || character.is_alphanumeric() || character == '.')
                })
                .unwrap_or(after_at.len());
            if name_end == 0 {
                return value;
            }
            let mut rest = &after_at[name_end..];
            rest = rest.trim_start();
            if rest.starts_with('(') {
                let Some(after_args) = strip_balanced_prefix(rest, "(", '(', ')') else {
                    return value;
                };
                rest = after_args;
            }
            value = rest;
            continue;
        }
        return value;
    }
}

fn strip_balanced_prefix<'a>(
    value: &'a str,
    prefix: &str,
    open: char,
    close: char,
) -> Option<&'a str> {
    if !value.starts_with(prefix) {
        return None;
    }
    let mut depth = 0_u32;
    for (index, character) in value.char_indices() {
        if character == open {
            depth += 1;
        } else if character == close {
            depth = depth.checked_sub(1)?;
            if depth == 0 {
                return Some(&value[index + character.len_utf8()..]);
            }
        }
    }
    None
}
