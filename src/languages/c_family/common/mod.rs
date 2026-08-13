use crate::facts::{FactBundle, ReferenceKind};
use crate::languages::common::TreeSitterAnalyzer;
use crate::model::Language;

/// C와 C++가 공유하는 AST 분석 기반을 만든다.
pub fn c() -> TreeSitterAnalyzer {
    TreeSitterAnalyzer::new(Language::C, || tree_sitter_c::LANGUAGE.into())
}

/// C++ 전용 grammar에 C 계열 공통 분석을 적용할 기반을 만든다.
pub fn cpp() -> TreeSitterAnalyzer {
    TreeSitterAnalyzer::new(Language::Cpp, || tree_sitter_cpp::LANGUAGE.into())
}

/// C 계열 include 경로를 공통 이름으로 정규화한다.
pub fn normalize(bundle: &mut FactBundle) {
    for reference in &mut bundle.references {
        if reference.kind == ReferenceKind::Include {
            reference.target_name = reference
                .target_name
                .trim()
                .trim_matches(['"', '\'', '<', '>', ';'])
                .to_string();
        }
    }
}
