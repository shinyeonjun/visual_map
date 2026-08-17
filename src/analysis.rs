use crate::model::{AnalysisRequest, AnalysisResult};
use crate::EngineError;

/// 코드 분석 엔진의 현재 공개 진입점이다.
pub fn analyze(request: AnalysisRequest) -> Result<AnalysisResult, EngineError> {
    AnalysisEngine::default().analyze(request)
}

/// 정적 파이프라인을 재사용하는 실행 객체다.
///
/// 같은 인스턴스로 반복 분석하면 내용이 바뀌지 않은 파일의 AST Facts를
/// 재사용한다. 그래프와 도메인 결과는 매번 전체적으로 재구성한다.
#[derive(Debug, Default)]
pub struct AnalysisEngine {
    pipeline: crate::pipeline::DomainAnalysisPipeline,
}

impl AnalysisEngine {
    pub fn analyze(&self, request: AnalysisRequest) -> Result<AnalysisResult, EngineError> {
        self.pipeline.run(request)
    }
}

#[cfg(test)]
mod tests {
    use super::AnalysisEngine;
    use crate::model::AnalysisRequest;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn 같은_engine의_반복분석은_파일_facts를_증분재사용한다() {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("시계를 읽어야 한다")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("visual-map-incremental-{suffix}"));
        fs::create_dir_all(&root).expect("임시 프로젝트를 만들어야 한다");
        let path = root.join("app.py");
        fs::write(&path, "def first():\n    return 1\n").expect("첫 소스를 써야 한다");

        let engine = AnalysisEngine::default();
        let first = engine
            .analyze(AnalysisRequest::new(&root))
            .expect("첫 분석이 성공해야 한다");
        let first_cache_len = engine.pipeline.fact_cache.len();
        let second = engine
            .analyze(AnalysisRequest::new(&root))
            .expect("반복 분석이 성공해야 한다");
        let second_cache_len = engine.pipeline.fact_cache.len();

        assert_eq!(first.summary.total_files, second.summary.total_files);
        assert_eq!(first_cache_len, 1);
        assert_eq!(second_cache_len, 1);

        fs::write(&path, "def second():\n    return 2\n").expect("변경 소스를 써야 한다");
        let changed = engine
            .analyze(AnalysisRequest::new(&root))
            .expect("변경 분석이 성공해야 한다");
        assert!(changed
            .overview
            .expect("변경 결과 Overview가 있어야 한다")
            .units
            .iter()
            .any(|unit| unit.name == "second"));

        fs::remove_dir_all(root).expect("임시 프로젝트를 정리해야 한다");
    }
}
