use codebase_fact_model::analysis::ProgrammingLanguage;
use tree_sitter::Node;

use super::{ImportForm, ImportRelation, ImportSite};
#[cfg(test)]
use crate::static_pipeline::language_ir::syntax::parse_tree;
use crate::static_pipeline::language_ir::syntax::{node_text, node_utf16_range, utf8_range};

#[cfg(test)]
pub(in crate::static_pipeline::language_ir) fn inventory_imports(
    language: ProgrammingLanguage,
    path: &str,
    source: &str,
) -> Result<Vec<ImportSite>, String> {
    let tree = parse_tree(language.as_str(), path, source, "import")?;
    Ok(inventory_imports_from_root(
        language,
        tree.root_node(),
        source,
    ))
}

pub(in crate::static_pipeline::language_ir) fn inventory_imports_from_root(
    language: ProgrammingLanguage,
    root: Node<'_>,
    source: &str,
) -> Vec<ImportSite> {
    let mut sites = Vec::new();
    visit(language, root, source, &mut sites);
    sites.sort_by(|left, right| {
        (&left.utf8_range, left.relation, &left.form, &left.specifier).cmp(&(
            &right.utf8_range,
            right.relation,
            &right.form,
            &right.specifier,
        ))
    });
    sites.dedup();
    sites
}

fn visit(language: ProgrammingLanguage, node: Node<'_>, source: &str, sites: &mut Vec<ImportSite>) {
    match language {
        ProgrammingLanguage::TypeScript | ProgrammingLanguage::JavaScript => {
            visit_ecmascript(language, node, source, sites)
        }
        ProgrammingLanguage::Python => visit_python(language, node, source, sites),
        ProgrammingLanguage::Java => visit_java(language, node, source, sites),
        ProgrammingLanguage::CSharp => visit_csharp(language, node, source, sites),
        ProgrammingLanguage::C | ProgrammingLanguage::Cpp => {
            visit_c_family(language, node, source, sites)
        }
        ProgrammingLanguage::Go => visit_go(language, node, source, sites),
        ProgrammingLanguage::Rust => visit_rust(language, node, source, sites),
        ProgrammingLanguage::Dart => visit_dart(language, node, source, sites),
    }

    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        visit(language, child, source, sites);
    }
}

fn visit_ecmascript(
    language: ProgrammingLanguage,
    node: Node<'_>,
    source: &str,
    sites: &mut Vec<ImportSite>,
) {
    match node.kind() {
        "import_statement" => {
            let source_node = node.child_by_field_name("source").or_else(|| {
                direct_named_child(node, "import_require_clause")
                    .and_then(|clause| clause.child_by_field_name("source"))
            });
            if let Some(source_node) = source_node {
                push_literal_site(
                    sites,
                    language,
                    ImportRelation::Imports,
                    ImportForm::EcmaScriptModule,
                    source_node,
                    node,
                    source,
                );
            }
        }
        "export_statement" => {
            if let Some(source_node) = node.child_by_field_name("source") {
                push_literal_site(
                    sites,
                    language,
                    ImportRelation::Exports,
                    ImportForm::EcmaScriptModule,
                    source_node,
                    node,
                    source,
                );
            }
        }
        "call_expression" => {
            let Some(function) = node.child_by_field_name("function") else {
                return;
            };
            let (form, accepted) = match (function.kind(), node_text(function, source).trim()) {
                ("import", _) => (ImportForm::EcmaScriptDynamic, true),
                (_, "require") => (ImportForm::EcmaScriptRequire, true),
                _ => (ImportForm::EcmaScriptDynamic, false),
            };
            if !accepted {
                return;
            }
            let Some(arguments) = node.child_by_field_name("arguments") else {
                return;
            };
            let Some(literal) = first_direct_named_child(arguments, &["string"]) else {
                return;
            };
            push_literal_site(
                sites,
                language,
                ImportRelation::Imports,
                form,
                literal,
                node,
                source,
            );
        }
        _ => {}
    }
}

fn visit_python(
    language: ProgrammingLanguage,
    node: Node<'_>,
    source: &str,
    sites: &mut Vec<ImportSite>,
) {
    match node.kind() {
        "import_statement" => {
            let mut cursor = node.walk();
            for imported in node.children_by_field_name("name", &mut cursor) {
                let name = if imported.kind() == "aliased_import" {
                    imported.child_by_field_name("name").unwrap_or(imported)
                } else {
                    imported
                };
                push_raw_site(
                    sites,
                    language,
                    ImportRelation::Imports,
                    ImportForm::PythonModule,
                    node_text(name, source),
                    node,
                    source,
                );
            }
        }
        "import_from_statement" => {
            if let Some(module) = node.child_by_field_name("module_name") {
                push_raw_site(
                    sites,
                    language,
                    ImportRelation::Imports,
                    ImportForm::PythonModule,
                    node_text(module, source),
                    node,
                    source,
                );
            }
        }
        _ => {}
    }
}

fn visit_java(
    language: ProgrammingLanguage,
    node: Node<'_>,
    source: &str,
    sites: &mut Vec<ImportSite>,
) {
    if node.kind() != "import_declaration" {
        return;
    }
    let mut body = node_text(node, source).trim();
    body = body.strip_prefix("import").map(str::trim).unwrap_or(body);
    let static_import = body
        .strip_prefix("static")
        .map(|value| {
            body = value.trim();
            true
        })
        .unwrap_or(false);
    body = body.strip_suffix(';').map(str::trim).unwrap_or(body);
    let wildcard = body.ends_with(".*");
    let specifier = body.strip_suffix(".*").unwrap_or(body);
    push_raw_site(
        sites,
        language,
        ImportRelation::Imports,
        ImportForm::Java {
            static_import,
            wildcard,
        },
        specifier,
        node,
        source,
    );
}

fn visit_csharp(
    language: ProgrammingLanguage,
    node: Node<'_>,
    source: &str,
    sites: &mut Vec<ImportSite>,
) {
    if node.kind() != "using_directive" {
        return;
    }
    let mut body = node_text(node, source).trim();
    body = body.strip_prefix("global").map(str::trim).unwrap_or(body);
    body = body.strip_prefix("using").map(str::trim).unwrap_or(body);
    body = body.strip_suffix(';').map(str::trim).unwrap_or(body);
    let static_import = body
        .strip_prefix("static")
        .map(|value| {
            body = value.trim();
            true
        })
        .unwrap_or(false);
    let (alias, specifier) = body
        .split_once('=')
        .map(|(_, target)| (true, target.trim()))
        .unwrap_or((false, body));
    push_raw_site(
        sites,
        language,
        ImportRelation::Imports,
        ImportForm::CSharp {
            static_import,
            alias,
        },
        specifier,
        node,
        source,
    );
}

fn visit_c_family(
    language: ProgrammingLanguage,
    node: Node<'_>,
    source: &str,
    sites: &mut Vec<ImportSite>,
) {
    if node.kind() != "preproc_include" {
        return;
    }
    let Some(path) = node.child_by_field_name("path") else {
        return;
    };
    let (system, literal) = match path.kind() {
        "string_literal" => (false, true),
        "system_lib_string" => (true, true),
        _ => (false, false),
    };
    let raw = node_text(path, source).trim();
    let specifier = if literal { strip_delimiters(raw) } else { raw };
    push_raw_site(
        sites,
        language,
        ImportRelation::Imports,
        ImportForm::CInclude { system, literal },
        specifier,
        node,
        source,
    );
}

fn visit_go(
    language: ProgrammingLanguage,
    node: Node<'_>,
    source: &str,
    sites: &mut Vec<ImportSite>,
) {
    if node.kind() != "import_spec" {
        return;
    }
    if let Some(path) = node.child_by_field_name("path") {
        push_literal_site(
            sites,
            language,
            ImportRelation::Imports,
            ImportForm::GoPackage,
            path,
            node,
            source,
        );
    }
}

fn visit_rust(
    language: ProgrammingLanguage,
    node: Node<'_>,
    source: &str,
    sites: &mut Vec<ImportSite>,
) {
    match node.kind() {
        "use_declaration" => {
            if let Some(argument) = node.child_by_field_name("argument") {
                push_raw_site(
                    sites,
                    language,
                    ImportRelation::Imports,
                    ImportForm::RustUse,
                    node_text(argument, source),
                    node,
                    source,
                );
            }
        }
        "extern_crate_declaration" => {
            if let Some(name) = node.child_by_field_name("name") {
                push_raw_site(
                    sites,
                    language,
                    ImportRelation::Imports,
                    ImportForm::RustExternCrate,
                    node_text(name, source),
                    node,
                    source,
                );
            }
        }
        _ => {}
    }
}

fn visit_dart(
    language: ProgrammingLanguage,
    node: Node<'_>,
    source: &str,
    sites: &mut Vec<ImportSite>,
) {
    let (relation, uri_container) = match node.kind() {
        "library_import" => (
            ImportRelation::Imports,
            direct_named_child(node, "import_specification")
                .and_then(|specification| specification.child_by_field_name("uri")),
        ),
        "library_export" => (ImportRelation::Exports, node.child_by_field_name("uri")),
        _ => return,
    };
    let Some(uri_container) = uri_container else {
        return;
    };
    let mut uris = Vec::new();
    collect_descendants(uri_container, "uri", &mut uris);
    if uri_container.kind() == "uri" {
        uris.insert(0, uri_container);
    }
    let Some(uri) = uris.first().copied() else {
        return;
    };
    let Some(literal) = first_descendant(uri, "string_literal") else {
        return;
    };
    let raw = node_text(literal, source).trim();
    let raw = raw
        .strip_prefix('r')
        .filter(|value| value.starts_with(['\'', '"']))
        .unwrap_or(raw);
    push_raw_site(
        sites,
        language,
        relation,
        ImportForm::DartUri {
            conditional: uris.len() > 1,
        },
        strip_delimiters(raw),
        node,
        source,
    );
}

fn push_literal_site(
    sites: &mut Vec<ImportSite>,
    language: ProgrammingLanguage,
    relation: ImportRelation,
    form: ImportForm,
    literal: Node<'_>,
    evidence_node: Node<'_>,
    source: &str,
) {
    push_raw_site(
        sites,
        language,
        relation,
        form,
        strip_delimiters(node_text(literal, source).trim()),
        evidence_node,
        source,
    );
}

fn push_raw_site(
    sites: &mut Vec<ImportSite>,
    language: ProgrammingLanguage,
    relation: ImportRelation,
    form: ImportForm,
    specifier: &str,
    evidence_node: Node<'_>,
    source: &str,
) {
    let specifier = specifier.trim();
    if specifier.is_empty() {
        return;
    }
    sites.push(ImportSite {
        language,
        relation,
        form,
        specifier: specifier.to_string(),
        utf8_range: utf8_range(evidence_node),
        utf16_range: node_utf16_range(source, evidence_node),
    });
}

fn strip_delimiters(value: &str) -> &str {
    let bytes = value.as_bytes();
    if bytes.len() >= 2 {
        let paired = matches!(
            (bytes[0], bytes[bytes.len() - 1]),
            (b'\'', b'\'') | (b'"', b'"') | (b'`', b'`') | (b'<', b'>')
        );
        if paired {
            return &value[1..value.len() - 1];
        }
    }
    value
}

fn direct_named_child<'tree>(node: Node<'tree>, kind: &str) -> Option<Node<'tree>> {
    let mut cursor = node.walk();
    let found = node
        .named_children(&mut cursor)
        .find(|child| child.kind() == kind);
    found
}

fn first_direct_named_child<'tree>(node: Node<'tree>, kinds: &[&str]) -> Option<Node<'tree>> {
    let mut cursor = node.walk();
    let found = node
        .named_children(&mut cursor)
        .find(|child| kinds.contains(&child.kind()));
    found
}

fn first_descendant<'tree>(node: Node<'tree>, kind: &str) -> Option<Node<'tree>> {
    if node.kind() == kind {
        return Some(node);
    }
    let mut cursor = node.walk();
    let found = node
        .named_children(&mut cursor)
        .find_map(|child| first_descendant(child, kind));
    found
}

fn collect_descendants<'tree>(node: Node<'tree>, kind: &str, found: &mut Vec<Node<'tree>>) {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() == kind {
            found.push(child);
        } else {
            collect_descendants(child, kind, found);
        }
    }
}
