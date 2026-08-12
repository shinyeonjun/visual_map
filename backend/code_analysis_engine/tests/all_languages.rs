use code_analysis_engine::{analyze, AnalysisRequest};
use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn temporary_project() -> PathBuf {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("visual-map-all-languages-{suffix}"));
    fs::create_dir_all(&path).expect("임시 프로젝트 디렉터리를 만들어야 한다");
    path
}

#[test]
fn 지원하는_열_개_언어가_공통_facts로_변환된다() {
    let root = temporary_project();
    let sources = [
        (
            "main.js",
            "function javascriptMain() { javascriptHelper(); }\nfunction javascriptHelper() {}\n",
        ),
        (
            "main.ts",
            "function typescriptMain() { typescriptHelper(); }\nfunction typescriptHelper() {}\n",
        ),
        (
            "main.py",
            "def python_main():\n    python_helper()\n\ndef python_helper():\n    return 1\n",
        ),
        (
            "Main.java",
            "class Main { void javaMain() { javaHelper(); } void javaHelper() {} }\n",
        ),
        (
            "main.c",
            "void c_helper() {}\nint c_main() { c_helper(); return 1; }\n",
        ),
        (
            "main.cpp",
            "void cpp_helper() {}\nint cpp_main() { cpp_helper(); return 1; }\n",
        ),
        (
            "Main.cs",
            "class Main { void CSharpMain() { CSharpHelper(); } void CSharpHelper() {} }\n",
        ),
        (
            "main.go",
            "package main\nfunc goMain() int { goHelper(); return 1 }\nfunc goHelper() {}\n",
        ),
        (
            "main.rs",
            "fn rust_main() -> i32 { rust_helper(); 1 }\nfn rust_helper() {}\n",
        ),
        (
            "main.dart",
            "void dartMain() { dartHelper(); }\nvoid dartHelper() {}\n",
        ),
    ];
    for (file, source) in sources {
        fs::write(root.join(file), source).expect("언어 fixture를 써야 한다");
    }

    let result = analyze(AnalysisRequest::new(&root)).expect("전체 언어 분석이 성공해야 한다");
    let overview = result.overview.expect("Overview가 생성되어야 한다");
    let languages: BTreeSet<_> = overview.coverage.languages.keys().cloned().collect();
    let expected: BTreeSet<_> = [
        "javascript",
        "typescript",
        "python",
        "java",
        "c",
        "cpp",
        "csharp",
        "go",
        "rust",
        "dart",
    ]
    .into_iter()
    .map(str::to_string)
    .collect();

    assert_eq!(languages, expected);
    assert_eq!(result.summary.total_files, 10);
    assert_eq!(result.summary.languages.len(), 10);

    for helper in [
        "javascriptHelper",
        "typescriptHelper",
        "python_helper",
        "javaHelper",
        "c_helper",
        "cpp_helper",
        "CSharpHelper",
        "goHelper",
        "rust_helper",
        "dartHelper",
    ] {
        assert!(
            overview.units.iter().any(|unit| unit.name == helper),
            "함수 유닛이 없어: {helper}"
        );
        assert!(
            overview
                .static_graph
                .edges
                .iter()
                .any(|edge| edge.target_name == helper),
            "호출 reference가 없어: {helper}"
        );
    }

    for entry in [
        "javascriptMain",
        "typescriptMain",
        "python_main",
        "javaMain",
        "c_main",
        "cpp_main",
        "CSharpMain",
        "goMain",
        "rust_main",
        "dartMain",
    ] {
        let unit_id = overview
            .units
            .iter()
            .find(|unit| unit.name == entry)
            .map(|unit| unit.id.as_str())
            .unwrap_or_else(|| panic!("진입 함수 유닛이 없어: {entry}"));
        assert!(
            overview
                .execution_flows
                .flows
                .iter()
                .any(|flow| flow.owner_unit_id == unit_id),
            "실행 흐름이 없어: {entry}"
        );
    }

    fs::remove_dir_all(root).expect("임시 프로젝트를 정리해야 한다");
}
