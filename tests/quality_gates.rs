use code_analysis_engine::{analyze, model::AnalysisStatus, AnalysisRequest};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

fn temporary_project(name: &str) -> PathBuf {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("visual-map-quality-{name}-{suffix}"));
    fs::create_dir_all(&path).expect("임시 프로젝트를 만들어야 한다");
    path
}

fn assert_elapsed_within(name: &str, elapsed: Duration, default_max_ms: u128) {
    let configured_max_ms = std::env::var("VISUAL_MAP_SCALE_MAX_MS")
        .ok()
        .and_then(|value| value.parse::<u128>().ok())
        .unwrap_or(default_max_ms);
    eprintln!(
        "[scale-gate] fixture={name} elapsed_ms={} max_ms={configured_max_ms}",
        elapsed.as_millis()
    );
    assert!(
        elapsed.as_millis() <= configured_max_ms,
        "{name} fixture가 시간 한도를 초과했다: {}ms > {configured_max_ms}ms",
        elapsed.as_millis()
    );
}

fn write_cross_file_fixture(root: &Path, file_count: usize, functions_per_file: usize) {
    for file_index in 0..file_count {
        let target_file = (file_index + 1) % file_count;
        let imported_names = (0..functions_per_file)
            .map(|function_index| format!("f_{target_file}_{function_index}"))
            .collect::<Vec<_>>()
            .join(", ");
        let mut source =
            format!("import {{ {imported_names} }} from \"./module_{target_file}\";\n");
        source.push_str(&format!(
            "export function main() {{ return f_{file_index}_0(); }}\n"
        ));
        for function_index in 0..functions_per_file {
            let target = format!("f_{target_file}_{function_index}");
            source.push_str(&format!(
                "export function f_{file_index}_{function_index}() {{ return {target}(); }}\n"
            ));
        }
        fs::write(root.join(format!("module_{file_index}.ts")), source)
            .expect("스케일 fixture를 써야 한다");
    }
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
    for reference in overview.static_graph.edges.iter() {
        assert!(!reference.target_name.contains(['\r', '\n']));
        assert!(reference.target_name.chars().count() <= 256);
    }

    fs::remove_dir_all(root).expect("임시 프로젝트를 정리해야 한다");
}

#[test]
fn 일반_문자열의_route와_ecma_main은_진입점으로_오인하지_않는다() {
    let root = temporary_project("entrypoint-negative");
    fs::write(
        root.join("plain.ts"),
        r#"
const documentation = "app.get('/not-a-route')";
export function main() { return documentation; }
"#,
    )
    .expect("음성 TypeScript fixture를 써야 한다");
    fs::write(root.join("real.c"), "int main(void) { return 0; }\n")
        .expect("C main fixture를 써야 한다");

    let overview = analyze(AnalysisRequest::new(&root))
        .expect("분석이 성공해야 한다")
        .overview
        .expect("Overview가 있어야 한다");

    assert!(!overview.entrypoints.iter().any(|entrypoint| {
        entrypoint.kind == code_analysis_engine::facts::EntrypointKind::Http
            && entrypoint.path.as_deref() == Some("/not-a-route")
    }));
    assert!(!overview.entrypoints.iter().any(|entrypoint| {
        entrypoint.kind == code_analysis_engine::facts::EntrypointKind::Main
            && overview
                .units
                .iter()
                .find(|unit| unit.id == entrypoint.unit_id)
                .is_some_and(|unit| unit.relative_path == "plain.ts")
    }));
    assert!(overview.entrypoints.iter().any(|entrypoint| {
        entrypoint.kind == code_analysis_engine::facts::EntrypointKind::Main
            && overview
                .units
                .iter()
                .find(|unit| unit.id == entrypoint.unit_id)
                .is_some_and(|unit| unit.relative_path == "real.c")
    }));

    fs::remove_dir_all(root).expect("음성 fixture를 정리해야 한다");
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
    write_cross_file_fixture(&root, FILE_COUNT, FUNCTIONS_PER_FILE);

    let started = Instant::now();
    let result = analyze(AnalysisRequest::new(&root)).expect("대형 결합 fixture를 분석해야 한다");
    assert_elapsed_within("cross-file-small-scale", started.elapsed(), 5_000);
    let overview = result.overview.expect("Overview가 있어야 한다");
    assert_eq!(overview.coverage.total_files, FILE_COUNT);
    assert!(!overview.features.is_empty());
    assert!(overview.features.iter().all(|feature| feature
        .unit_ids
        .iter()
        .all(|unit_id| { overview.units.iter().any(|unit| &unit.id == unit_id) })));

    fs::remove_dir_all(root).expect("임시 프로젝트를 정리해야 한다");
}

#[test]
fn 확정된_참조는_존재하는_동일_언어_유닛만_가리킨다() {
    let root = temporary_project("confirmed-edge-integrity");
    fs::write(
        root.join("service.ts"),
        "export function run() { return helper(); }\nfunction helper() { return true; }\n",
    )
    .expect("참조 무결성 fixture를 써야 한다");

    let overview = analyze(AnalysisRequest::new(&root))
        .expect("참조 무결성 fixture를 분석해야 한다")
        .overview
        .expect("Overview가 생성되어야 한다");
    for reference in overview.static_graph.edges.iter().filter(|reference| {
        reference.status == code_analysis_engine::facts::ResolutionStatus::Confirmed
    }) {
        let target_id = reference
            .target_unit_id
            .as_deref()
            .expect("확정 참조에는 targetUnitId가 있어야 한다");
        let source = overview
            .units
            .iter()
            .find(|unit| unit.id == reference.source_unit_id)
            .expect("확정 참조의 source unit이 있어야 한다");
        let target = overview
            .units
            .iter()
            .find(|unit| unit.id == target_id)
            .expect("확정 참조의 target unit이 있어야 한다");
        assert_eq!(source.language, target.language);
    }

    fs::remove_dir_all(root).expect("참조 무결성 fixture를 정리해야 한다");
}

#[test]
#[ignore = "Orange Pi 야간 성능 게이트에서 release 모드로 실행한다"]
fn 대규모_파일간_호출은_시간_한도안에_overview를_완성한다() {
    let root = temporary_project("cross-file-large-scale");
    const FILE_COUNT: usize = 400;
    const FUNCTIONS_PER_FILE: usize = 50;
    write_cross_file_fixture(&root, FILE_COUNT, FUNCTIONS_PER_FILE);

    let started = Instant::now();
    let mut request = AnalysisRequest::new(&root);
    request.options.profile = true;
    let result = analyze(request).expect("대형 결합 fixture를 분석해야 한다");
    let elapsed = started.elapsed();
    let overview = result.overview.expect("Overview가 생성되어야 한다");

    assert_eq!(overview.coverage.total_files, FILE_COUNT);
    assert!(!overview.features.is_empty());
    assert!(overview
        .features
        .iter()
        .any(|feature| feature.reachable_unit_count > feature.unit_ids.len()));
    assert_elapsed_within("cross-file-large-scale", elapsed, 30_000);
    fs::remove_dir_all(root).expect("대형 fixture를 정리해야 한다");
}

#[test]
#[ignore = "Orange Pi 야간 성능 게이트에서 release 모드로 실행한다"]
fn 단일_대형_파일은_시간_한도안에_분석한다() {
    let root = temporary_project("single-file-large-scale");
    const FUNCTION_COUNT: usize = 20_000;
    let mut source = String::from("export function main() { return helper_0(); }\n");
    for index in 0..FUNCTION_COUNT {
        let next = (index + 1) % FUNCTION_COUNT;
        source.push_str(&format!(
            "export function helper_{index}() {{ return helper_{next}(); }}\n"
        ));
    }
    fs::write(root.join("large.ts"), source).expect("대형 단일 파일을 써야 한다");

    let started = Instant::now();
    let mut request = AnalysisRequest::new(&root);
    request.options.profile = true;
    let result = analyze(request).expect("대형 단일 파일을 분석해야 한다");
    let elapsed = started.elapsed();
    assert!(result.overview.is_some());
    assert_elapsed_within("single-file-large-scale", elapsed, 120_000);
    fs::remove_dir_all(root).expect("대형 fixture를 정리해야 한다");
}
