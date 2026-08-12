//! 정적 Facts를 사람이 이해할 수 있는 의미로 보정하는 영역.
//!
//! 현재 지원 provider는 Codex CLI 하나다. 의미 분석기는 정적 분석을 대체하지
//! 않으며, 기존 코드 단위·관계·증거를 참조하는 제안만 반환한다.

pub mod codex;
mod codex_prompt;
pub mod context;
pub mod names;
pub mod proposal;
pub mod provider;
pub mod validator;

pub use codex::CodexProvider;
pub use provider::{SemanticProvider, SemanticStatus};
