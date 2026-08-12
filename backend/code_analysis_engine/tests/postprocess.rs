use code_analysis_engine::config::AnalysisConfig;
use code_analysis_engine::postprocess::{build_codex_context, PostprocessError};
use code_analysis_engine::{analyze, AnalysisRequest};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn temporary_project(name: &str) -> PathBuf {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("시간을 읽어야 한다")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("visual-map-postprocess-{name}-{suffix}"));
    fs::create_dir_all(&root).expect("임시 프로젝트를 만들어야 한다");
    root
}

#[test]
fn 분석결과에서_codex_컨텍스트를_재분석없이_생성한다() {
    let root = temporary_project("context");
    fs::write(
        root.join("service.ts"),
        r#"
export function createOrder() {
  return saveOrder(loadOrder());
}
function loadOrder() { return fetch("/orders"); }
function saveOrder(order: unknown) { return order; }
"#,
    )
    .expect("소스 파일을 써야 한다");

    let result = analyze(AnalysisRequest::new(&root)).expect("정적 분석이 성공해야 한다");
    let context = build_codex_context(&result, &AnalysisConfig::default())
        .expect("Codex 컨텍스트가 생성되어야 한다");
    let json = serde_json::to_string_pretty(&context).expect("컨텍스트를 직렬화해야 한다");

    assert_eq!(context.schema_version, "codex-semantic-context.v1");
    assert_eq!(context.source_analysis_id, result.analysis_id);
    assert!(!json.contains("staticGraph"));
    assert!(!json.contains("semanticAnalysis"));
    assert!(context
        .domains
        .iter()
        .flat_map(|domain| domain.features.iter())
        .all(|feature| feature
            .flow_ids
            .iter()
            .all(|flow_id| domain_flow_ids(&context, flow_id))));

    fs::remove_dir_all(root).expect("임시 프로젝트를 정리해야 한다");
}

#[test]
fn overview가_없는_결과는_조용히_빈_컨텍스트가_되지_않는다() {
    let result = code_analysis_engine::AnalysisResult {
        schema_version: "analysis-result.v1".into(),
        analysis_id: "analysis_1".into(),
        status: code_analysis_engine::model::AnalysisStatus::Ready,
        project: code_analysis_engine::model::ProjectContext {
            project_id: "project_1".into(),
            root_path: ".".into(),
            snapshot_id: "snapshot_1".into(),
        },
        files: Vec::new(),
        summary: Default::default(),
        diagnostics: Vec::new(),
        elapsed_ms: 0,
        overview: None,
        preprocessed_overview: None,
    };

    assert!(matches!(
        build_codex_context(&result, &AnalysisConfig::default()),
        Err(PostprocessError::MissingOverview)
    ));
}

fn domain_flow_ids(
    context: &code_analysis_engine::postprocess::CodexSemanticContext,
    flow_id: &str,
) -> bool {
    context
        .domains
        .iter()
        .flat_map(|domain| domain.flows.iter())
        .any(|flow| flow.id == flow_id)
}
