//! Codex JSONL 응답에서 이름 제안만 추출한다.

use crate::semantic::codex::CodexError;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct NameProposal {
    pub domains: Vec<NameSuggestion>,
    pub modules: Vec<NameSuggestion>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NameSuggestion {
    pub id: String,
    pub name: String,
}

pub(super) fn parse_jsonl(stdout: &[u8]) -> Result<NameProposal, CodexError> {
    let text = String::from_utf8_lossy(stdout);
    let mut candidates = Vec::new();
    if let Ok(value) = serde_json::from_str::<Value>(&text) {
        collect_strings(&value, &mut candidates);
    }
    for line in text.lines() {
        if let Ok(value) = serde_json::from_str::<Value>(line) {
            collect_strings(&value, &mut candidates);
        }
    }
    candidates.push(text.to_string());

    for candidate in candidates.into_iter().rev() {
        let cleaned = candidate
            .trim()
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim();
        let Some(start) = cleaned.find('{') else {
            continue;
        };
        let Some(end) = cleaned.rfind('}') else {
            continue;
        };
        if let Ok(proposal) = serde_json::from_str::<NameProposal>(&cleaned[start..=end]) {
            return Ok(proposal);
        }
    }

    Err(CodexError::InvalidResponse(
        "이름 제안 JSON을 찾지 못했습니다.".into(),
    ))
}

fn collect_strings(value: &Value, output: &mut Vec<String>) {
    match value {
        Value::String(text) => output.push(text.clone()),
        Value::Array(values) => values
            .iter()
            .for_each(|value| collect_strings(value, output)),
        Value::Object(map) => map
            .values()
            .for_each(|value| collect_strings(value, output)),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::parse_jsonl;

    #[test]
    fn 이름_응답에서_도메인과_모듈_이름을_읽는다() {
        let output = r#"{"domains":[{"id":"domain_a","name":"인증"}],"modules":[{"id":"unit_a","name":"로그인"}]}"#;
        let proposal = parse_jsonl(output.as_bytes()).expect("이름 제안을 읽어야 한다");
        assert_eq!(proposal.domains[0].id, "domain_a");
        assert_eq!(proposal.modules[0].name, "로그인");
    }
}
