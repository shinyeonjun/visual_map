use crate::facts::FactBundle;
use crate::languages::common::{LanguageAnalyzer, TreeSitterAnalyzer};
use crate::model::Language;

pub fn analyzer() -> Box<dyn LanguageAnalyzer> {
    Box::new(TreeSitterAnalyzer::new(Language::Go, || {
        tree_sitter_go::LANGUAGE.into()
    }))
}

pub fn normalize(bundle: &mut FactBundle, patterns: &[String]) {
    crate::languages::common::mark_dynamic_calls(bundle, patterns);
}
