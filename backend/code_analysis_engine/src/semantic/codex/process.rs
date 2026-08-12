//! Codex CLI 프로세스 실행과 입출력 수집을 담당한다.

use super::response::{
    join_reader, looks_like_input_limit_error, parse_jsonl_proposal, spawn_reader,
};
use super::{CodexError, CodexProvider};
use crate::semantic::codex_prompt::build_domain_review_prompt;
use crate::semantic::context::SemanticContext;
use crate::semantic::proposal::CodexProposal;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

impl CodexProvider {
    /// Codex CLI를 읽기 전용 비대화식 모드로 실행해 도메인 제안을 받는다.
    pub fn review(
        &self,
        context: &SemanticContext,
        project_root: &Path,
    ) -> Result<CodexProposal, CodexError> {
        self.review_chunk(context, project_root, 0, 1)
    }

    /// 분할된 컨텍스트 하나를 Codex CLI에 전달한다.
    pub fn review_chunk(
        &self,
        context: &SemanticContext,
        project_root: &Path,
        chunk_index: usize,
        chunk_count: usize,
    ) -> Result<CodexProposal, CodexError> {
        let prompt = build_domain_review_prompt(context, chunk_index, chunk_count)
            .map_err(|error| CodexError::Io(error.to_string()))?;
        let actual_bytes = prompt.len();
        if actual_bytes > self.max_input_bytes {
            return Err(CodexError::InputTooLarge {
                actual_bytes,
                max_bytes: self.max_input_bytes,
            });
        }

        let stdout = self.execute_prompt(&prompt, project_root)?;
        parse_jsonl_proposal(&stdout)
    }

    /// 이름 전용 분석처럼 다른 JSON 응답 스키마를 사용하는 호출도 같은
    /// 읽기 전용 Codex 실행 계약을 공유한다.
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
