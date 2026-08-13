//! Codex CLI의 JSONL 출력에서 구조화된 제안을 추출한다.

use super::CodexError;
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
