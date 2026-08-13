use crate::facts::FactBundle;
use crate::languages::common::{LanguageAnalyzer, TreeSitterAnalyzer};
use crate::model::Language;

pub fn analyzer() -> Box<dyn LanguageAnalyzer> {
    Box::new(TreeSitterAnalyzer::new(Language::Java, || {
        tree_sitter_java::LANGUAGE.into()
    }))
}

pub fn normalize(bundle: &mut FactBundle, patterns: &[String]) {
    crate::languages::common::mark_dynamic_calls(bundle, patterns);
}
