use crate::facts::FactBundle;
use crate::languages::common::LanguageAnalyzer;

/// C 계열 공통 분석에 C++ grammar를 연결한다.
pub fn analyzer() -> Box<dyn LanguageAnalyzer> {
    Box::new(super::common::cpp())
}

pub fn normalize(bundle: &mut FactBundle, patterns: &[String]) {
    crate::languages::common::mark_dynamic_calls(bundle, patterns);
}
