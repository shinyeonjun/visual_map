//! Codex 제안 검증 결과의 모델이다.

use crate::diagnostics::Diagnostic;
use crate::semantic::proposal::{DomainMergeSuggestion, DomainSemanticSuggestion};

/// 검증을 통과한 Codex 제안 결과다.
#[derive(Debug, Default)]
pub struct ValidatedProposal {
    pub suggestions: Vec<DomainSemanticSuggestion>,
    pub merges: Vec<DomainMergeSuggestion>,
    pub diagnostics: Vec<Diagnostic>,
}
