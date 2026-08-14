//! Claude CLI 프로세스 실행과 입출력 수집을 담당한다.

use super::{ClaudeError, ClaudeProvider};
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

impl ClaudeProvider {
    pub(crate) fn execute_prompt(
        &self,
        prompt: &str,
        project_root: &Path,
    ) -> Result<Vec<u8>, ClaudeError> {
        let actual_bytes = prompt.len();
        if actual_bytes > self.max_input_bytes {
            return Err(ClaudeError::InputTooLarge {
                actual_bytes,
                max_bytes: self.max_input_bytes,
            });
        }

        let mut command = Command::new(&self.executable);
        command
            .current_dir(project_root)
            .arg("-p")
            .arg("--output-format")
            .arg("text")
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
            .map_err(|error| ClaudeError::Spawn(error.to_string()))?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(prompt.as_bytes())
                .map_err(|error| ClaudeError::Io(error.to_string()))?;
        }

        let stdout_reader = child.stdout.take().map(|mut reader| {
            thread::spawn(move || {
                let mut buffer = Vec::new();
                std::io::Read::read_to_end(&mut reader, &mut buffer).map(|_| buffer)
            })
        });
        let stderr_reader = child.stderr.take().map(|mut reader| {
            thread::spawn(move || {
                let mut buffer = Vec::new();
                std::io::Read::read_to_end(&mut reader, &mut buffer).map(|_| buffer)
            })
        });

        let started = Instant::now();
        let status = loop {
            match child
                .try_wait()
                .map_err(|error| ClaudeError::Io(error.to_string()))?
            {
                Some(status) => break status,
                None if self.timeout_ms > 0
                    && started.elapsed() > Duration::from_millis(self.timeout_ms) =>
                {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(ClaudeError::Timeout);
                }
                None => thread::sleep(Duration::from_millis(25)),
            }
        };

        let stdout = stdout_reader
            .map(|handle| {
                handle
                    .join()
                    .map_err(|_| ClaudeError::Io("stdout reader 종료 실패".into()))?
                    .map_err(|error| ClaudeError::Io(error.to_string()))
            })
            .transpose()?
            .unwrap_or_default();
        let stderr = stderr_reader
            .map(|handle| {
                handle
                    .join()
                    .map_err(|_| ClaudeError::Io("stderr reader 종료 실패".into()))?
                    .map_err(|error| ClaudeError::Io(error.to_string()))
            })
            .transpose()?
            .unwrap_or_default();

        if !status.success() {
            let stderr_text = String::from_utf8_lossy(&stderr).trim().to_string();
            let message = if stderr_text.is_empty() {
                String::from_utf8_lossy(&stdout).trim().to_string()
            } else {
                stderr_text
            };
            return Err(ClaudeError::Process(message));
        }
        Ok(stdout)
    }
}
