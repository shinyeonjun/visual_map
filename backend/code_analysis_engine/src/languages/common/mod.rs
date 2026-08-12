//! 여러 언어가 공유하는 Tree-sitter 분석 기반과 Facts 정규화 경계다.

mod call_sites;
mod declarations;
mod flow_facts;
mod line_facts;
pub(crate) mod metadata;
mod normalization;
mod references;
mod resources;
pub(crate) mod unit_index;
mod walker;

use crate::config::ParserPolicy;
use crate::diagnostics::Diagnostic;
use crate::facts::{CodeUnit, CodeUnitKind, FactBundle};
use crate::model::{FileEntry, Language};
use std::path::Path;
use tree_sitter::{Language as TreeLanguage, Parser};

use line_facts::extract_line_facts;
use metadata::{file_module_name, stable_id};

// 기존 외부 호출 경로를 유지하고 실제 구현은 책임별 모듈에 둔다.
pub use normalization::{mark_dynamic_calls, normalize_bundle};

/// 언어별 AST 분석기가 구현해야 하는 공통 계약이다.
pub trait LanguageAnalyzer: Send + Sync {
    fn language(&self) -> Language;
    fn analyze(&self, file: &FileEntry, source: &str, parser_policy: &ParserPolicy) -> FactBundle;
}

pub use LanguageAnalyzer as Analyzer;

/// Tree-sitter grammar를 사용하는 기본 언어 분석기다.
pub struct TreeSitterAnalyzer {
    language: Language,
    grammar: fn() -> TreeLanguage,
}

impl TreeSitterAnalyzer {
    pub fn new(language: Language, grammar: fn() -> TreeLanguage) -> Self {
        Self { language, grammar }
    }
}

impl LanguageAnalyzer for TreeSitterAnalyzer {
    fn language(&self) -> Language {
        self.language
    }

    fn analyze(&self, file: &FileEntry, source: &str, parser_policy: &ParserPolicy) -> FactBundle {
        analyze_tree(self.language, self.grammar, file, source, parser_policy)
    }
}

/// Tree-sitter AST를 공통 Facts로 변환한다.
pub fn analyze_tree(
    language: Language,
    grammar: fn() -> TreeLanguage,
    file: &FileEntry,
    source: &str,
    parser_policy: &ParserPolicy,
) -> FactBundle {
    analyze_tree_with_hook(
        language,
        grammar,
        file,
        source,
        parser_policy,
        |_root, _source, _file, _bundle| {},
    )
}

/// 공통 AST 추출 뒤 언어별 보강기를 실행한다.
///
/// 공통 walker가 모든 언어에서 재사용되도록 유지하면서, Python decorator/import처럼
/// 문법별 해석이 필요한 사실만 언어 분석기가 후처리한다.
pub fn analyze_tree_with_hook<F>(
    language: Language,
    grammar: fn() -> TreeLanguage,
    file: &FileEntry,
    source: &str,
    parser_policy: &ParserPolicy,
    hook: F,
) -> FactBundle
where
    F: FnOnce(tree_sitter::Node<'_>, &[u8], &FileEntry, &mut FactBundle),
{
    let file_span = crate::facts::SourceSpan::new(
        file.file_id.clone(),
        file.relative_path.clone(),
        1,
        1,
        file.line_count.max(1) as u32,
        1,
    );
    let file_unit_id = stable_id("unit", &format!("{}:file", file.file_id));
    let mut bundle = FactBundle {
        language: Some(language),
        file_id: file.file_id.clone(),
        parse_status: crate::model::ParseStatus::NotAnalyzed,
        units: vec![CodeUnit {
            id: file_unit_id.clone(),
            kind: CodeUnitKind::File,
            name: file.relative_path.clone(),
            qualified_name: file_module_name(&file.relative_path),
            file_id: file.file_id.clone(),
            relative_path: file.relative_path.clone(),
            language,
            parent_id: None,
            span: file_span,
            body_span: None,
            signature: None,
            parameters: Vec::new(),
            return_type: None,
            visibility: crate::facts::CodeUnitVisibility::Public,
            modifiers: Vec::new(),
            exported: true,
        }],
        ..FactBundle::default()
    };

    // 확장자가 없는 Play `conf/routes` 같은 프레임워크 DSL은 언어 AST가
    // 아니다. JavaScript 문법으로 억지 파싱하면 항상 부분 파싱 경고가
    // 생기므로, 설정된 line fact만 추출하고 파일 단위 사실로 보존한다.
    if language == Language::Unknown {
        bundle.parse_status = crate::model::ParseStatus::Parsed;
        let unit_index = unit_index::UnitSpanIndex::build(&bundle.units);
        extract_line_facts(
            language,
            file,
            source,
            &unit_index,
            &mut bundle,
            parser_policy,
        );
        return bundle;
    }

    let mut parser = Parser::new();
    let tree_language = grammar();
    if let Err(error) = parser.set_language(&tree_language) {
        bundle.diagnostics.push(Diagnostic::error(
            "LANGUAGE_SETUP_FAILED",
            format!("언어 파서를 설정하지 못했습니다: {error}"),
            Path::new(&file.relative_path),
        ));
        bundle.parse_status = crate::model::ParseStatus::Failed;
        return bundle;
    }

    let Some(tree) = parser.parse(source, None) else {
        bundle.diagnostics.push(Diagnostic::error(
            "PARSE_FAILED",
            "언어 AST를 생성하지 못했습니다.",
            Path::new(&file.relative_path),
        ));
        bundle.parse_status = crate::model::ParseStatus::Failed;
        return bundle;
    };

    if tree.root_node().has_error() {
        bundle.parse_status = crate::model::ParseStatus::ParsedWithErrors;
        bundle.diagnostics.push(Diagnostic::warning(
            "PARSE_ERROR",
            "문법 오류가 있는 AST를 부분적으로 분석합니다.",
            Path::new(&file.relative_path),
        ));
    } else {
        bundle.parse_status = crate::model::ParseStatus::Parsed;
    }

    let root = tree.root_node();
    walker::walk_tree(
        root,
        source.as_bytes(),
        file,
        &file_unit_id,
        None,
        &mut bundle,
    );
    let units = bundle.units.clone();
    let unit_index = unit_index::UnitSpanIndex::build(&units);
    call_sites::extract(
        language,
        root,
        source.as_bytes(),
        file,
        &mut bundle,
        &unit_index,
    );
    extract_line_facts(
        language,
        file,
        source,
        &unit_index,
        &mut bundle,
        parser_policy,
    );
    hook(root, source.as_bytes(), file, &mut bundle);
    let call_sites = bundle.call_sites.clone();
    resources::extract(
        language,
        &call_sites,
        file,
        &mut bundle,
        &parser_policy.resource_rules,
    );
    bundle
}
