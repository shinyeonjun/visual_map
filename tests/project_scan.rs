use code_analysis_engine::{analyze, model::AnalysisStatus, AnalysisRequest};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn temporary_project() -> PathBuf {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("visual-map-engine-test-{suffix}"));
    fs::create_dir_all(&path).expect("임시 프로젝트 디렉터리를 만들어야 한다");
    path
}

#[test]
fn 프로젝트를_스캔하고_파일_메타데이터를_반환한다() {
    let root = temporary_project();
    fs::create_dir_all(root.join("src")).expect("src 디렉터리를 만들어야 한다");
    fs::create_dir_all(root.join("node_modules")).expect("무시 디렉터리를 만들어야 한다");
    fs::write(root.join("src/main.ts"), "export const answer = 42;\n")
        .expect("TypeScript 파일을 써야 한다");
    fs::write(root.join("src/helper.py"), "print('hello')").expect("Python 파일을 써야 한다");
    fs::write(root.join("node_modules/vendor.js"), "ignored();\n").expect("무시 파일을 써야 한다");
    fs::write(root.join("README.md"), "문서").expect("문서 파일을 써야 한다");

    let result = analyze(AnalysisRequest::new(&root)).expect("분석이 성공해야 한다");

    assert_eq!(result.status, AnalysisStatus::Ready);
    assert_eq!(result.summary.total_files, 2);
    assert_eq!(result.summary.languages["typescript"], 1);
    assert_eq!(result.summary.languages["python"], 1);
    assert_eq!(result.files[0].relative_path, "src/helper.py");
    assert!(result.files.iter().all(|file| file.content_hash.is_some()));
    assert!(result.project.snapshot_id.starts_with("snapshot_"));

    fs::remove_dir_all(root).expect("임시 프로젝트를 정리해야 한다");
}

#[test]
fn 다양한_언어의_테스트_파일명_규칙을_판별한다() {
    let root = temporary_project();
    fs::create_dir_all(root.join("__tests__/nested")).expect("테스트 디렉터리를 만들어야 한다");
    for path in [
        "test_prefix.py",
        "AlphaTests.java",
        "__tests__/nested/widget.test.jsx",
        "__tests__/nested/widget.spec.tsx",
    ] {
        let path = root.join(path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("fixture 부모 디렉터리를 만들어야 한다");
        }
        fs::write(path, "function testFixture() {}\n").expect("테스트 fixture를 써야 한다");
    }

    let output = analyze(AnalysisRequest::new(&root)).expect("프로젝트를 스캔해야 한다");
    assert!(output.files.iter().all(|file| file.is_test));

    fs::remove_dir_all(root).expect("fixture를 정리해야 한다");
}

#[test]
fn 해시를_끄면_파일_내용은_반환하지_않는다() {
    let root = temporary_project();
    fs::write(root.join("main.rs"), "fn main() {}\n").expect("Rust 파일을 써야 한다");

    let mut request = AnalysisRequest::new(&root);
    request.options.config.scan.compute_hashes = false;
    let result = analyze(request).expect("분석이 성공해야 한다");

    assert_eq!(result.summary.total_files, 1);
    assert!(result.files[0].content_hash.is_none());
    assert!(result.project.snapshot_id.starts_with("snapshot_"));

    fs::remove_dir_all(root).expect("임시 프로젝트를 정리해야 한다");
}
