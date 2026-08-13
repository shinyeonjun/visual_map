//! Python 전용 AST 보강기다.
//!
//! 공통 walker는 Python의 선언·호출·제어흐름을 수집한다. 이 모듈은 공통 walker가
//! 소스 텍스트 전체를 하나의 import 또는 decorator로 취급하면서 잃어버리는
//! Python 문법의 경계를 AST 기준으로 복원한다.

mod ast;
mod imports;
mod syntax;

use crate::config::ParserPolicy;
use crate::facts::{Entrypoint, EntrypointKind, Evidence, FactBundle, ReferenceKind};
use crate::languages::common::metadata::{node_span, node_text, stable_id};
use crate::languages::common::{analyze_tree_with_hook, LanguageAnalyzer};
use crate::model::{FileEntry, Language};
use tree_sitter::Node;

use ast::{walk_nodes, UnitSpanIndex};
use imports::extract_import_references;
use syntax::{extract_call_site_facts, extract_decorator_facts};

pub fn analyzer() -> Box<dyn LanguageAnalyzer> {
    Box::new(PythonAnalyzer)
}

pub fn normalize(bundle: &mut FactBundle, patterns: &[String]) {
    crate::languages::common::mark_dynamic_calls(bundle, patterns);
}

struct PythonAnalyzer;

impl LanguageAnalyzer for PythonAnalyzer {
    fn language(&self) -> Language {
        Language::Python
    }

    fn analyze(&self, file: &FileEntry, source: &str, parser_policy: &ParserPolicy) -> FactBundle {
        analyze_tree_with_hook(
            Language::Python,
            || tree_sitter_python::LANGUAGE.into(),
            file,
            source,
            parser_policy,
            extract_python_facts,
        )
    }
}

fn extract_python_facts(root: Node<'_>, source: &[u8], file: &FileEntry, bundle: &mut FactBundle) {
    // 공통 walker가 만든 Python import는 import_from_statement를 문장 전체로
    // 기록하므로, Python AST 기준의 정규화 결과로 교체한다.
    bundle
        .references
        .retain(|reference| reference.kind != ReferenceKind::Import);

    // route의 의미는 framework adapter가 결정한다. Python 분석기는 generic
    // line_facts가 만든 route를 제거하고 decorator 원시 사실만 보존한다.
    bundle
        .entrypoints
        .retain(|entrypoint| entrypoint.kind != EntrypointKind::Http);

    normalize_python_unit_kinds(bundle);
    let unit_index = UnitSpanIndex::build(&bundle.units);
    for node in walk_nodes(root) {
        match node.kind() {
            "import_statement" | "future_import_statement" | "import_from_statement" => {
                extract_import_references(node, source, file, bundle, &unit_index);
            }
            "decorated_definition" => {
                extract_decorator_facts(node, source, file, bundle);
            }
            "call" => {
                extract_call_site_facts(node, source, file, bundle, &unit_index);
            }
            _ => {}
        }
    }
    extract_python_cli_guard(root, source, file, bundle);
}

/// `if __name__ == "__main__":`는 Python CLI의 정적 진입점 경계다.
/// 실제 호출 대상이 하나로 확정되면 그 함수에 연결하고, 그렇지 않으면
/// 파일 유닛에 귀속해 미해결 상태를 숨기지 않는다.
fn extract_python_cli_guard(
    root: Node<'_>,
    source: &[u8],
    file: &FileEntry,
    bundle: &mut FactBundle,
) {
    for node in walk_nodes(root) {
        if node.kind() != "if_statement" {
            continue;
        }
        let condition = node
            .child_by_field_name("condition")
            .map(|condition| node_text(condition, source).to_ascii_lowercase())
            .unwrap_or_default();
        if !condition.contains("__name__") || !condition.contains("__main__") {
            continue;
        }
        let target_unit_id = bundle
            .units
            .iter()
            .filter(|unit| unit.name == "main")
            .map(|unit| unit.id.clone())
            .collect::<Vec<_>>();
        let unit_id = target_unit_id
            .first()
            .cloned()
            .unwrap_or_else(|| stable_id("unit", &format!("{}:file", file.file_id)));
        let id = stable_id(
            "entry",
            &format!("{}:python-cli:{}", file.file_id, node.start_byte()),
        );
        if bundle
            .entrypoints
            .iter()
            .any(|entrypoint| entrypoint.id == id)
        {
            continue;
        }
        bundle.entrypoints.push(Entrypoint {
            id,
            unit_id,
            kind: EntrypointKind::Cli,
            name: "python-cli".to_string(),
            method: Some("CLI".to_string()),
            path: None,
            framework_id: None,
            evidence: vec![Evidence::new(
                "cliGuard",
                node_text(node, source),
                node_span(node, file),
            )],
        });
    }
}

fn normalize_python_unit_kinds(bundle: &mut FactBundle) {
    for unit in &mut bundle.units {
        if unit.kind == crate::facts::CodeUnitKind::Class {
            let signature = unit.signature.as_deref().unwrap_or_default();
            let compact = signature
                .to_ascii_lowercase()
                .chars()
                .filter(|character| !character.is_whitespace())
                .collect::<String>();
            if compact.contains("protocol") {
                unit.kind = crate::facts::CodeUnitKind::Interface;
            } else if compact.contains("enum") {
                unit.kind = crate::facts::CodeUnitKind::Enum;
            }
        }
        if unit.name == "__init__"
            && matches!(
                unit.kind,
                crate::facts::CodeUnitKind::Function | crate::facts::CodeUnitKind::Method
            )
        {
            unit.kind = crate::facts::CodeUnitKind::Constructor;
        }
    }
}
