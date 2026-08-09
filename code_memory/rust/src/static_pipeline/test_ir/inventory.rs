//! Exact syntax inventory for executable test cases.
//!
//! A test case is accepted only when the language's runner contract is visible
//! in source: an imported test registration API, a framework annotation, a
//! compiler/tool naming rule, or an explicit C/C++ registration macro. File
//! proximity and a production symbol with a similar name are never enough.

use crate::static_pipeline::language_ir::syntax::{node_text, utf8_range};
use codebase_fact_model::analysis::ProgrammingLanguage;
use codebase_fact_model::source::SourceFileKind;
use std::collections::{BTreeMap, BTreeSet};
use tree_sitter::Node;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SyntaxTestCase {
    pub(super) framework: String,
    pub(super) native_kind: String,
    pub(super) display_name: String,
    pub(super) marker_range: Vec<i32>,
    pub(super) body_range: Vec<i32>,
}

#[derive(Clone, Debug)]
struct TestBinding {
    framework: String,
    entrypoint: String,
}

pub(super) fn inventory_test_cases(
    language: ProgrammingLanguage,
    path: &str,
    file_kind: SourceFileKind,
    root: Node<'_>,
    source: &str,
) -> Vec<SyntaxTestCase> {
    let mut cases = match language {
        ProgrammingLanguage::TypeScript | ProgrammingLanguage::JavaScript => {
            javascript_cases(root, source)
        }
        ProgrammingLanguage::Python => python_cases(root, source, file_kind),
        ProgrammingLanguage::Java => java_cases(root, source),
        ProgrammingLanguage::CSharp => csharp_cases(root, source),
        ProgrammingLanguage::C => c_registration_cases(root, source),
        ProgrammingLanguage::Cpp => cpp_cases(root, source),
        ProgrammingLanguage::Go => go_cases(root, source, path),
        ProgrammingLanguage::Rust => rust_cases(root, source),
        ProgrammingLanguage::Dart => dart_cases(root, source),
    };
    cases.sort_by(|left, right| {
        (&left.marker_range, &left.body_range, &left.display_name).cmp(&(
            &right.marker_range,
            &right.body_range,
            &right.display_name,
        ))
    });
    cases.dedup_by(|left, right| {
        left.marker_range == right.marker_range && left.body_range == right.body_range
    });
    cases
}

pub(super) fn likely_contains_test_syntax(
    language: ProgrammingLanguage,
    file_kind: SourceFileKind,
    source: &str,
) -> bool {
    if file_kind == SourceFileKind::Test {
        return true;
    }
    match language {
        ProgrammingLanguage::TypeScript | ProgrammingLanguage::JavaScript => [
            "from 'vitest'",
            "from \"vitest\"",
            "@jest/globals",
            "node:test",
            "bun:test",
            "from 'mocha'",
            "from \"mocha\"",
        ]
        .iter()
        .any(|marker| source.contains(marker)),
        ProgrammingLanguage::Python => false,
        ProgrammingLanguage::Java => {
            source.contains("org.junit") || source.contains("org.testng.annotations")
        }
        ProgrammingLanguage::CSharp => {
            source.contains("using Xunit")
                || source.contains("using NUnit.Framework")
                || source.contains("Microsoft.VisualStudio.TestTools.UnitTesting")
        }
        ProgrammingLanguage::C => {
            source.contains("cmocka_unit_test") || source.contains("RUN_TEST")
        }
        ProgrammingLanguage::Cpp => {
            source.contains("gtest/gtest.h")
                || source.contains("catch2/catch")
                || source.contains("cmocka_unit_test")
                || source.contains("RUN_TEST")
        }
        ProgrammingLanguage::Go => false,
        ProgrammingLanguage::Rust => {
            source.contains("#[test]")
                || source.contains("::test]")
                || source.contains("#[test_case")
        }
        ProgrammingLanguage::Dart => {
            source.contains("package:test/test.dart")
                || source.contains("package:flutter_test/flutter_test.dart")
        }
    }
}

fn javascript_cases(root: Node<'_>, source: &str) -> Vec<SyntaxTestCase> {
    let (direct, namespaces) = javascript_test_bindings(root, source);
    let mut result = Vec::new();
    visit_named(root, &mut |node| {
        if node.kind() != "call_expression" {
            return;
        }
        let Some(function) = node.child_by_field_name("function") else {
            return;
        };
        let callee = node_text(function, source).trim();
        let binding = if let Some(binding) = direct.get(callee) {
            Some(binding.clone())
        } else if let Some((namespace, member)) = callee.rsplit_once('.') {
            namespaces.get(namespace).and_then(|framework| {
                matches!(member, "test" | "it").then(|| TestBinding {
                    framework: framework.clone(),
                    entrypoint: member.to_string(),
                })
            })
        } else {
            None
        };
        let Some(binding) = binding else {
            return;
        };
        let Some(arguments) = node.child_by_field_name("arguments") else {
            return;
        };
        let children = named_children(arguments);
        if children.len() < 2 {
            return;
        }
        let Some(name) = static_string(children[0], source) else {
            return;
        };
        let callback = children[1];
        if !matches!(callback.kind(), "arrow_function" | "function_expression") {
            return;
        }
        let Some(body) = callback.child_by_field_name("body") else {
            return;
        };
        result.push(SyntaxTestCase {
            framework: binding.framework.clone(),
            native_kind: format!("{}:{}", binding.framework, binding.entrypoint),
            display_name: bounded_name(&name),
            marker_range: utf8_range(function),
            body_range: utf8_range(body),
        });
    });
    result
}

fn javascript_test_bindings(
    root: Node<'_>,
    source: &str,
) -> (BTreeMap<String, TestBinding>, BTreeMap<String, String>) {
    let mut direct = BTreeMap::new();
    let mut namespaces = BTreeMap::new();
    visit_named(root, &mut |node| {
        if node.kind() != "import_statement" {
            return;
        }
        let Some(module) = node
            .child_by_field_name("source")
            .and_then(|value| static_string(value, source))
        else {
            return;
        };
        let Some(framework) = js_framework(&module) else {
            return;
        };
        visit_named(node, &mut |child| match child.kind() {
            "import_specifier" => {
                let Some(imported) = child.child_by_field_name("name") else {
                    return;
                };
                let imported = node_text(imported, source).trim();
                if !matches!(imported, "test" | "it") {
                    return;
                }
                let local = child
                    .child_by_field_name("alias")
                    .map(|alias| node_text(alias, source).trim())
                    .unwrap_or(imported);
                direct.insert(
                    local.to_string(),
                    TestBinding {
                        framework: framework.to_string(),
                        entrypoint: imported.to_string(),
                    },
                );
            }
            "namespace_import" => {
                if let Some(identifier) = first_descendant_kind(child, "identifier") {
                    namespaces.insert(
                        node_text(identifier, source).trim().to_string(),
                        framework.to_string(),
                    );
                }
            }
            _ => {}
        });
        // Only node:test defines its default import as the test registration
        // function. Other test frameworks use named bindings.
        if module == "node:test" {
            if let Some(clause) = node.named_child(0) {
                if clause.kind() == "import_clause" {
                    if let Some(identifier) = clause
                        .named_children(&mut clause.walk())
                        .find(|child| child.kind() == "identifier")
                    {
                        let local = node_text(identifier, source).trim();
                        direct.insert(
                            local.to_string(),
                            TestBinding {
                                framework: framework.to_string(),
                                entrypoint: "test".to_string(),
                            },
                        );
                    }
                }
            }
        }
    });
    (direct, namespaces)
}

fn js_framework(module: &str) -> Option<&'static str> {
    match module {
        "vitest" => Some("vitest"),
        "@jest/globals" | "jest" => Some("jest"),
        "node:test" => Some("node-test"),
        "bun:test" => Some("bun-test"),
        "mocha" => Some("mocha"),
        _ => None,
    }
}

fn python_cases(root: Node<'_>, source: &str, file_kind: SourceFileKind) -> Vec<SyntaxTestCase> {
    if file_kind != SourceFileKind::Test {
        return Vec::new();
    }
    let mut result = Vec::new();
    visit_named(root, &mut |node| {
        if node.kind() != "function_definition" || has_function_ancestor(node) {
            return;
        }
        let Some(name_node) = node.child_by_field_name("name") else {
            return;
        };
        let name = node_text(name_node, source).trim();
        if !name.starts_with("test_") {
            return;
        }
        if let Some(class) = ancestor_kind(node, "class_definition") {
            let class_name = class
                .child_by_field_name("name")
                .map(|name| node_text(name, source).trim())
                .unwrap_or_default();
            if !class_name.starts_with("Test") {
                return;
            }
        }
        let Some(body) = node.child_by_field_name("body") else {
            return;
        };
        result.push(SyntaxTestCase {
            framework: "pytest".to_string(),
            native_kind: "pytest:test_function".to_string(),
            display_name: bounded_name(name),
            marker_range: utf8_range(name_node),
            body_range: utf8_range(body),
        });
    });
    result
}

fn java_cases(root: Node<'_>, source: &str) -> Vec<SyntaxTestCase> {
    let imports = java_imports(root, source);
    let mut result = Vec::new();
    visit_named(root, &mut |node| {
        if node.kind() != "method_declaration" {
            return;
        }
        let mut framework = None;
        let mut marker = None;
        visit_named(node, &mut |child| {
            if framework.is_some() || !matches!(child.kind(), "marker_annotation" | "annotation") {
                return;
            }
            let Some(name) = child.child_by_field_name("name") else {
                return;
            };
            if let Some(found) = java_test_annotation(node_text(name, source).trim(), &imports) {
                framework = Some(found.to_string());
                marker = Some(child);
            }
        });
        let (Some(framework), Some(marker)) = (framework, marker) else {
            return;
        };
        let (Some(name), Some(body)) = (
            node.child_by_field_name("name"),
            node.child_by_field_name("body"),
        ) else {
            return;
        };
        result.push(SyntaxTestCase {
            native_kind: format!("{framework}:annotation"),
            framework,
            display_name: bounded_name(node_text(name, source).trim()),
            marker_range: utf8_range(marker),
            body_range: utf8_range(body),
        });
    });
    result
}

fn java_imports(root: Node<'_>, source: &str) -> BTreeSet<String> {
    let mut imports = BTreeSet::new();
    visit_named(root, &mut |node| {
        if node.kind() == "import_declaration" {
            let value = node_text(node, source)
                .trim()
                .trim_start_matches("import")
                .trim()
                .trim_start_matches("static")
                .trim()
                .trim_end_matches(';')
                .trim();
            imports.insert(value.to_string());
        }
    });
    imports
}

fn java_test_annotation(annotation: &str, imports: &BTreeSet<String>) -> Option<&'static str> {
    let short = annotation.rsplit('.').next().unwrap_or(annotation);
    let candidates: &[(&str, &str, &str)] = &[
        ("Test", "org.junit.jupiter.api.Test", "junit-jupiter"),
        (
            "RepeatedTest",
            "org.junit.jupiter.api.RepeatedTest",
            "junit-jupiter",
        ),
        (
            "ParameterizedTest",
            "org.junit.jupiter.params.ParameterizedTest",
            "junit-jupiter",
        ),
        ("Test", "org.junit.Test", "junit4"),
        ("Test", "org.testng.annotations.Test", "testng"),
    ];
    candidates.iter().find_map(|(name, qualified, framework)| {
        if short != *name {
            return None;
        }
        let package = qualified.rsplit_once('.').map(|(package, _)| package)?;
        (annotation == *qualified
            || imports.contains(*qualified)
            || imports.contains(&format!("{package}.*")))
        .then_some(*framework)
    })
}

fn csharp_cases(root: Node<'_>, source: &str) -> Vec<SyntaxTestCase> {
    let usings = csharp_usings(root, source);
    let mut result = Vec::new();
    visit_named(root, &mut |node| {
        if node.kind() != "method_declaration" {
            return;
        }
        let mut framework = None;
        let mut marker = None;
        visit_named(node, &mut |child| {
            if framework.is_some() || child.kind() != "attribute" {
                return;
            }
            let Some(name) = child.child_by_field_name("name") else {
                return;
            };
            if let Some(found) = csharp_test_attribute(node_text(name, source).trim(), &usings) {
                framework = Some(found.to_string());
                marker = Some(child);
            }
        });
        let Some(framework) = framework else {
            return;
        };
        let Some(name) = node.child_by_field_name("name") else {
            return;
        };
        let body = node
            .child_by_field_name("body")
            .or_else(|| node.child_by_field_name("expression_body"));
        let (Some(body), Some(marker)) = (body, marker) else {
            return;
        };
        result.push(SyntaxTestCase {
            native_kind: format!("{framework}:attribute"),
            framework,
            display_name: bounded_name(node_text(name, source).trim()),
            marker_range: utf8_range(marker),
            body_range: utf8_range(body),
        });
    });
    result
}

fn csharp_usings(root: Node<'_>, source: &str) -> BTreeSet<String> {
    let mut usings = BTreeSet::new();
    visit_named(root, &mut |node| {
        if node.kind() == "using_directive" {
            let value = node_text(node, source)
                .trim()
                .trim_start_matches("global")
                .trim()
                .trim_start_matches("using")
                .trim()
                .trim_end_matches(';')
                .trim();
            if !value.contains('=') && !value.starts_with("static ") {
                usings.insert(value.to_string());
            }
        }
    });
    usings
}

fn csharp_test_attribute(attribute: &str, usings: &BTreeSet<String>) -> Option<&'static str> {
    let normalized = attribute.trim_end_matches("Attribute");
    let short = normalized
        .rsplit(['.', ':'])
        .find(|part| !part.is_empty())
        .unwrap_or(normalized);
    let candidates: &[(&[&str], &str, &str)] = &[
        (&["Fact", "Theory"], "Xunit", "xunit"),
        (
            &["Test", "TestCase", "TestCaseSource"],
            "NUnit.Framework",
            "nunit",
        ),
        (
            &["TestMethod", "DataTestMethod"],
            "Microsoft.VisualStudio.TestTools.UnitTesting",
            "mstest",
        ),
    ];
    candidates.iter().find_map(|(names, namespace, framework)| {
        (names.contains(&short)
            && (usings.contains(*namespace) || normalized.starts_with(&format!("{namespace}."))))
        .then_some(*framework)
    })
}

fn c_registration_cases(root: Node<'_>, source: &str) -> Vec<SyntaxTestCase> {
    let includes = includes(root, source);
    let cmocka = includes.iter().any(|value| value.contains("cmocka.h"));
    let unity = includes.iter().any(|value| value.contains("unity.h"));
    if !cmocka && !unity {
        return Vec::new();
    }
    let functions = named_function_definitions(root, source);
    let mut result = Vec::new();
    visit_named(root, &mut |node| {
        if node.kind() != "call_expression" {
            return;
        }
        let Some(callee_node) = node.child_by_field_name("function") else {
            return;
        };
        let callee = node_text(callee_node, source).trim();
        let framework = if cmocka && callee.starts_with("cmocka_unit_test") {
            "cmocka"
        } else if unity && callee == "RUN_TEST" {
            "unity"
        } else {
            return;
        };
        let Some(arguments) = node.child_by_field_name("arguments") else {
            return;
        };
        let Some(target) = arguments.named_child(0) else {
            return;
        };
        let target_name = node_text(target, source).trim();
        let Some((name_range, body_range)) = functions.get(target_name) else {
            return;
        };
        result.push(SyntaxTestCase {
            framework: framework.to_string(),
            native_kind: format!("{framework}:registration"),
            display_name: bounded_name(target_name),
            marker_range: utf8_range(callee_node),
            body_range: body_range.clone(),
        });
        let _ = name_range;
    });
    result
}

fn cpp_cases(root: Node<'_>, source: &str) -> Vec<SyntaxTestCase> {
    let includes = includes(root, source);
    let has_gtest = includes.iter().any(|value| value.contains("gtest/gtest.h"));
    let has_catch = includes.iter().any(|value| value.contains("catch2/catch"));
    let mut result = c_registration_cases(root, source);
    visit_named(root, &mut |node| {
        if node.kind() != "function_definition" {
            return;
        }
        let Some(declarator) = node.child_by_field_name("declarator") else {
            return;
        };
        if declarator.kind() != "function_declarator" {
            return;
        }
        let Some(name_node) = declarator.child_by_field_name("declarator") else {
            return;
        };
        let macro_name = node_text(name_node, source).trim();
        let Some(parameters) = declarator.child_by_field_name("parameters") else {
            return;
        };
        let arguments = parameters
            .named_children(&mut parameters.walk())
            .collect::<Vec<_>>();
        let (framework, display_name) = if has_gtest
            && matches!(macro_name, "TEST" | "TEST_F" | "TEST_P")
            && arguments.len() >= 2
        {
            (
                "google-test",
                format!(
                    "{}.{}",
                    node_text(arguments[0], source).trim(),
                    node_text(arguments[1], source).trim()
                ),
            )
        } else if has_catch
            && matches!(macro_name, "TEST_CASE" | "SCENARIO")
            && !arguments.is_empty()
        {
            let Some(name) = static_string(arguments[0], source) else {
                return;
            };
            ("catch2", name)
        } else {
            return;
        };
        let Some(body) = node.child_by_field_name("body") else {
            return;
        };
        result.push(SyntaxTestCase {
            framework: framework.to_string(),
            native_kind: format!("{framework}:{macro_name}"),
            display_name: bounded_name(&display_name),
            marker_range: utf8_range(name_node),
            body_range: utf8_range(body),
        });
    });
    result
}

fn go_cases(root: Node<'_>, source: &str, path: &str) -> Vec<SyntaxTestCase> {
    if !path.ends_with("_test.go") {
        return Vec::new();
    }
    let mut result = Vec::new();
    visit_named(root, &mut |node| {
        if node.kind() != "function_declaration" {
            return;
        }
        let (Some(name), Some(parameters), Some(body)) = (
            node.child_by_field_name("name"),
            node.child_by_field_name("parameters"),
            node.child_by_field_name("body"),
        ) else {
            return;
        };
        let name_text = node_text(name, source).trim();
        let Some(suffix) = name_text.strip_prefix("Test") else {
            return;
        };
        if suffix.is_empty() || suffix.chars().next().is_some_and(char::is_lowercase) {
            return;
        }
        let parameter_nodes = parameters
            .named_children(&mut parameters.walk())
            .filter(|child| child.kind() == "parameter_declaration")
            .collect::<Vec<_>>();
        if parameter_nodes.len() != 1
            || !node_text(parameter_nodes[0], source).contains("*testing.T")
        {
            return;
        }
        result.push(SyntaxTestCase {
            framework: "go-test".to_string(),
            native_kind: "go-test:function".to_string(),
            display_name: bounded_name(name_text),
            marker_range: utf8_range(name),
            body_range: utf8_range(body),
        });
    });
    result
}

fn rust_cases(root: Node<'_>, source: &str) -> Vec<SyntaxTestCase> {
    let mut result = Vec::new();
    visit_named(root, &mut |node| {
        if node.kind() != "function_item" {
            return;
        }
        let mut sibling = node.prev_named_sibling();
        let mut marker = None;
        let mut framework = None;
        while let Some(candidate) = sibling {
            if candidate.kind() != "attribute_item" {
                break;
            }
            let text = node_text(candidate, source)
                .chars()
                .filter(|character| !character.is_whitespace())
                .collect::<String>();
            let found = match text.as_str() {
                "#[test]" => Some("rust-test"),
                value if value.starts_with("#[tokio::test") => Some("tokio-test"),
                value if value.starts_with("#[async_std::test") => Some("async-std-test"),
                value if value.starts_with("#[actix_rt::test") => Some("actix-test"),
                value if value.starts_with("#[test_case") => Some("test-case"),
                _ => None,
            };
            if let Some(found) = found {
                marker = Some(candidate);
                framework = Some(found.to_string());
                break;
            }
            sibling = candidate.prev_named_sibling();
        }
        let (Some(marker), Some(framework), Some(name), Some(body)) = (
            marker,
            framework,
            node.child_by_field_name("name"),
            node.child_by_field_name("body"),
        ) else {
            return;
        };
        result.push(SyntaxTestCase {
            native_kind: format!("{framework}:attribute"),
            framework,
            display_name: bounded_name(node_text(name, source).trim()),
            marker_range: utf8_range(marker),
            body_range: utf8_range(body),
        });
    });
    result
}

fn dart_cases(root: Node<'_>, source: &str) -> Vec<SyntaxTestCase> {
    let mut direct = BTreeMap::<String, TestBinding>::new();
    let mut namespaces = BTreeMap::<String, String>::new();
    visit_named(root, &mut |node| {
        if node.kind() != "library_import" {
            return;
        }
        let text = node_text(node, source);
        let framework = if text.contains("package:test/test.dart") {
            "dart-test"
        } else if text.contains("package:flutter_test/flutter_test.dart") {
            "flutter-test"
        } else {
            return;
        };
        let prefix = text
            .split_whitespace()
            .collect::<Vec<_>>()
            .windows(2)
            .find_map(|pair| (pair[0] == "as").then_some(pair[1].trim_end_matches(';')));
        if let Some(prefix) = prefix {
            namespaces.insert(prefix.to_string(), framework.to_string());
        } else {
            for entrypoint in ["test", "testWidgets"] {
                direct.insert(
                    entrypoint.to_string(),
                    TestBinding {
                        framework: framework.to_string(),
                        entrypoint: entrypoint.to_string(),
                    },
                );
            }
        }
    });
    let mut result = Vec::new();
    visit_named(root, &mut |node| {
        if node.kind() != "call_expression" {
            return;
        }
        let Some(function) = node.child_by_field_name("function") else {
            return;
        };
        let callee = node_text(function, source).trim();
        let binding = direct.get(callee).cloned().or_else(|| {
            let (namespace, member) = callee.rsplit_once('.')?;
            let framework = namespaces.get(namespace)?;
            matches!(member, "test" | "testWidgets").then(|| TestBinding {
                framework: framework.clone(),
                entrypoint: member.to_string(),
            })
        });
        let Some(binding) = binding else {
            return;
        };
        let Some(arguments) = node.child_by_field_name("arguments") else {
            return;
        };
        let children = named_children(arguments);
        if children.len() < 2 {
            return;
        }
        let Some(name) = static_string(children[0], source) else {
            return;
        };
        let callback = children[1];
        if callback.kind() != "function_expression" {
            return;
        }
        let Some(body) = first_descendant_kind(callback, "block") else {
            return;
        };
        result.push(SyntaxTestCase {
            native_kind: format!("{}:{}", binding.framework, binding.entrypoint),
            framework: binding.framework,
            display_name: bounded_name(&name),
            marker_range: utf8_range(function),
            body_range: utf8_range(body),
        });
    });
    result
}

fn includes(root: Node<'_>, source: &str) -> BTreeSet<String> {
    let mut result = BTreeSet::new();
    visit_named(root, &mut |node| {
        if node.kind() == "preproc_include" {
            result.insert(node_text(node, source).to_string());
        }
    });
    result
}

fn named_function_definitions(
    root: Node<'_>,
    source: &str,
) -> BTreeMap<String, (Vec<i32>, Vec<i32>)> {
    let mut result = BTreeMap::new();
    visit_named(root, &mut |node| {
        if node.kind() != "function_definition" {
            return;
        }
        let (Some(declarator), Some(body)) = (
            node.child_by_field_name("declarator"),
            node.child_by_field_name("body"),
        ) else {
            return;
        };
        let Some(name) = declarator_identifier(declarator) else {
            return;
        };
        result.insert(
            node_text(name, source).trim().to_string(),
            (utf8_range(name), utf8_range(body)),
        );
    });
    result
}

fn declarator_identifier(mut node: Node<'_>) -> Option<Node<'_>> {
    loop {
        if node.kind() == "identifier" {
            return Some(node);
        }
        node = node
            .child_by_field_name("declarator")
            .or_else(|| first_descendant_kind(node, "identifier"))?;
    }
}

fn static_string(node: Node<'_>, source: &str) -> Option<String> {
    if !matches!(
        node.kind(),
        "string" | "string_literal" | "interpreted_string_literal"
    ) {
        return None;
    }
    let value = node_text(node, source).trim();
    let first = value.chars().next()?;
    let last = value.chars().last()?;
    if value.len() < 2 || first != last || !matches!(first, '\'' | '"' | '`') {
        return None;
    }
    if value.contains("${") || value.contains("#{") {
        return None;
    }
    Some(value[1..value.len() - 1].to_string())
}

fn bounded_name(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.chars().count() <= 400 {
        return trimmed.to_string();
    }
    let mut result = trimmed.chars().take(397).collect::<String>();
    result.push_str("...");
    result
}

fn named_children(node: Node<'_>) -> Vec<Node<'_>> {
    node.named_children(&mut node.walk()).collect()
}

fn first_descendant_kind<'tree>(node: Node<'tree>, kind: &str) -> Option<Node<'tree>> {
    if node.kind() == kind {
        return Some(node);
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if let Some(found) = first_descendant_kind(child, kind) {
            return Some(found);
        }
    }
    None
}

fn ancestor_kind<'tree>(mut node: Node<'tree>, kind: &str) -> Option<Node<'tree>> {
    while let Some(parent) = node.parent() {
        if parent.kind() == kind {
            return Some(parent);
        }
        node = parent;
    }
    None
}

fn has_function_ancestor(mut node: Node<'_>) -> bool {
    while let Some(parent) = node.parent() {
        if parent.kind() == "function_definition" {
            return true;
        }
        node = parent;
    }
    false
}

fn visit_named<'tree>(node: Node<'tree>, visitor: &mut impl FnMut(Node<'tree>)) {
    visitor(node);
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        visit_named(child, visitor);
    }
}
