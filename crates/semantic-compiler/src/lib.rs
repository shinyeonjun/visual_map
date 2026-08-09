//! Deterministic base-semantic prompt compilation and provider-output
//! verification.

#![forbid(unsafe_code)]
#![warn(unreachable_pub)]

mod error;
mod packet;
mod partition;
mod prompt;
mod schema;
mod verifier;

pub use error::{SemanticCompileError, SemanticCompileErrorCode};
pub use partition::{
    compile_reconciliation_prompt, compile_semantic_plan, compile_semantic_plan_with_policy,
    CompiledSemanticPartition, CompiledSemanticPlan, SemanticPartitionPolicy,
    VerifiedSemanticPartition,
};
pub use prompt::{
    compile_base_prompt, compile_base_repair_prompt, BaseSemanticDraft, CompiledBasePrompt,
    PACKET_COMPILER_VERSION, PROMPT_POLICY_VERSION,
};
pub use schema::base_semantic_output_schema;
pub use verifier::{parse_and_verify_base_response, verify_base_proposal};
