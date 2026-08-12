//! Codex CLI의 JSONL 출력에서 구조화된 제안을 추출한다.

use super::CodexError;
use crate::semantic::proposal::CodexProposal;
use serde_json::Value;
use std::io::Read;
use std::thread;

pub(super) fn looks_like_input_limit_error(message: &str) -> bool {
    let message = message.to_ascii_lowercase();
    message.contains("input exceeds")
        || message.contains("maximum length")
        || message.contains("too large")
}

pub(super) fn join_reader(
    reader: Option<thread::JoinHandle<std::io::Result<Vec<u8>>>>,
    name: &str,
) -> Result<Vec<u8>, CodexError> {
    reader
        .map(|reader| {
            reader
                .join()
                .map_err(|_| CodexError::Io(format!("{name} reader 종료 실패")))?
                .map_err(|error| CodexError::Io(error.to_string()))
        })
        .transpose()
        .map(|value| value.unwrap_or_default())
}

pub(super) fn spawn_reader<R>(mut reader: R) -> thread::JoinHandle<std::io::Result<Vec<u8>>>
where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        let mut buffer = Vec::new();
        reader.read_to_end(&mut buffer).map(|_| buffer)
    })
}

pub(super) fn parse_jsonl_proposal(stdout: &[u8]) -> Result<CodexProposal, CodexError> {
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
        if let Ok(proposal) = serde_json::from_str::<CodexProposal>(&cleaned[start..=end]) {
            return Ok(proposal);
        }
    }

    Err(CodexError::InvalidResponse(
        "구조화된 JSON 제안이 없습니다.".into(),
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
