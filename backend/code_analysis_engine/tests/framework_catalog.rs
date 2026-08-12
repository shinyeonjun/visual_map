use code_analysis_engine::frameworks::registry::catalog::supported_frameworks;
use code_analysis_engine::{analyze, AnalysisRequest};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn temporary_project(name: &str) -> PathBuf {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("visual-map-framework-catalog-{name}-{suffix}"));
    fs::create_dir_all(&path).expect("임시 프로젝트를 만들어야 한다");
    path
}

fn extension(language: &str) -> &'static str {
    match language {
        "javascript" => "js",
        "typescript" => "ts",
        "python" => "py",
        "java" => "java",
        "c" => "c",
        "cpp" => "cpp",
        "csharp" => "cs",
        "go" => "go",
        "rust" => "rs",
        "dart" => "dart",
        other => panic!("지원 언어가 아닌 framework catalog 언어: {other}"),
    }
}

#[test]
fn registry의_모든_프레임워크가_지원언어에서_감지된다() {
    for spec in supported_frameworks() {
        let root = temporary_project(&spec.id.replace('.', "-"));
        let language = spec
            .languages
            .first()
            .expect("framework는 하나 이상의 언어를 가져야 한다");
        let file_name = format!("marker.{}", extension(language));
        let source = format!("// Visual Map framework marker: {}\n", spec.markers[0]);
        fs::write(root.join(file_name), source).expect("framework marker fixture를 써야 한다");

        let result = analyze(AnalysisRequest::new(&root))
            .unwrap_or_else(|error| panic!("{} 분석이 실패했다: {error}", spec.id));
        let overview = result
            .overview
            .unwrap_or_else(|| panic!("{} Overview가 없다", spec.id));
        assert!(
            overview
                .detected_frameworks
                .iter()
                .any(|framework| framework.id == spec.id),
            "registry framework가 감지되지 않았다: {}",
            spec.id
        );

        fs::remove_dir_all(root).expect("framework marker fixture를 정리해야 한다");
    }
}
