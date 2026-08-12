//! 이름 전용 Codex 응답 호출 어댑터.

use super::response::{parse_jsonl, NameProposal};
use crate::semantic::codex::{CodexError, CodexProvider};
use std::path::Path;

impl CodexProvider {
    pub(crate) fn review_name_prompt(
        &self,
        prompt: &str,
        project_root: &Path,
    ) -> Result<NameProposal, CodexError> {
        let stdout = self.execute_prompt(prompt, project_root)?;
        parse_jsonl(&stdout)
    }
}
