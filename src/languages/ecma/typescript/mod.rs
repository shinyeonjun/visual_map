use crate::facts::FactBundle;
use crate::languages::common::LanguageAnalyzer;

/// ECMAScript 공통 분석에 TypeScript grammar를 연결한다.
pub fn analyzer() -> Box<dyn LanguageAnalyzer> {
    Box::new(super::common::typescript())
}

pub fn normalize(bundle: &mut FactBundle, patterns: &[String]) {
    crate::languages::common::mark_dynamic_calls(bundle, patterns);
}
