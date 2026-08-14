use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Command, Output, Stdio};

pub(crate) fn run_engine_with_progress<const N: usize, F>(
    engine: &Path,
    args: [&std::ffi::OsStr; N],
    mut on_line: F,
) -> Result<Vec<String>, String>
where
    F: FnMut(&str),
{
    let mut child = Command::new(engine)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| {
            format!(
                "분석 엔진을 실행하지 못했습니다 ({}): {error}",
                engine.display()
            )
        })?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "분석 엔진의 진행 출력을 읽지 못했습니다.".to_string())?;
    let reader = BufReader::new(stderr);
    let mut stderr_text = String::new();
    let mut lines = Vec::new();
    for line in reader.lines() {
        let line = line.map_err(|error| format!("분석 엔진 출력을 읽지 못했습니다: {error}"))?;
        on_line(&line);
        lines.push(line.clone());
        stderr_text.push_str(&line);
        stderr_text.push('\n');
    }
    let status = child
        .wait()
        .map_err(|error| format!("분석 엔진 종료 상태를 확인하지 못했습니다: {error}"))?;
    if status.success() {
        return Ok(lines);
    }
    Err(format!(
        "분석 단계가 실패했습니다 ({}): {}",
        engine.display(),
        stderr_text.trim()
    ))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SemanticProgressEvent {
    pub stage: Option<String>,
    pub chunk: Option<usize>,
    pub completed: Option<usize>,
    pub total: Option<usize>,
    pub status: Option<String>,
}

pub(crate) fn parse_semantic_progress(line: &str) -> Option<SemanticProgressEvent> {
    if !line.contains("[semantic] progress") {
        return None;
    }
    let mut event = SemanticProgressEvent {
        stage: None,
        chunk: None,
        completed: None,
        total: None,
        status: None,
    };
    for token in line.split_whitespace() {
        if let Some(value) = token.strip_prefix("stage=") {
            event.stage = Some(value.to_string());
        }
        if let Some(value) = token.strip_prefix("chunk=") {
            event.chunk = value.parse::<usize>().ok();
        }
        if let Some(value) = token.strip_prefix("completed=") {
            event.completed = value.parse::<usize>().ok();
        }
        if let Some(value) = token.strip_prefix("total=") {
            event.total = value.parse::<usize>().ok();
        }
        if let Some(value) = token.strip_prefix("status=") {
            event.status = Some(value.to_string());
        }
    }
    if event.status.is_some()
        || event.total.is_some()
        || event.chunk.is_some()
        || event.completed.is_some()
    {
        Some(event)
    } else {
        None
    }
}

pub(crate) fn command_output(output: &Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if stderr.is_empty() {
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    } else {
        stderr
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_semantic_progress, SemanticProgressEvent};

    #[test]
    fn parses_chunk_started_event() {
        let event = parse_semantic_progress(
            "[semantic] progress stage=feature chunk=2 total=5 status=started",
        )
        .expect("started event");
        assert_eq!(
            event,
            SemanticProgressEvent {
                stage: Some("feature".into()),
                chunk: Some(2),
                completed: None,
                total: Some(5),
                status: Some("started".into()),
            }
        );
    }

    #[test]
    fn parses_chunk_completed_event() {
        let event = parse_semantic_progress(
            "[semantic] progress stage=flow chunk=1 total=3 status=completed",
        )
        .expect("completed event");
        assert_eq!(event.status.as_deref(), Some("completed"));
        assert_eq!(event.chunk, Some(1));
    }

    #[test]
    fn parses_chunk_failed_event() {
        let event = parse_semantic_progress(
            "[semantic] progress stage=domain chunk=1 total=1 status=failed",
        )
        .expect("failed event");
        assert_eq!(event.status.as_deref(), Some("failed"));
        assert_eq!(event.chunk, Some(1));
    }
}
