use code_analysis_engine::{analyze, model::AnalysisStatus, AnalysisRequest};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn temporary_project(name: &str) -> PathBuf {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("visual-map-quality-{name}-{suffix}"));
    fs::create_dir_all(&path).expect("임시 프로젝트를 만들어야 한다");
    path
}

#[test]
fn 잘못된_문법은_크래시하지_않고_parse_error를_남긴다() {
    let root = temporary_project("parse-error");
    fs::write(root.join("broken.py"), "def broken(:\n    return 1\n")
        .expect("깨진 Python 파일을 써야 한다");

    let result = analyze(AnalysisRequest::new(&root)).expect("부분 분석 결과를 반환해야 한다");

    assert_eq!(result.status, AnalysisStatus::Partial);
    assert!(result
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "PARSE_ERROR"));
    assert!(result
        .files
        .iter()
        .any(|file| file.relative_path == "broken.py"));

    fs::remove_dir_all(root).expect("임시 프로젝트를 정리해야 한다");
}

#[test]
fn 참조_target_name은_한줄_이름_불변식을_지킨다() {
    let root = temporary_project("target-name-invariant");
    fs::write(
        root.join("main.ts"),
        r#"
export function main() {
  return createValue("a", { nested: true });
}
export async function createValue(first: string, second: object) {
  return Promise.resolve(second);
}
export { createValue as valueFactory };
"#,
    )
    .expect("TypeScript 파일을 써야 한다");

    let result = analyze(AnalysisRequest::new(&root)).expect("분석이 성공해야 한다");
    let overview = result.overview.expect("Overview가 있어야 한다");
    for reference in &overview.static_graph.edges {
        assert!(!reference.target_name.contains(['\r', '\n']));
        assert!(reference.target_name.chars().count() <= 256);
    }

    fs::remove_dir_all(root).expect("임시 프로젝트를 정리해야 한다");
}

#[test]
fn 파일_한도에_도달하면_부분결과와_진단을_반환한다() {
    let root = temporary_project("limits");
    fs::write(root.join("a.ts"), "export function a() { return 1; }\n")
        .expect("첫 파일을 써야 한다");
    fs::write(root.join("b.ts"), "export function b() { return 2; }\n")
        .expect("두 번째 파일을 써야 한다");

    let mut request = AnalysisRequest::new(&root);
    request.options.config.limits.max_files = 1;
    let result = analyze(request).expect("한도에 도달해도 부분 결과를 반환해야 한다");

    assert_eq!(result.status, AnalysisStatus::Partial);
    assert_eq!(result.files.len(), 1);
    assert!(result
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "ANALYSIS_LIMIT_REACHED"));

    fs::remove_dir_all(root).expect("임시 프로젝트를 정리해야 한다");
}

#[test]
fn 파일간_호출이_많은_프로젝트도_overview를_완성한다() {
    let root = temporary_project("cross-file-scale");
    const FILE_COUNT: usize = 120;
    const FUNCTIONS_PER_FILE: usize = 12;

    for file_index in 0..FILE_COUNT {
        let mut source = format!("export function main() {{ return f_{file_index}_0(); }}\n");
        for function_index in 0..FUNCTIONS_PER_FILE {
            let target_file = (file_index + 1) % FILE_COUNT;
            let target = format!("f_{target_file}_{function_index}");
            source.push_str(&format!(
                "export function f_{file_index}_{function_index}() {{ return {target}(); }}\n"
            ));
        }
        fs::write(root.join(format!("module_{file_index}.ts")), source)
            .expect("스케일 fixture를 써야 한다");
    }

    let result = analyze(AnalysisRequest::new(&root)).expect("대형 결합 fixture를 분석해야 한다");
    let overview = result.overview.expect("Overview가 있어야 한다");
    assert_eq!(overview.coverage.total_files, FILE_COUNT);
    assert!(overview.coverage.total_entrypoints >= FILE_COUNT);
    assert!(!overview.features.is_empty());
    assert!(overview.features.iter().all(|feature| feature
        .unit_ids
        .iter()
        .all(|unit_id| { overview.units.iter().any(|unit| &unit.id == unit_id) })));

    fs::remove_dir_all(root).expect("임시 프로젝트를 정리해야 한다");
}
