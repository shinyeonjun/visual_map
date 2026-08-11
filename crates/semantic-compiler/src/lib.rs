//! Deterministic base-semantic prompt compilation and provider-output
//! verification.

#![forbid(unsafe_code)]
#![warn(unreachable_pub)]

mod error;
mod packet;
mod partition;
mod prompt;
mod reconciliation;
mod schema;
mod verifier;

pub use error::{SemanticCompileError, SemanticCompileErrorCode};
pub use partition::{
    compile_semantic_plan, compile_semantic_plan_with_policy, CompiledSemanticPartition,
    CompiledSemanticPlan, SemanticPartitionPolicy, VerifiedSemanticPartition,
};
pub use prompt::{
    compile_base_prompt, compile_base_repair_prompt, compile_base_repair_prompt_with_history,
    BaseSemanticDraft, CompiledBasePrompt, SemanticVerificationPhase, PACKET_COMPILER_VERSION,
    PROMPT_POLICY_VERSION,
};
pub use reconciliation::{
    compile_global_reconciliation_prompt, compile_global_reconciliation_repair_prompt,
    parse_and_verify_global_reconciliation, CompiledGlobalReconciliation,
};
pub use schema::base_semantic_output_schema;
pub use verifier::{parse_and_verify_base_response, verify_base_proposal};
