//! Codex CLI 프로세스 실행과 입출력 수집을 담당한다.

use super::response::{join_reader, looks_like_input_limit_error, spawn_reader};
use super::{CodexError, CodexProvider};
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

impl CodexProvider {
    /// 의미 리뷰처럼 JSON 응답 스키마가 달라지는 호출도 공유하는
    /// 읽기 전용 Codex 실행 계약이다.
    pub(crate) fn execute_prompt(
        &self,
        prompt: &str,
        project_root: &Path,
    ) -> Result<Vec<u8>, CodexError> {
        let actual_bytes = prompt.len();
        if actual_bytes > self.max_input_bytes {
            return Err(CodexError::InputTooLarge {
                actual_bytes,
                max_bytes: self.max_input_bytes,
            });
        }

        let mut command = Command::new(&self.executable);
        command
            .args(&self.command_prefix)
            .arg("exec")
            .arg("--ephemeral")
            .arg("--skip-git-repo-check")
            .arg("--sandbox")
            .arg("read-only")
            .arg("--json")
            .arg("-C")
            .arg(project_root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(model) = self
            .model
            .as_deref()
            .filter(|model| !model.trim().is_empty())
        {
            command.arg("--model").arg(model);
        }
        let mut child = command
            .spawn()
            .map_err(|error| CodexError::Spawn(error.to_string()))?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(prompt.as_bytes())
                .map_err(|error| CodexError::Io(error.to_string()))?;
        }

        let stdout_reader = child.stdout.take().map(spawn_reader);
        let stderr_reader = child.stderr.take().map(spawn_reader);

        let started = Instant::now();
        let status = loop {
            match child
                .try_wait()
                .map_err(|error| CodexError::Io(error.to_string()))?
            {
                Some(status) => break status,
                None if self.timeout_ms > 0
                    && started.elapsed() > Duration::from_millis(self.timeout_ms) =>
                {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(CodexError::Timeout);
                }
                None => thread::sleep(Duration::from_millis(25)),
            }
        };

        let stdout = join_reader(stdout_reader, "stdout")?;
        let stderr = join_reader(stderr_reader, "stderr")?;
        if !status.success() {
            let message = String::from_utf8_lossy(&stderr).trim().to_string();
            if looks_like_input_limit_error(&message) {
                return Err(CodexError::InputTooLarge {
                    actual_bytes,
                    max_bytes: self.max_input_bytes,
                });
            }
            return Err(CodexError::Process(message));
        }
        Ok(stdout)
    }
}
