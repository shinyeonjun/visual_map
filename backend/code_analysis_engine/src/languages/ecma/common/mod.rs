use crate::config::ParserPolicy;
use crate::facts::{BindingKind, DecoratorFact, Evidence, SymbolBinding};
use crate::facts::{FactBundle, Reference, ReferenceKind, ResolutionStatus};
use crate::languages::common::metadata::{node_span, node_text, stable_id};
use crate::languages::common::unit_index::UnitSpanIndex;
use crate::languages::common::{analyze_tree_with_hook, LanguageAnalyzer};
use crate::model::{FileEntry, Language};
use tree_sitter::Language as TreeLanguage;
use tree_sitter::Node;

/// JavaScript와 TypeScript가 공유하는 AST 분석의 기반을 만든다.
pub fn javascript() -> EcmaAnalyzer {
    EcmaAnalyzer {
        language: Language::JavaScript,
        grammar: || tree_sitter_javascript::LANGUAGE.into(),
    }
}

/// TypeScript의 공통 ECMAScript AST 분석 기반을 만든다.
pub fn typescript() -> EcmaAnalyzer {
    EcmaAnalyzer {
        language: Language::TypeScript,
        grammar: || tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
    }
}

pub struct EcmaAnalyzer {
    language: Language,
    grammar: fn() -> TreeLanguage,
}

impl EcmaAnalyzer {
    fn grammar_for(&self, file: &FileEntry) -> fn() -> TreeLanguage {
        if self.language == Language::TypeScript
            && file
                .relative_path
                .rsplit_once('.')
                .is_some_and(|(_, extension)| extension.eq_ignore_ascii_case("tsx"))
        {
            return || tree_sitter_typescript::LANGUAGE_TSX.into();
        }
        self.grammar
    }
}

impl LanguageAnalyzer for EcmaAnalyzer {
    fn language(&self) -> Language {
        self.language
    }

    fn analyze(&self, file: &FileEntry, source: &str, parser_policy: &ParserPolicy) -> FactBundle {
        analyze_tree_with_hook(
            self.language,
            self.grammar_for(file),
            file,
            source,
            parser_policy,
            extract_ecma_facts,
        )
    }
}

fn extract_ecma_facts(root: Node<'_>, source: &[u8], file: &FileEntry, bundle: &mut FactBundle) {
    extract_ecma_decorators(root, source, file, bundle);
    extract_ecma_import_bindings(root, source, file, bundle);
    extract_ecma_exports(root, source, file, bundle);
    extract_ecma_type_references(root, source, file, bundle);
}

/// ES module의 imported local 이름을 실제 모듈·심볼 경로에 연결한다.
///
/// 공통 walker는 `import_declaration` 자체를 모듈 관계로 보존하지만,
/// `import { User } from "./models"`의 `User`가 어떤 외부 심볼을 가리키는지는
/// 알 수 없다. 이 binding이 있으면 호출·생성·타입 참조가 모두 같은
/// `./models::User` 경로로 해석되어 파일 간 관계가 하나의 계약을 공유한다.
fn extract_ecma_import_bindings(
    root: Node<'_>,
    source: &[u8],
    file: &FileEntry,
    bundle: &mut FactBundle,
) {
    let file_unit_id = bundle
        .units
        .iter()
        .find(|unit| unit.kind == crate::facts::CodeUnitKind::File)
        .map(|unit| unit.id.clone())
        .unwrap_or_else(|| file.file_id.clone());
    for node in walk_nodes(root) {
        if !matches!(node.kind(), "import_declaration" | "import_statement") {
            continue;
        }
        let statement = node_text(node, source);
        let Some((module, specifiers)) = parse_import_statement(&statement) else {
            continue;
        };
        for (local_name, imported_name, kind) in specifiers {
            let target_name = format!("{module}::{imported_name}");
            let id = stable_id(
                "binding",
                &format!("{}:{}:{}", file.file_id, node.start_byte(), local_name),
            );
            if bundle.bindings.iter().any(|binding| binding.id == id) {
                continue;
            }
            bundle.bindings.push(SymbolBinding {
                id,
                source_unit_id: file_unit_id.clone(),
                local_name,
                target_name,
                kind,
                evidence: vec![Evidence::new(
                    "importBinding",
                    statement.trim(),
                    node_span(node, file),
                )],
            });
        }
    }
}

type ImportBindingSpec = (String, String, BindingKind);

fn parse_import_statement(statement: &str) -> Option<(String, Vec<ImportBindingSpec>)> {
    let statement = statement
        .trim()
        .trim_start_matches("import")
        .trim()
        .trim_end_matches(';')
        .trim();
    let (clause, module) = statement.split_once(" from ").unwrap_or(("", statement));
    let module = module
        .trim()
        .trim_matches(['"', '\'', '`'])
        .trim()
        .to_string();
    if module.is_empty() {
        return None;
    }

    let mut specifiers = Vec::new();
    let clause = clause.trim();
    if clause.is_empty() {
        return Some((module, specifiers));
    }
    if let Some((_, named)) = clause.split_once('{') {
        let named = named.split('}').next().unwrap_or_default();
        for item in named.split(',') {
            let item = item.trim();
            if item.is_empty() {
                continue;
            }
            let (imported, local) = item
                .split_once(" as ")
                .map(|(imported, local)| (imported.trim(), local.trim()))
                .unwrap_or((item, item));
            specifiers.push((
                local.to_string(),
                imported.trim_start_matches("type ").to_string(),
                if local == imported {
                    BindingKind::Import
                } else {
                    BindingKind::ImportAlias
                },
            ));
        }
    }
    if let Some((_, namespace)) = clause.split_once('*') {
        if let Some((_, local)) = namespace.split_once(" as ") {
            specifiers.push((
                local.trim().to_string(),
                "*".to_string(),
                BindingKind::ImportAlias,
            ));
        }
    }
    let default_name =
        clause.split(',').next().map(str::trim).filter(|name| {
            !name.is_empty() && !matches!(name.as_bytes().first(), Some(b'{' | b'*'))
        });
    if let Some(local) = default_name {
        specifiers.push((
            local.to_string(),
            "default".to_string(),
            BindingKind::Import,
        ));
    }
    Some((module, specifiers))
}

/// TypeScript의 타입 위치를 일반 `Uses` 관계로 보존한다. 타입 이름은
/// 실행 호출이 아니지만, 도메인·기능 그래프에서 DTO·인터페이스·엔티티 간
/// 연결을 보여주려면 동일한 참조 해석 계약을 사용해야 한다.
fn extract_ecma_type_references(
    root: Node<'_>,
    source: &[u8],
    file: &FileEntry,
    bundle: &mut FactBundle,
) {
    if file
        .relative_path
        .rsplit_once('.')
        .is_none_or(|(_, extension)| !matches!(extension, "ts" | "tsx" | "mts" | "cts"))
    {
        return;
    }
    let unit_index = UnitSpanIndex::build(&bundle.units);
    for node in walk_nodes(root) {
        if node.kind() != "type_identifier" || !is_type_reference(node) {
            continue;
        }
        let target_name = node_text(node, source).trim().to_string();
        if target_name.is_empty() {
            continue;
        }
        let source_unit_id = unit_index.unit_for_line(node.start_position().row as u32 + 1);
        let id = stable_id(
            "reference",
            &format!(
                "{}:type:{}:{}",
                file.file_id,
                node.start_byte(),
                target_name
            ),
        );
        if bundle.references.iter().any(|reference| reference.id == id) {
            continue;
        }
        bundle.references.push(Reference {
            id,
            source_unit_id,
            target_unit_id: None,
            candidate_unit_ids: Vec::new(),
            target_name: target_name.clone(),
            kind: ReferenceKind::Uses,
            status: ResolutionStatus::Candidate,
            evidence: vec![Evidence::new(
                "typeReference",
                target_name,
                node_span(node, file),
            )],
        });
    }
}

fn is_type_reference(node: Node<'_>) -> bool {
    if node.parent().is_some_and(|parent| {
        parent
            .child_by_field_name("name")
            .is_some_and(|name| name.id() == node.id())
    }) {
        return false;
    }
    let mut current = node.parent();
    while let Some(parent) = current {
        if matches!(
            parent.kind(),
            "type_annotation"
                | "return_type"
                | "type_arguments"
                | "type_parameters"
                | "extends_type_clause"
                | "implements_type_clause"
                | "generic_type"
                | "object_type"
                | "tuple_type"
                | "array_type"
                | "type_predicate"
        ) {
            return true;
        }
        if matches!(
            parent.kind(),
            "function_declaration"
                | "method_definition"
                | "class_declaration"
                | "interface_declaration"
                | "type_alias_declaration"
        ) {
            return false;
        }
        current = parent.parent();
    }
    false
}

fn walk_nodes(root: Node<'_>) -> Vec<Node<'_>> {
    let mut nodes = Vec::new();
    let mut pending = vec![root];
    while let Some(node) = pending.pop() {
        let mut cursor = node.walk();
        let mut children = node.named_children(&mut cursor).collect::<Vec<_>>();
        children.reverse();
        pending.extend(children);
        nodes.push(node);
    }
    nodes
}

fn extract_ecma_decorators(
    root: Node<'_>,
    source: &[u8],
    file: &FileEntry,
    bundle: &mut FactBundle,
) {
    let index = UnitSpanIndex::build(&bundle.units);
    let mut pending = vec![root];
    while let Some(node) = pending.pop() {
        if node.kind() == "decorator" {
            let expression = node_text(node, source)
                .trim()
                .trim_start_matches('@')
                .trim()
                .to_string();
            let (callee, arguments) = split_decorator_call(&expression);
            let (receiver, name) = callee
                .rsplit_once('.')
                .map(|(receiver, name)| (Some(receiver.trim().to_string()), name.to_string()))
                .unwrap_or((None, callee));
            let unit_id = index.unit_for_annotation_line(node.start_position().row as u32 + 1);
            bundle.decorators.push(DecoratorFact {
                id: stable_id(
                    "decorator",
                    &format!("{}:{}", file.file_id, node.start_byte()),
                ),
                unit_id,
                receiver,
                name,
                arguments,
                expression: expression.clone(),
                evidence: vec![Evidence::new(
                    "decorator",
                    expression,
                    node_span(node, file),
                )],
            });
        }
        let mut cursor = node.walk();
        let mut children = node.named_children(&mut cursor).collect::<Vec<_>>();
        children.reverse();
        pending.extend(children);
    }
}

/// JS/TS export declaration을 파일 간 관계로 보존한다.
fn extract_ecma_exports(root: Node<'_>, source: &[u8], file: &FileEntry, bundle: &mut FactBundle) {
    let unit_index = UnitSpanIndex::build(&bundle.units);
    let mut pending = vec![root];
    while let Some(node) = pending.pop() {
        if node.kind() == "export_statement" {
            let text = node_text(node, source).trim().to_string();
            for target_name in export_targets(&text) {
                if target_name.is_empty() {
                    continue;
                }
                let id = stable_id(
                    "reference",
                    &format!(
                        "{}:export:{}:{}",
                        file.file_id,
                        node.start_byte(),
                        target_name
                    ),
                );
                if bundle.references.iter().any(|reference| reference.id == id) {
                    continue;
                }
                bundle.references.push(Reference {
                    id,
                    source_unit_id: unit_index.unit_for_line(node.start_position().row as u32 + 1),
                    target_unit_id: None,
                    candidate_unit_ids: Vec::new(),
                    target_name: target_name.clone(),
                    kind: ReferenceKind::Export,
                    status: ResolutionStatus::Candidate,
                    evidence: vec![Evidence::new("export", target_name, node_span(node, file))],
                });
            }
        }
        let mut cursor = node.walk();
        let mut children = node.named_children(&mut cursor).collect::<Vec<_>>();
        children.reverse();
        pending.extend(children);
    }
}

fn export_targets(text: &str) -> Vec<String> {
    let body = text
        .trim_start_matches("export")
        .trim_start()
        .trim_end_matches(';')
        .trim();
    // `from`은 named re-export(`export { User } from "./models"`)에서만
    // 모듈 경로로 취급한다. 함수 본문 안의 문자열이나 식에 우연히 있는
    // `from`을 export 대상이라고 해석하지 않는다.
    let is_re_export = body.starts_with('{')
        || body.starts_with("* from ")
        || body.starts_with("* as ")
        || body.starts_with("type {");
    if is_re_export {
        if let Some(from) = body.rsplit_once(" from ") {
            return vec![from.1.trim().trim_matches(['"', '\'', '`']).to_string()];
        }
    }

    // 중괄호가 함수·클래스·객체 리터럴의 본문인지 named export 목록인지
    // 먼저 구분한다. 이전 구현은 모든 `{ ... }`를 목록으로 간주해
    // `export function f() { ... }`의 함수 본문 전체를 targetName에 넣었다.
    let named_exports = body.strip_prefix("type ").unwrap_or(body).trim_start();
    if named_exports.starts_with('{') {
        if let Some(close) = named_exports.find('}') {
            return named_exports[1..close]
                .split(',')
                .filter_map(|value| {
                    let value = value.trim();
                    let value = value
                        .split_once(" as ")
                        .map(|(name, _)| name)
                        .unwrap_or(value);
                    (!value.is_empty()).then(|| value.to_string())
                })
                .collect();
        }
    }

    let mut body = body.strip_prefix("default ").unwrap_or(body).trim();
    if let Some(async_body) = body.strip_prefix("async ") {
        if async_body.starts_with("function") {
            body = async_body.trim_start();
        }
    }
    for keyword in [
        "function",
        "class",
        "interface",
        "type",
        "enum",
        "const",
        "let",
        "var",
    ] {
        if let Some(rest) = body.strip_prefix(keyword) {
            let name = rest
                .trim_start()
                .split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
                .next()
                .unwrap_or_default();
            if !name.is_empty() && name != "default" {
                return vec![name.to_string()];
            }
        }
    }

    // 이름 없는 default object/function도 소스 본문을 반환하지 않는다.
    if body.starts_with('{') || body.starts_with("(") {
        return vec!["default".to_string()];
    }
    body.split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
        .next()
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .into_iter()
        .collect()
}

fn split_decorator_call(expression: &str) -> (String, Vec<String>) {
    let Some(open) = expression.find('(') else {
        return (expression.to_string(), Vec::new());
    };
    let callee = expression[..open].trim().to_string();
    let inner = expression[open + 1..].trim_end_matches(')').trim();
    (callee, split_arguments(inner))
}

fn split_arguments(value: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut current = String::new();
    let mut depth = 0_i32;
    let mut quote = None;
    for character in value.chars() {
        if let Some(active) = quote {
            current.push(character);
            if character == active {
                quote = None;
            }
            continue;
        }
        match character {
            '\'' | '"' | '`' => {
                quote = Some(character);
                current.push(character);
            }
            '(' | '[' | '{' => {
                depth += 1;
                current.push(character);
            }
            ')' | ']' | '}' => {
                depth -= 1;
                current.push(character);
            }
            ',' if depth == 0 => {
                if !current.trim().is_empty() {
                    result.push(current.trim().to_string());
                }
                current.clear();
            }
            _ => current.push(character),
        }
    }
    if !current.trim().is_empty() {
        result.push(current.trim().to_string());
    }
    result
}

/// JavaScript 계열의 import 경로를 공통 이름으로 정규화한다.
pub fn normalize(bundle: &mut FactBundle) {
    for reference in &mut bundle.references {
        if reference.kind != ReferenceKind::Import {
            continue;
        }
        let value = reference.target_name.trim();
        let normalized = value
            .rsplit_once(" from ")
            .map(|(_, target)| target)
            .unwrap_or(value)
            .trim()
            .trim_matches(['"', '\'', ';']);
        reference.target_name = normalized.to_string();
    }
}

#[cfg(test)]
mod tests {
    use super::export_targets;

    #[test]
    fn 함수와_클래스_export는_본문이_아닌_선언이름만_보존한다() {
        assert_eq!(
            export_targets("export async function startApplication() {\n return true;\n }")
                .as_slice(),
            ["startApplication"]
        );
        assert_eq!(
            export_targets("export class AppComponent { render() {} }").as_slice(),
            ["AppComponent"]
        );
    }

    #[test]
    fn named_export와_re_export는_기존_관계를_보존한다() {
        assert_eq!(
            export_targets("export { login, logout as signOut };").as_slice(),
            ["login", "logout"]
        );
        assert_eq!(
            export_targets("export { User } from \"./models\";").as_slice(),
            ["./models"]
        );
    }

    #[test]
    fn default_object_export는_객체_본문을_참조이름으로_만들지_않는다() {
        let targets = export_targets("export default { ready: true, run() {} };");
        assert_eq!(targets.as_slice(), ["default"]);
        assert!(targets.iter().all(|target| !target.contains(['\n', '\r'])));
    }
}
