//! 언어별 구문 분석기 영역.
//!
//! 각 분석기는 언어 문법에서 사실을 추출하고 `crate::facts`의 공통 형태로
//! 변환한다. 도메인 그룹화 로직은 이 폴더를 직접 참조하지 않는다.

pub mod c_family;
pub mod common;
pub mod csharp;
pub mod dart;
pub mod ecma;
pub mod go;
pub mod java;

pub use common::LanguageAnalyzer;

/// 언어 감지 결과를 AST 추출, 계열 공통 보강, 개별 언어 보강으로 통과시킨다.
pub fn analyze_file(
    file: &crate::model::FileEntry,
    source: &str,
    config: &crate::config::AnalysisConfig,
) -> crate::facts::FactBundle {
    let mut bundle = analyzer_for(file.language).analyze(file, source, &config.parser);
    common::normalize_bundle(&mut bundle);
    let dynamic_patterns = config
        .parser
        .dynamic_call_patterns
        .get(file.language.key())
        .map(Vec::as_slice)
        .unwrap_or(&[]);

    match file.language {
        crate::model::Language::JavaScript => {
            ecma::common::normalize(&mut bundle);
            ecma::javascript::normalize(&mut bundle, dynamic_patterns);
        }
        crate::model::Language::TypeScript => {
            ecma::common::normalize(&mut bundle);
            ecma::typescript::normalize(&mut bundle, dynamic_patterns);
        }
        crate::model::Language::C => {
            c_family::common::normalize(&mut bundle);
            c_family::c::normalize(&mut bundle, dynamic_patterns);
        }
        crate::model::Language::Cpp => {
            c_family::common::normalize(&mut bundle);
            c_family::cpp::normalize(&mut bundle, dynamic_patterns);
        }
        crate::model::Language::Python => python::normalize(&mut bundle, dynamic_patterns),
        crate::model::Language::Java => java::normalize(&mut bundle, dynamic_patterns),
        crate::model::Language::CSharp => csharp::normalize(&mut bundle, dynamic_patterns),
        crate::model::Language::Go => go::normalize(&mut bundle, dynamic_patterns),
        crate::model::Language::Rust => rust::normalize(&mut bundle, dynamic_patterns),
        crate::model::Language::Dart => dart::normalize(&mut bundle, dynamic_patterns),
        crate::model::Language::Unknown => {}
    }
    common::normalize_bundle(&mut bundle);
    bundle
}

/// 감지된 언어에 맞는 AST 분석기를 만든다.
pub fn analyzer_for(language: crate::model::Language) -> Box<dyn LanguageAnalyzer> {
    match language {
        crate::model::Language::JavaScript => ecma::javascript::analyzer(),
        crate::model::Language::TypeScript => ecma::typescript::analyzer(),
        crate::model::Language::Python => python::analyzer(),
        crate::model::Language::Java => java::analyzer(),
        crate::model::Language::C => c_family::c::analyzer(),
        crate::model::Language::Cpp => c_family::cpp::analyzer(),
        crate::model::Language::CSharp => csharp::analyzer(),
        crate::model::Language::Go => go::analyzer(),
        crate::model::Language::Rust => rust::analyzer(),
        crate::model::Language::Dart => dart::analyzer(),
        crate::model::Language::Unknown => Box::new(common::TreeSitterAnalyzer::new(
            crate::model::Language::Unknown,
            || tree_sitter_javascript::LANGUAGE.into(),
        )),
    }
}
pub mod python;
pub mod rust;
