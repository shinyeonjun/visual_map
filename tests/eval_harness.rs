use code_analysis_engine::eval::{
    load_catalog, load_gold, score_gold, EvalMode, EvalSnapshot, SnapDomain,
};
use code_analysis_engine::domain::DomainKind;
use std::collections::HashMap;
use std::path::PathBuf;

fn gold_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/eval/gold")
}

#[test]
fn catalog_json을_읽고_known과_lab_정답을_모두_로드한다() {
    let golds = load_catalog(&gold_dir()).expect("catalog를 읽어야 한다");
    let ids: Vec<_> = golds.iter().map(|gold| gold.id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "meeting-overlay-assistant",
            "ai-schedule-web",
            "simplebank",
            "my-fastest-drogon-app-cpp",
            "c-curl"
        ]
    );
    let curl = golds.iter().find(|gold| gold.id == "c-curl").unwrap();
    assert_eq!(curl.mode, EvalMode::Library);
    assert!(curl.must_have_domains.is_empty());
}

#[test]
fn simplebank_gold는_빈_스냅샷에서_필수_domain을_누락으로_채점한다() {
    let gold = load_gold(&gold_dir().join("lab/simplebank.json")).expect("simplebank gold");
    let report = score_gold(
        &gold,
        &EvalSnapshot {
            domains: Vec::new(),
            features: Vec::new(),
            flows: HashMap::new(),
        },
    );
    assert!(!report.passed);
    assert_eq!(report.domain_hits, 0);
    assert!(report.findings.iter().any(|item| item.kind == "missing" && item.layer == "domain"));
    assert!(report.findings.iter().any(|item| item.kind == "missing" && item.layer == "feature"));
}

#[test]
fn c_curl_library_gold는_domain이_없으면_통과하고_src_카드면_실패한다() {
    let gold = load_gold(&gold_dir().join("lab/c-curl.json")).expect("c-curl gold");
    let empty = score_gold(
        &gold,
        &EvalSnapshot {
            domains: Vec::new(),
            features: Vec::new(),
            flows: HashMap::new(),
        },
    );
    assert!(empty.passed, "{:?}", empty.findings);

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
    assert!(folder.findings.iter().any(|item| item.kind == "folderName"));
}
