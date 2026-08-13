use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Command, Output, Stdio};

pub(crate) fn run_engine<const N: usize>(
    engine: &Path,
    args: [&std::ffi::OsStr; N],
) -> Result<(), String> {
    let output = Command::new(engine).args(args).output().map_err(|error| {
        format!(
            "분석 엔진을 실행하지 못했습니다 ({}): {error}",
            engine.display()
        )
    })?;
    if output.status.success() {
        return Ok(());
    }
    Err(format!(
        "분석 단계가 실패했습니다 ({}): {}",
        engine.display(),
        command_output(&output)
    ))
}

pub(crate) fn run_engine_with_progress<const N: usize, F>(
    engine: &Path,
    args: [&std::ffi::OsStr; N],
    mut on_line: F,
) -> Result<(), String>
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
    for line in reader.lines() {
        let line = line.map_err(|error| format!("분석 엔진 출력을 읽지 못했습니다: {error}"))?;
        on_line(&line);
        stderr_text.push_str(&line);
        stderr_text.push('\n');
    }
    let status = child
        .wait()
        .map_err(|error| format!("분석 엔진 종료 상태를 확인하지 못했습니다: {error}"))?;
    if status.success() {
        return Ok(());
    }
    Err(format!(
        "분석 단계가 실패했습니다 ({}): {}",
        engine.display(),
        stderr_text.trim()
    ))
}

pub(crate) fn parse_semantic_progress(line: &str) -> Option<(usize, usize)> {
    if !line.contains("[semantic] progress") {
        return None;
    }
    let mut completed = None;
    let mut total = None;
    for token in line.split_whitespace() {
        if let Some(value) = token.strip_prefix("completed=") {
            completed = value.parse::<usize>().ok();
        }
        if let Some(value) = token.strip_prefix("total=") {
            total = value.parse::<usize>().ok();
        }
    }
    Some((completed.unwrap_or(0), total.unwrap_or(0)))
}

pub(crate) fn command_output(output: &Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if stderr.is_empty() {
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    } else {
        stderr
    }
}
