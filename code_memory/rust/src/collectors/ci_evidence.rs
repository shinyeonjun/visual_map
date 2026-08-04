use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use super::discovery::{find_files, read_descriptor, relative_path, stable_segment};
use super::model::{
    properties, CollectedEvidence, CollectedFact, CollectedRelation, CollectionDiagnostic,
    CollectionMode, CollectionStatus, CollectorResult, TruthClass,
};

const ID: &str = "ci-evidence";

pub(crate) fn collect(root: &Path) -> CollectorResult {
    let mut result = CollectorResult::new(ID, "verification-evidence", CollectionMode::Passive);
    let mut files = find_files(root, is_supported_artifact);
    let latest_verification = root
        .join(".code_memory")
        .join("evidence")
        .join("verification-run.json");
    if latest_verification.is_file()
        && std::fs::symlink_metadata(&latest_verification)
            .is_ok_and(|metadata| !metadata.file_type().is_symlink())
    {
        files.push(latest_verification);
    }
    files.sort();
    files.dedup();
    if files.is_empty() {
        return result;
    }
    for file in files {
        let path = relative_path(root, &file);
        result.summary.detected_by.push(path.clone());
        let source = match read_descriptor(&file) {
            Ok(source) => source,
            Err(message) => {
                result.diagnostics.push(diagnostic(path, message));
                continue;
            }
        };
        let before = result.facts.len();
        let parsed = if path.to_ascii_lowercase().ends_with("verification-run.json") {
            parse_verification(&path, &source, &mut result)
        } else if path.to_ascii_lowercase().ends_with(".sarif")
            || path.to_ascii_lowercase().ends_with(".sarif.json")
        {
            parse_sarif(&path, &source, &mut result)
        } else if is_lcov(&path) {
            parse_lcov(&path, &source, &mut result)
        } else {
            parse_junit(&path, &source, &mut result)
        };
        if !parsed {
            result.facts.truncate(before);
            result.diagnostics.push(diagnostic(
                path,
                "artifact name matched but its supported evidence structure was absent".to_string(),
            ));
        }
    }
    result.summary.detected_by.sort();
    result.summary.detected_by.dedup();
    result.summary.status = if result.facts.is_empty() {
        CollectionStatus::Failed
    } else if result.diagnostics.is_empty() {
        CollectionStatus::Collected
    } else {
        CollectionStatus::Partial
    };
    result
}

fn is_supported_artifact(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    name.ends_with(".sarif")
        || name.ends_with(".sarif.json")
        || name == "verification-run.json"
        || matches!(name.as_str(), "lcov.info" | "coverage.info")
        || ((name.ends_with(".xml"))
            && (name.contains("junit")
                || name.contains("test-result")
                || name.starts_with("test-")))
}

fn parse_verification(path: &str, source: &str, result: &mut CollectorResult) -> bool {
    let value: Value = match serde_json::from_str(source) {
        Ok(value) => value,
        Err(error) => {
            result.diagnostics.push(diagnostic(
                path.to_string(),
                format!("invalid verification JSON: {error}"),
            ));
            return false;
        }
    };
    if value.get("schema").and_then(Value::as_str) != Some("code-memory.verification-run.v1") {
        return false;
    }
    let label = value
        .get("label")
        .and_then(Value::as_str)
        .unwrap_or("verification");
    let tool = value.get("tool").and_then(Value::as_str);
    let status = value.get("status").and_then(Value::as_str);
    let duration = value.get("duration_ms").map(Value::to_string);
    let exit_code = value.get("exit_code").map(Value::to_string);
    result.facts.push(CollectedFact {
        stable_key: format!("ci:verification:{}", stable_segment(path)),
        kind: "verification-run".to_string(),
        name: label.to_string(),
        path: Some(path.to_string()),
        properties: properties(&[
            ("tool", tool),
            ("status", status),
            ("duration_ms", duration.as_deref()),
            ("exit_code", exit_code.as_deref()),
        ]),
    });
    true
}

fn is_lcov(path: &str) -> bool {
    let name = Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    matches!(name.as_str(), "lcov.info" | "coverage.info")
}

fn parse_sarif(path: &str, source: &str, result: &mut CollectorResult) -> bool {
    let value: Value = match serde_json::from_str(source) {
        Ok(value) => value,
        Err(error) => {
            result.diagnostics.push(diagnostic(
                path.to_string(),
                format!("invalid SARIF JSON: {error}"),
            ));
            return false;
        }
    };
    let Some(runs) = value.get("runs").and_then(Value::as_array) else {
        return false;
    };
    let report = report_fact(path, "sarif");
    let report_key = report.stable_key.clone();
    result.facts.push(report);
    let mut groups: HashMap<(String, String, String), SarifGroup> = HashMap::new();
    for run in runs {
        for item in run
            .get("results")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let rule = item
                .get("ruleId")
                .and_then(Value::as_str)
                .unwrap_or("unclassified");
            let level = item
                .get("level")
                .and_then(Value::as_str)
                .unwrap_or("warning");
            let location = item
                .pointer("/locations/0/physicalLocation/artifactLocation/uri")
                .and_then(Value::as_str)
                .unwrap_or("");
            let line = item
                .pointer("/locations/0/physicalLocation/region/startLine")
                .and_then(Value::as_u64)
                .and_then(|line| u32::try_from(line).ok());
            let message = item
                .pointer("/message/text")
                .and_then(Value::as_str)
                .unwrap_or("");
            let group = groups
                .entry((rule.to_string(), level.to_string(), location.to_string()))
                .or_default();
            group.count += 1;
            group.line = group.line.or(line);
            if group.message.is_empty() {
                group.message = message.to_string();
            }
        }
    }
    for ((rule, level, location), group) in groups {
        let key = format!(
            "ci:sarif:{}:{}:{}:{}",
            stable_segment(path),
            stable_segment(&rule),
            stable_segment(&level),
            stable_segment(&location)
        );
        result.facts.push(CollectedFact {
            stable_key: key.clone(),
            kind: "static-analysis-result".to_string(),
            name: rule.clone(),
            path: (!location.is_empty()).then_some(location.clone()),
            properties: properties(&[
                ("rule", Some(&rule)),
                ("level", Some(&level)),
                ("count", Some(&group.count.to_string())),
                (
                    "first_message",
                    (!group.message.is_empty()).then_some(group.message.as_str()),
                ),
            ]),
        });
        result.relations.push(contains(
            &report_key,
            &key,
            path,
            group.line,
            (!group.message.is_empty()).then_some(group.message),
        ));
    }
    true
}

#[derive(Default)]
struct SarifGroup {
    count: usize,
    line: Option<u32>,
    message: String,
}

fn parse_lcov(path: &str, source: &str, result: &mut CollectorResult) -> bool {
    let report = report_fact(path, "lcov");
    let report_key = report.stable_key.clone();
    result.facts.push(report);
    let mut current: Option<LcovFile> = None;
    let mut found = false;
    for line in source.lines() {
        if let Some(file) = line.strip_prefix("SF:") {
            if let Some(previous) = current.take() {
                emit_lcov(path, &report_key, previous, result);
            }
            current = Some(LcovFile {
                path: file.trim().replace('\\', "/"),
                ..LcovFile::default()
            });
            found = true;
        } else if let Some(record) = line.strip_prefix("DA:") {
            let mut fields = record.split(',');
            let _line = fields.next();
            if let (Some(file), Some(hits)) = (&mut current, fields.next()) {
                file.found += 1;
                if hits.parse::<u64>().unwrap_or(0) > 0 {
                    file.hit += 1;
                }
            }
        } else if line == "end_of_record" {
            if let Some(file) = current.take() {
                emit_lcov(path, &report_key, file, result);
            }
        }
    }
    if let Some(file) = current {
        emit_lcov(path, &report_key, file, result);
    }
    found
}

#[derive(Default)]
struct LcovFile {
    path: String,
    found: usize,
    hit: usize,
}

fn emit_lcov(path: &str, report: &str, file: LcovFile, result: &mut CollectorResult) {
    let key = format!(
        "ci:coverage:{}:{}",
        stable_segment(path),
        stable_segment(&file.path)
    );
    let rate = if file.found == 0 {
        "0".to_string()
    } else {
        format!("{:.4}", file.hit as f64 / file.found as f64)
    };
    result.facts.push(CollectedFact {
        stable_key: key.clone(),
        kind: "coverage-result".to_string(),
        name: Path::new(&file.path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(&file.path)
            .to_string(),
        path: Some(file.path),
        properties: properties(&[
            ("lines_found", Some(&file.found.to_string())),
            ("lines_hit", Some(&file.hit.to_string())),
            ("line_rate", Some(&rate)),
        ]),
    });
    result
        .relations
        .push(contains(report, &key, path, None, None));
}

fn parse_junit(path: &str, source: &str, result: &mut CollectorResult) -> bool {
    let cases = junit_cases(source);
    if cases.is_empty() {
        return false;
    }
    let report = report_fact(path, "junit");
    let report_key = report.stable_key.clone();
    result.facts.push(report);
    let mut suites: HashMap<String, TestSummary> = HashMap::new();
    for case in cases {
        let suite = suites.entry(case.suite).or_default();
        suite.tests += 1;
        suite.failures += usize::from(case.status == "failed");
        suite.errors += usize::from(case.status == "error");
        suite.skipped += usize::from(case.status == "skipped");
        suite.seconds += case.seconds;
    }
    for (suite, summary) in suites {
        let key = format!(
            "ci:junit:{}:{}",
            stable_segment(path),
            stable_segment(&suite)
        );
        result.facts.push(CollectedFact {
            stable_key: key.clone(),
            kind: "test-suite-result".to_string(),
            name: suite,
            path: Some(path.to_string()),
            properties: properties(&[
                ("tests", Some(&summary.tests.to_string())),
                ("failures", Some(&summary.failures.to_string())),
                ("errors", Some(&summary.errors.to_string())),
                ("skipped", Some(&summary.skipped.to_string())),
                ("seconds", Some(&format!("{:.3}", summary.seconds))),
            ]),
        });
        result
            .relations
            .push(contains(&report_key, &key, path, None, None));
    }
    true
}

#[derive(Default)]
struct TestSummary {
    tests: usize,
    failures: usize,
    errors: usize,
    skipped: usize,
    seconds: f64,
}

struct TestCase {
    suite: String,
    status: &'static str,
    seconds: f64,
}

fn junit_cases(source: &str) -> Vec<TestCase> {
    let mut output = Vec::new();
    let mut cursor = 0usize;
    // ponytail: JUnit testcases are flat in the standard formats we import;
    // add a streaming XML dependency only if namespaced/nonstandard reports appear.
    while let Some(offset) = source[cursor..].find("<testcase") {
        let start = cursor + offset;
        let Some(end) = xml_tag_end(source, start) else {
            break;
        };
        let tag = &source[start + 1..end];
        if !tag
            .as_bytes()
            .get("testcase".len())
            .is_some_and(|byte| byte.is_ascii_whitespace() || *byte == b'/' || *byte == b'>')
        {
            cursor = end + 1;
            continue;
        }
        let attributes = xml_attributes(tag);
        let self_closing = tag.trim_end().ends_with('/');
        let (body, next) = if self_closing {
            ("", end + 1)
        } else if let Some(close) = source[end + 1..].find("</testcase>") {
            let close = end + 1 + close;
            (&source[end + 1..close], close + "</testcase>".len())
        } else {
            cursor = end + 1;
            continue;
        };
        let status = if body.contains("<failure") {
            "failed"
        } else if body.contains("<error") {
            "error"
        } else if body.contains("<skipped") {
            "skipped"
        } else {
            "passed"
        };
        output.push(TestCase {
            suite: attributes
                .get("classname")
                .or_else(|| attributes.get("file"))
                .cloned()
                .unwrap_or_else(|| "tests".to_string()),
            status,
            seconds: attributes
                .get("time")
                .and_then(|value| value.parse().ok())
                .unwrap_or(0.0),
        });
        cursor = next;
    }
    output
}

fn xml_tag_end(source: &str, start: usize) -> Option<usize> {
    let mut quote = None;
    for (offset, byte) in source.as_bytes().get(start + 1..)?.iter().enumerate() {
        match (*byte, quote) {
            (b'\'' | b'"', None) => quote = Some(*byte),
            (value, Some(expected)) if value == expected => quote = None,
            (b'>', None) => return Some(start + 1 + offset),
            _ => {}
        }
    }
    None
}

fn xml_attributes(tag: &str) -> HashMap<String, String> {
    let bytes = tag.as_bytes();
    let mut output = HashMap::new();
    let mut index = tag.find(char::is_whitespace).unwrap_or(tag.len());
    while index < bytes.len() {
        while bytes.get(index).is_some_and(u8::is_ascii_whitespace) {
            index += 1;
        }
        let key_start = index;
        while bytes.get(index).is_some_and(|byte| {
            byte.is_ascii_alphanumeric() || matches!(*byte, b'_' | b'-' | b':' | b'.')
        }) {
            index += 1;
        }
        if key_start == index {
            index += 1;
            continue;
        }
        let key = &tag[key_start..index];
        while bytes.get(index).is_some_and(u8::is_ascii_whitespace) {
            index += 1;
        }
        if bytes.get(index) != Some(&b'=') {
            continue;
        }
        index += 1;
        while bytes.get(index).is_some_and(u8::is_ascii_whitespace) {
            index += 1;
        }
        let Some(quote @ (b'\'' | b'"')) = bytes.get(index).copied() else {
            continue;
        };
        index += 1;
        let value_start = index;
        while bytes.get(index).is_some_and(|byte| *byte != quote) {
            index += 1;
        }
        output.insert(key.to_string(), tag[value_start..index].to_string());
        index += usize::from(index < bytes.len());
    }
    output
}

fn report_fact(path: &str, format: &str) -> CollectedFact {
    CollectedFact {
        stable_key: format!("ci:report:{}", stable_segment(path)),
        kind: "verification-report".to_string(),
        name: Path::new(path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(path)
            .to_string(),
        path: Some(path.to_string()),
        properties: properties(&[("format", Some(format))]),
    }
}

fn contains(
    report: &str,
    item: &str,
    path: &str,
    line: Option<u32>,
    note: Option<String>,
) -> CollectedRelation {
    CollectedRelation {
        from: report.to_string(),
        to: item.to_string(),
        kind: "CONTAINS".to_string(),
        truth_class: TruthClass::Confirmed,
        evidence_type: "CI_ARTIFACT".to_string(),
        evidence: vec![CollectedEvidence {
            path: path.to_string(),
            line,
            note,
        }],
        properties: BTreeMap::new(),
    }
}

fn diagnostic(path: String, message: String) -> CollectionDiagnostic {
    CollectionDiagnostic {
        collector: ID,
        level: "warning",
        code: "invalid-ci-artifact",
        message,
        path: Some(path),
    }
}

#[cfg(test)]
mod tests {
    use super::collect;

    #[test]
    fn imports_ci_evidence_without_testcase_node_explosion() {
        let root =
            std::env::temp_dir().join(format!("code-memory-ci-evidence-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("junit.xml"),
            r#"<testsuite><testcase classname="api" name="ok" time="0.1"/><testcase classname="api" name="bad"><failure/></testcase></testsuite>"#,
        )
        .unwrap();
        std::fs::write(
            root.join("lcov.info"),
            "SF:src/app.ts\nDA:1,1\nDA:2,0\nend_of_record\n",
        )
        .unwrap();
        std::fs::write(
            root.join("scan.sarif"),
            r#"{"runs":[{"results":[{"ruleId":"R1","level":"warning","message":{"text":"first"},"locations":[{"physicalLocation":{"artifactLocation":{"uri":"src/app.ts"},"region":{"startLine":2}}}]},{"ruleId":"R1","level":"warning","message":{"text":"second"},"locations":[{"physicalLocation":{"artifactLocation":{"uri":"src/app.ts"}}}]}]}]}"#,
        )
        .unwrap();
        std::fs::create_dir_all(root.join(".code_memory/evidence")).unwrap();
        std::fs::write(
            root.join(".code_memory/evidence/verification-run.json"),
            r#"{"schema":"code-memory.verification-run.v1","label":"unit tests","tool":"cargo","status":"passed","duration_ms":42,"exit_code":0}"#,
        )
        .unwrap();

        let result = collect(&root);
        assert!(result.facts.iter().any(|fact| {
            fact.kind == "test-suite-result"
                && fact.properties.get("tests").map(String::as_str) == Some("2")
                && fact.properties.get("failures").map(String::as_str) == Some("1")
        }));
        assert!(result.facts.iter().any(|fact| {
            fact.kind == "coverage-result"
                && fact.properties.get("line_rate").map(String::as_str) == Some("0.5000")
        }));
        assert!(result.facts.iter().any(|fact| {
            fact.kind == "static-analysis-result"
                && fact.properties.get("count").map(String::as_str) == Some("2")
        }));
        assert!(result.facts.iter().any(|fact| {
            fact.kind == "verification-run"
                && fact.properties.get("status").map(String::as_str) == Some("passed")
        }));
        assert_eq!(
            result
                .facts
                .iter()
                .filter(|fact| fact.kind == "test-suite-result")
                .count(),
            1
        );
        let _ = std::fs::remove_dir_all(root);
    }
}
