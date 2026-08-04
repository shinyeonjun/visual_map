use serde::Serialize;
use std::path::Path;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::{resolve_tool, run_bounded_command, tool_command};

const MAX_OUTPUT_BYTES: usize = 8 * 1024 * 1024;

#[derive(Serialize)]
pub(crate) struct VerificationRun {
    pub(crate) schema: &'static str,
    pub(crate) mode: &'static str,
    pub(crate) label: String,
    pub(crate) tool: String,
    pub(crate) tool_origin: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) tool_version: Option<String>,
    pub(crate) project_root: String,
    pub(crate) started_unix_ms: u128,
    pub(crate) duration_ms: u128,
    pub(crate) status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) exit_code: Option<i32>,
    pub(crate) argument_count: usize,
    pub(crate) network_policy: &'static str,
    pub(crate) captured_stdout_bytes: usize,
    pub(crate) captured_stderr_bytes: usize,
    pub(crate) stdout_truncated: bool,
    pub(crate) stderr_truncated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) failure: Option<String>,
}

pub(crate) struct VerificationExecution {
    pub(crate) report: VerificationRun,
    pub(crate) stdout: Vec<u8>,
    pub(crate) stderr: Vec<u8>,
}

pub(crate) fn run(
    root: &Path,
    providers_root: Option<&Path>,
    tool: &str,
    arguments: &[String],
    label: &str,
    timeout: Duration,
) -> Result<VerificationExecution, String> {
    validate_tool_name(tool)?;
    if label.is_empty() || label.len() > 100 || label.chars().any(char::is_control) {
        return Err("verification label must contain 1-100 printable characters".to_string());
    }
    let root = crate::source::canonical_project_root(root)?;
    let resolution = resolve_tool(tool, providers_root);
    if resolution.path.is_none() {
        return Err(format!("verification tool is unavailable: {tool}"));
    }
    let mut command = tool_command(tool, providers_root)?;
    command.current_dir(&root).args(arguments);
    let started_unix_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let started = Instant::now();
    let execution = run_bounded_command(
        command,
        &format!("verification {label}"),
        timeout,
        MAX_OUTPUT_BYTES,
        MAX_OUTPUT_BYTES,
    );
    let duration_ms = started.elapsed().as_millis();
    let network_policy = if std::env::var("CODE_MEMORY_ALLOW_NETWORK").as_deref() == Ok("1") {
        "inherited"
    } else {
        "offline-environment"
    };
    match execution {
        Ok(output) => {
            let status = if output.status.success() {
                "passed"
            } else {
                "failed"
            };
            Ok(VerificationExecution {
                report: VerificationRun {
                    schema: "code-memory.verification-run.v1",
                    mode: "active-explicit",
                    label: label.to_string(),
                    tool: tool.to_string(),
                    tool_origin: resolution.origin,
                    tool_version: resolution.version,
                    project_root: root.to_string_lossy().into_owned(),
                    started_unix_ms,
                    duration_ms,
                    status,
                    exit_code: output.status.code(),
                    argument_count: arguments.len(),
                    network_policy,
                    captured_stdout_bytes: output.stdout.len(),
                    captured_stderr_bytes: output.stderr.len(),
                    stdout_truncated: output.stdout_truncated,
                    stderr_truncated: output.stderr_truncated,
                    failure: None,
                },
                stdout: output.stdout,
                stderr: output.stderr,
            })
        }
        Err(error) => Ok(VerificationExecution {
            report: VerificationRun {
                schema: "code-memory.verification-run.v1",
                mode: "active-explicit",
                label: label.to_string(),
                tool: tool.to_string(),
                tool_origin: resolution.origin,
                tool_version: resolution.version,
                project_root: root.to_string_lossy().into_owned(),
                started_unix_ms,
                duration_ms,
                status: if error.contains(" timeout after ") {
                    "timed-out"
                } else {
                    "failed"
                },
                exit_code: None,
                argument_count: arguments.len(),
                network_policy,
                captured_stdout_bytes: 0,
                captured_stderr_bytes: 0,
                stdout_truncated: false,
                stderr_truncated: false,
                failure: Some(error),
            },
            stdout: Vec::new(),
            stderr: Vec::new(),
        }),
    }
}

fn validate_tool_name(tool: &str) -> Result<(), String> {
    if tool.is_empty()
        || !tool
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err("verification tool must be a simple executable name".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::validate_tool_name;

    #[test]
    fn verification_tool_cannot_escape_to_a_shell_or_path() {
        assert!(validate_tool_name("cargo").is_ok());
        assert!(validate_tool_name("npm.cmd").is_ok());
        assert!(validate_tool_name("cmd /c echo bad").is_err());
        assert!(validate_tool_name("../tool").is_err());
    }
}
