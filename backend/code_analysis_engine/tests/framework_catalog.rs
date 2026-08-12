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
        // marker가 주석에만 존재해도 감지되는 회귀를 막기 위해 실제
        // import 문맥으로 fixture를 만든다.
        let source = format!("import {}\n", spec.markers[0]);
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

#[test]
fn 주석과_문자열만으로는_프레임워크를_감지하지_않는다() {
    let root = temporary_project("false-positive");
    fs::write(
        root.join("plain.js"),
        r#"
// React is mentioned in documentation only.
const note = "React is not used here";
function plain() { return note; }
"#,
    )
    .expect("음성 framework fixture를 써야 한다");

    let overview = analyze(AnalysisRequest::new(&root))
        .expect("분석이 성공해야 한다")
        .overview
        .expect("Overview가 생성되어야 한다");
    assert!(!overview
        .detected_frameworks
        .iter()
        .any(|framework| framework.id == "javascript.react"));

    fs::remove_dir_all(root).expect("음성 fixture를 정리해야 한다");
}

#[test]
fn 의존성_토큰_경계와_중첩_manifest를_보존한다() {
    let root = temporary_project("nested-manifest");
    fs::create_dir_all(root.join("packages/web")).expect("중첩 package 디렉터리를 만들어야 한다");
    fs::create_dir_all(root.join("packages/web/src"))
        .expect("중첩 source 디렉터리를 만들어야 한다");
    fs::write(
        root.join("packages/web/package.json"),
        r#"{
  "name": "web",
  "dependencies": { "react": "^19.0.0" }
}"#,
    )
    .expect("중첩 package manifest를 써야 한다");
    fs::write(
        root.join("packages/web/src/index.js"),
        "const preactValue = 1; export { preactValue };\n",
    )
    .expect("중첩 source를 써야 한다");

    let overview = analyze(AnalysisRequest::new(&root))
        .expect("중첩 manifest 분석이 성공해야 한다")
        .overview
        .expect("Overview가 생성되어야 한다");
    let react = overview
        .detected_frameworks
        .iter()
        .find(|framework| framework.id == "javascript.react")
        .expect("중첩 package dependency에서 React를 감지해야 한다");
    assert!(react
        .evidence
        .iter()
        .any(|evidence| evidence.span.relative_path == "packages/web/package.json"));

    fs::remove_dir_all(root).expect("중첩 manifest fixture를 정리해야 한다");

    let root = temporary_project("identifier-boundary");
    fs::write(
        root.join("preact.js"),
        "const preactValue = 1; export { preactValue };\n",
    )
    .expect("유사 식별자 fixture를 써야 한다");
    let overview = analyze(AnalysisRequest::new(&root))
        .expect("유사 식별자 분석이 성공해야 한다")
        .overview
        .expect("Overview가 생성되어야 한다");
    assert!(!overview
        .detected_frameworks
        .iter()
        .any(|framework| framework.id == "javascript.react"));
    fs::remove_dir_all(root).expect("유사 식별자 fixture를 정리해야 한다");
}
