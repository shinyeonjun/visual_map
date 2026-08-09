//! Provider-neutral source call-site inventory.
//!
//! Semantic providers answer *what* a token resolves to. They are not the
//! authority for whether a token is an executable call. This module uses a
//! concrete syntax tree to establish that denominator before provider facts
//! are reconciled.

use codebase_fact_model::analysis::ProviderProtocol;
use tree_sitter::{Language, Node, Parser, Point};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum CallSiteForm {
    Call,
    MethodCall,
    Construct,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SyntaxCallSite {
    pub(crate) form: CallSiteForm,
    /// SCIP/compiler coordinates: zero-based line and UTF-8 byte column.
    pub(crate) callee_utf8_range: Vec<i32>,
    /// LSP coordinates: zero-based line and UTF-16 code-unit column.
    pub(crate) callee_utf16_range: Vec<i32>,
    pub(crate) expression_utf8_range: Vec<i32>,
    pub(crate) expression_utf16_range: Vec<i32>,
    /// Exact source name of the nearest named callable/property that owns the
    /// call. Providers such as scip-dotnet legitimately omit enclosing ranges,
    /// so a brace-only fallback would otherwise attach method calls to the
    /// surrounding class (and break executable flow paths).
    pub(crate) owner_name_utf8_range: Option<Vec<i32>>,
    pub(crate) owner_name_utf16_range: Option<Vec<i32>>,
    pub(crate) callee_text: String,
}

impl SyntaxCallSite {
    pub(crate) fn callee_range(&self, protocol: ProviderProtocol) -> Vec<i32> {
        match protocol {
            ProviderProtocol::LanguageServerProtocol => self.callee_utf16_range.clone(),
            ProviderProtocol::Scip | ProviderProtocol::CompilerApi => {
                self.callee_utf8_range.clone()
            }
        }
    }

    pub(crate) fn expression_range(&self, protocol: ProviderProtocol) -> &[i32] {
        match protocol {
            ProviderProtocol::LanguageServerProtocol => &self.expression_utf16_range,
            ProviderProtocol::Scip | ProviderProtocol::CompilerApi => &self.expression_utf8_range,
        }
    }

    pub(crate) fn matches_provider_range(&self, range: &[i32]) -> bool {
        ranges_equal(range, &self.callee_utf8_range)
            || ranges_equal(range, &self.callee_utf16_range)
    }

    pub(crate) fn expression_contains_provider_range(&self, range: &[i32]) -> bool {
        range_contains(&self.expression_utf8_range, range)
            || range_contains(&self.expression_utf16_range, range)
    }

    pub(crate) fn owner_name_range(&self, protocol: ProviderProtocol) -> Option<&[i32]> {
        match protocol {
            ProviderProtocol::LanguageServerProtocol => self.owner_name_utf16_range.as_deref(),
            ProviderProtocol::Scip | ProviderProtocol::CompilerApi => {
                self.owner_name_utf8_range.as_deref()
            }
        }
    }
}

pub(crate) fn inventory_call_sites(
    language: &str,
    source: &str,
) -> Result<Vec<SyntaxCallSite>, String> {
    let Some(language) = parser_language(language) else {
        return Ok(Vec::new());
    };
    let mut parser = Parser::new();
    parser
        .set_language(&language)
        .map_err(|error| format!("cannot load {language:?} syntax grammar: {error}"))?;
    let tree = parser
        .parse(source, None)
        .ok_or_else(|| "syntax parser returned no tree".to_string())?;
    let mut sites = Vec::new();
    visit(
        language_name(language),
        tree.root_node(),
        source,
        &mut sites,
    );
    sites.sort_by(|left, right| {
        (
            &left.callee_utf8_range,
            &left.expression_utf8_range,
            &left.owner_name_utf8_range,
            form_rank(left.form),
        )
            .cmp(&(
                &right.callee_utf8_range,
                &right.expression_utf8_range,
                &right.owner_name_utf8_range,
                form_rank(right.form),
            ))
    });
    sites.dedup_by(|left, right| {
        left.form == right.form
            && left.callee_utf8_range == right.callee_utf8_range
            && left.expression_utf8_range == right.expression_utf8_range
            && left.owner_name_utf8_range == right.owner_name_utf8_range
    });
    Ok(sites)
}

fn parser_language(language: &str) -> Option<Language> {
    match language {
        "csharp" => Some(tree_sitter_c_sharp::LANGUAGE.into()),
        "java" => Some(tree_sitter_java::LANGUAGE.into()),
        "c" => Some(tree_sitter_c::LANGUAGE.into()),
        "cpp" => Some(tree_sitter_cpp::LANGUAGE.into()),
        "go" => Some(tree_sitter_go::LANGUAGE.into()),
        "rust" => Some(tree_sitter_rust::LANGUAGE.into()),
        _ => None,
    }
}

// Language does not retain the caller's label, and comparing generated
// language handles is not a public tree-sitter contract. Keep this helper
// local to the parser selection order above.
fn language_name(language: Language) -> &'static str {
    if language == tree_sitter_c_sharp::LANGUAGE.into() {
        "csharp"
    } else if language == tree_sitter_java::LANGUAGE.into() {
        "java"
    } else if language == tree_sitter_c::LANGUAGE.into() {
        "c"
    } else if language == tree_sitter_cpp::LANGUAGE.into() {
        "cpp"
    } else if language == tree_sitter_go::LANGUAGE.into() {
        "go"
    } else {
        "rust"
    }
}

fn visit(language: &str, node: Node<'_>, source: &str, sites: &mut Vec<SyntaxCallSite>) {
    match (language, node.kind()) {
        ("csharp", "invocation_expression") => {
            if let Some(function) = node.child_by_field_name("function") {
                push_site(sites, call_form(function), function, node, source);
            }
        }
        ("csharp", "object_creation_expression") => {
            if let Some(target_type) = node.child_by_field_name("type") {
                push_site(sites, CallSiteForm::Construct, target_type, node, source);
            }
        }
        ("csharp", "implicit_object_creation_expression") => {
            if let Some(keyword) = keyword_span(node, source, "new") {
                push_site_from_span(sites, CallSiteForm::Construct, keyword, node, source);
            }
        }
        ("java", "method_invocation") => {
            if let Some(name) = node.child_by_field_name("name") {
                let form = if node.child_by_field_name("object").is_some() {
                    CallSiteForm::MethodCall
                } else {
                    CallSiteForm::Call
                };
                push_site(sites, form, name, node, source);
            }
        }
        ("java", "object_creation_expression") => {
            if let Some(target_type) = node.child_by_field_name("type") {
                push_site(sites, CallSiteForm::Construct, target_type, node, source);
            }
        }
        ("java", "explicit_constructor_invocation") => {
            if let Some(constructor) = node.child_by_field_name("constructor") {
                push_site_from_span(sites, CallSiteForm::Construct, constructor, node, source);
            }
        }
        ("c" | "cpp" | "go" | "rust", "call_expression") => {
            if let Some(function) = node.child_by_field_name("function") {
                push_site(sites, call_form(function), function, node, source);
            }
        }
        ("cpp", "new_expression") => {
            if let Some(target_type) = node.child_by_field_name("type") {
                push_site(sites, CallSiteForm::Construct, target_type, node, source);
            }
        }
        ("cpp", "declaration") => collect_cpp_direct_initializers(node, source, sites),
        _ => {}
    }

    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        visit(language, child, source, sites);
    }
}

fn collect_cpp_direct_initializers(
    declaration: Node<'_>,
    source: &str,
    sites: &mut Vec<SyntaxCallSite>,
) {
    let Some(target_type) = declaration.child_by_field_name("type") else {
        return;
    };
    let mut cursor = declaration.walk();
    for child in declaration.named_children(&mut cursor) {
        if child.kind() != "init_declarator" {
            continue;
        }
        let Some(value) = child.child_by_field_name("value") else {
            continue;
        };
        if matches!(value.kind(), "argument_list" | "initializer_list") {
            push_site(sites, CallSiteForm::Construct, target_type, child, source);
        }
    }
}

fn call_form(function: Node<'_>) -> CallSiteForm {
    if matches!(
        function.kind(),
        "field_expression" | "member_access_expression" | "selector_expression"
    ) {
        CallSiteForm::MethodCall
    } else {
        CallSiteForm::Call
    }
}

fn push_site(
    sites: &mut Vec<SyntaxCallSite>,
    form: CallSiteForm,
    candidate: Node<'_>,
    expression: Node<'_>,
    source: &str,
) {
    let Some(callee) = callee_leaf(candidate) else {
        return;
    };
    push_site_from_span(sites, form, callee, expression, source);
}

fn push_site_from_span(
    sites: &mut Vec<SyntaxCallSite>,
    form: CallSiteForm,
    callee: Node<'_>,
    expression: Node<'_>,
    source: &str,
) {
    let Ok(callee_text) = callee.utf8_text(source.as_bytes()) else {
        return;
    };
    if callee_text.trim().is_empty() {
        return;
    }
    let owner_name = enclosing_named_execution_owner(expression);
    sites.push(SyntaxCallSite {
        form,
        callee_utf8_range: point_range(callee.start_position(), callee.end_position()),
        callee_utf16_range: utf16_range(source, callee.start_position(), callee.end_position()),
        expression_utf8_range: point_range(expression.start_position(), expression.end_position()),
        expression_utf16_range: utf16_range(
            source,
            expression.start_position(),
            expression.end_position(),
        ),
        owner_name_utf8_range: owner_name
            .map(|owner| point_range(owner.start_position(), owner.end_position())),
        owner_name_utf16_range: owner_name
            .map(|owner| utf16_range(source, owner.start_position(), owner.end_position())),
        callee_text: callee_text.to_string(),
    });
}

fn enclosing_named_execution_owner(mut node: Node<'_>) -> Option<Node<'_>> {
    loop {
        if is_named_execution_owner(node.kind()) {
            if let Some(name) = execution_owner_name(node) {
                return Some(name);
            }
        }
        node = node.parent()?;
    }
}

fn is_named_execution_owner(kind: &str) -> bool {
    matches!(
        kind,
        // C#
        "method_declaration"
            | "constructor_declaration"
            | "destructor_declaration"
            | "operator_declaration"
            | "conversion_operator_declaration"
            | "local_function_statement"
            | "property_declaration"
            | "indexer_declaration"
            | "event_declaration"
            // Java
            | "compact_constructor_declaration"
            // C / C++
            | "function_definition"
            // Go
            | "function_declaration"
            // Rust
            | "function_item"
    )
}

fn execution_owner_name(node: Node<'_>) -> Option<Node<'_>> {
    for field in ["name", "declarator"] {
        if let Some(candidate) = node.child_by_field_name(field) {
            if let Some(name) = declarator_name(candidate) {
                return Some(name);
            }
        }
    }
    None
}

fn declarator_name(node: Node<'_>) -> Option<Node<'_>> {
    if is_callee_leaf(node.kind()) {
        return Some(node);
    }
    for field in ["name", "declarator", "field"] {
        if let Some(child) = node.child_by_field_name(field) {
            if let Some(name) = declarator_name(child) {
                return Some(name);
            }
        }
    }
    None
}

fn callee_leaf(node: Node<'_>) -> Option<Node<'_>> {
    if is_callee_leaf(node.kind()) {
        return Some(node);
    }
    for field in ["name", "field", "member"] {
        if let Some(child) = node.child_by_field_name(field) {
            if let Some(found) = callee_leaf(child) {
                return Some(found);
            }
        }
    }
    if let Some(function) = node.child_by_field_name("function") {
        return callee_leaf(function);
    }
    let mut cursor = node.walk();
    let children = node.named_children(&mut cursor).collect::<Vec<_>>();
    children.into_iter().rev().find_map(callee_leaf)
}

fn is_callee_leaf(kind: &str) -> bool {
    matches!(
        kind,
        "identifier"
            | "field_identifier"
            | "type_identifier"
            | "namespace_identifier"
            | "operator_name"
            | "destructor_name"
    )
}

fn keyword_span<'tree>(node: Node<'tree>, source: &str, keyword: &str) -> Option<Node<'tree>> {
    let mut cursor = node.walk();
    let found = node.children(&mut cursor).find(|child| {
        child
            .utf8_text(source.as_bytes())
            .is_ok_and(|text| text == keyword)
    });
    found
}

fn point_range(start: Point, end: Point) -> Vec<i32> {
    vec![
        start.row as i32,
        start.column as i32,
        end.row as i32,
        end.column as i32,
    ]
}

fn utf16_range(source: &str, start: Point, end: Point) -> Vec<i32> {
    vec![
        start.row as i32,
        utf16_column(source, start) as i32,
        end.row as i32,
        utf16_column(source, end) as i32,
    ]
}

fn utf16_column(source: &str, point: Point) -> usize {
    source
        .lines()
        .nth(point.row)
        .and_then(|line| line.get(..point.column))
        .map(|prefix| prefix.encode_utf16().count())
        .unwrap_or(point.column)
}

fn canonical_bounds(range: &[i32]) -> Option<((i32, i32), (i32, i32))> {
    match range {
        [line, start, end] => Some(((*line, *start), (*line, *end))),
        [start_line, start_column, end_line, end_column, ..] => {
            Some(((*start_line, *start_column), (*end_line, *end_column)))
        }
        _ => None,
    }
}

fn ranges_equal(left: &[i32], right: &[i32]) -> bool {
    canonical_bounds(left) == canonical_bounds(right)
}

fn range_contains(outer: &[i32], inner: &[i32]) -> bool {
    let Some((outer_start, outer_end)) = canonical_bounds(outer) else {
        return false;
    };
    let Some((inner_start, inner_end)) = canonical_bounds(inner) else {
        return false;
    };
    outer_start <= inner_start && inner_end <= outer_end
}

fn form_rank(form: CallSiteForm) -> u8 {
    match form {
        CallSiteForm::Call => 0,
        CallSiteForm::MethodCall => 1,
        CallSiteForm::Construct => 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csharp_generic_construction_is_a_construct_not_a_missing_call() {
        let source = "class P { void M() { var value = new Box<string>(\"x\"); value.Get(); } }";
        let sites = inventory_call_sites("csharp", source).unwrap();
        assert!(sites
            .iter()
            .any(|site| { site.form == CallSiteForm::Construct && site.callee_text == "Box" }));
        assert!(sites
            .iter()
            .any(|site| { site.form == CallSiteForm::MethodCall && site.callee_text == "Get" }));
    }

    #[test]
    fn csharp_expression_body_keeps_the_method_as_the_exact_call_owner() {
        let source = "class DbContext { int SaveChanges() => SaveChanges(true); int SaveChanges(bool value) => 1; }";
        let sites = inventory_call_sites("csharp", source).unwrap();
        let site = sites
            .iter()
            .find(|site| site.callee_text == "SaveChanges")
            .unwrap();
        let owner = site.owner_name_utf8_range.as_ref().unwrap();
        assert_eq!(&source[range_bytes(source, owner)], "SaveChanges");
        assert_ne!(owner, &site.callee_utf8_range);
    }

    #[test]
    fn java_inventory_preserves_method_constructor_and_exact_owner_sites() {
        let source = "class Service { Service() { this(1); } Service(int value) {} void handle() { repository.save(); helper(); new User(); } }";
        let sites = inventory_call_sites("java", source).unwrap();

        let save = sites
            .iter()
            .find(|site| site.callee_text == "save")
            .unwrap();
        assert_eq!(save.form, CallSiteForm::MethodCall);
        let owner = save.owner_name_utf8_range.as_ref().unwrap();
        assert_eq!(&source[range_bytes(source, owner)], "handle");
        assert!(sites
            .iter()
            .any(|site| site.form == CallSiteForm::Call && site.callee_text == "helper"));
        assert!(sites
            .iter()
            .any(|site| site.form == CallSiteForm::Construct && site.callee_text == "User"));
        assert!(sites
            .iter()
            .any(|site| site.form == CallSiteForm::Construct && site.callee_text == "this"));
    }

    #[test]
    fn c_and_cpp_inventory_calls_without_treating_declarations_as_calls() {
        let c = "int add(int a, int b) { return a + b; } int main(void) { return add(1, 2); }";
        let c_sites = inventory_call_sites("c", c).unwrap();
        assert_eq!(
            c_sites
                .iter()
                .filter(|site| site.callee_text == "add")
                .count(),
            1
        );

        let cpp = "template<class T> class Box { public: explicit Box(T v) {} }; int main() { Box<int> value(2); }";
        let cpp_sites = inventory_call_sites("cpp", cpp).unwrap();
        assert!(cpp_sites
            .iter()
            .any(|site| { site.form == CallSiteForm::Construct && site.callee_text == "Box" }));
        assert!(!cpp_sites.iter().any(|site| {
            site.form == CallSiteForm::Call
                && site.expression_utf8_range.first() == Some(&0)
                && site.callee_text == "Box"
        }));
    }

    #[test]
    fn go_and_rust_have_one_syntax_site_even_if_provider_returns_multiple_targets() {
        let go = "package p\ntype U struct{}\nfunc (U) ID() string { return \"x\" }\nfunc f(u U) { _ = u.ID() }";
        let go_sites = inventory_call_sites("go", go).unwrap();
        assert_eq!(
            go_sites
                .iter()
                .filter(|site| site.callee_text == "ID")
                .count(),
            1
        );

        let rust = "trait E { fn id(&self) -> &str; }\nimpl E for U { fn id(&self) -> &str { \"x\" } }\nfn f(u: U) { u.id(); }";
        let rust_sites = inventory_call_sites("rust", rust).unwrap();
        assert_eq!(
            rust_sites
                .iter()
                .filter(|site| site.callee_text == "id")
                .count(),
            1
        );
    }

    #[test]
    fn provider_ranges_match_utf8_or_utf16_without_guessing_the_protocol() {
        let source = "class P { void M() { café(); } }";
        let sites = inventory_call_sites("csharp", source).unwrap();
        let site = sites
            .iter()
            .find(|site| site.callee_text == "café")
            .unwrap();
        assert!(site.matches_provider_range(&site.callee_utf8_range));
        assert!(site.matches_provider_range(&site.callee_utf16_range));
    }

    fn range_bytes(source: &str, range: &[i32]) -> std::ops::Range<usize> {
        let line_offsets = std::iter::once(0)
            .chain(source.match_indices('\n').map(|(index, _)| index + 1))
            .collect::<Vec<_>>();
        let [start_line, start_column, end_line, end_column] = range else {
            panic!("expected four-part range");
        };
        (line_offsets[*start_line as usize] + *start_column as usize)
            ..(line_offsets[*end_line as usize] + *end_column as usize)
    }
}
