//! 정적 Facts를 사람이 이해할 수 있는 의미로 보정하는 영역.
//!
//! 지원 provider는 Codex CLI와 Claude CLI다. 의미 분석기는 정적 분석을 대체하지
//! 않으며, 기존 코드 단위·관계·증거를 참조하는 제안만 반환한다.

pub mod claude;
pub mod codex;
pub mod provider;
pub mod review;

pub use claude::ClaudeProvider;
pub use codex::CodexProvider;
pub use provider::{SemanticProvider, SemanticStatus};

use std::path::Path;

pub enum AiProvider {
    Codex(CodexProvider),
    Claude(ClaudeProvider),
}

#[derive(Debug)]
pub enum ProviderError {
    Codex(codex::CodexError),
    Claude(claude::ClaudeError),
}

impl std::fmt::Display for ProviderError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Codex(error) => error.fmt(formatter),
            Self::Claude(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for ProviderError {}

impl AiProvider {
    pub fn execute_prompt(&self, prompt: &str, project_root: &Path) -> Result<Vec<u8>, ProviderError> {
        match self {
            Self::Codex(provider) => provider.execute_prompt(prompt, project_root).map_err(ProviderError::Codex),
            Self::Claude(provider) => provider.execute_prompt(prompt, project_root).map_err(ProviderError::Claude),
        }
    }
}
