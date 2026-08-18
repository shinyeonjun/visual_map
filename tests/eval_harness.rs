use code_analysis_engine::domain::DomainKind;
use code_analysis_engine::eval::{
    classify_outcome, load_catalog, load_gold, resolve_clean_dir, score_gold, EvalMode,
    EvalOutcome, EvalSnapshot, SnapDomain,
};
use std::collections::HashMap;
use std::path::PathBuf;

fn eval_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/eval")
}

fn catalog_path() -> PathBuf {
    eval_root().join("catalog.json")
}

fn gold_path(relative: &str) -> PathBuf {
    eval_root().join(relative)
}

#[test]
fn catalog_json은_25개_gold를_로드한다() {
    let golds = load_catalog(&catalog_path()).expect("catalog를 읽어야 한다");
    assert_eq!(golds.len(), 25);
    assert!(golds.iter().any(|gold| gold.id == "nestjs-boilerplate"));
    assert!(golds.iter().any(|gold| gold.id == "ghost"));
    assert!(golds.iter().any(|gold| gold.id == "c-curl"));
}

#[test]
fn library_cli_gold는_negative_only_통과_정의를_쓴다() {
    let gold = load_gold(&gold_path("library-cli/c-curl/gold.json")).expect("c-curl gold");
    assert_eq!(gold.mode, EvalMode::Library);
    assert!(gold.must_have_domains.is_empty());
    assert!(gold.must_have_features.is_empty());

    let report = score_gold(
        &gold,
        &EvalSnapshot {
            domains: Vec::new(),
            features: Vec::new(),
            flows: HashMap::new(),
        },
    );
    assert!(report.passed);
    assert_eq!(report.outcome, EvalOutcome::PassNegativeOnly);
}

#[test]
fn simplebank_gold는_빈_스냅샷에서_필수_domain을_누락으로_채점한다() {
    let gold = load_gold(&gold_path("service/simplebank/gold.json")).expect("simplebank gold");
    let report = score_gold(
        &gold,
        &EvalSnapshot {
            domains: Vec::new(),
            features: Vec::new(),
            flows: HashMap::new(),
        },
    );
    assert!(!report.passed);
    assert_eq!(report.outcome, EvalOutcome::Fail);
    assert_eq!(report.domain_hits, 0);
    assert!(report
        .findings
        .iter()
        .any(|item| item.kind == "missing" && item.layer == "domain"));
    assert!(report
        .findings
        .iter()
        .any(|item| item.kind == "missing" && item.layer == "feature"));
}

#[test]
fn c_curl_library_gold는_domain이_없으면_통과하고_src_카드면_실패한다() {
    let gold = load_gold(&gold_path("library-cli/c-curl/gold.json")).expect("c-curl gold");
    let empty = score_gold(
        &gold,
        &EvalSnapshot {
            domains: Vec::new(),
            features: Vec::new(),
            flows: HashMap::new(),
        },
    );
    assert!(empty.passed, "{:?}", empty.findings);
    assert_eq!(empty.outcome, EvalOutcome::PassNegativeOnly);

    let folder = score_gold(
        &gold,
        &EvalSnapshot {
            domains: vec![SnapDomain {
                id: "d-src".into(),
                key: "src".into(),
                kind: DomainKind::Unknown,
            }],
            features: Vec::new(),
            flows: HashMap::new(),
        },
    );
    assert_eq!(folder.outcome, EvalOutcome::Fail);
    assert!(folder.findings.iter().any(|item| item.kind == "folderName"));
}

#[test]
fn classify_outcome은_서비스_통과와_라이브러리_negative_only를_구분한다() {
    let service = load_gold(&gold_path("service/nestjs-boilerplate/gold.json")).unwrap();
    let library = load_gold(&gold_path("library-cli/c-curl/gold.json")).unwrap();

    assert_eq!(classify_outcome(&service, true), EvalOutcome::PassPositive);
    assert_eq!(
        classify_outcome(&library, true),
        EvalOutcome::PassNegativeOnly
    );
}

#[test]
fn resolve_clean_dir은_clean_하위_번들을_찾는다() {
    let gold = load_gold(&gold_path("service/simplebank/gold.json")).unwrap();
    let runs = eval_root().join("runs/service");
    if !runs.is_dir() {
        return;
    }

    let resolved = resolve_clean_dir(&runs, &gold);
    assert!(
        resolved.ends_with("simplebank/clean"),
        "unexpected path: {}",
        resolved.display()
    );
    assert!(resolved.join("manifest.json").is_file());
}
