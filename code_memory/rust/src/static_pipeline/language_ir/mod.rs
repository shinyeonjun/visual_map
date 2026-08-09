//! Typed bridge from current provider results to transient Language IR.
//!
//! `ProviderUnitBatch` is the sole Language IR authority. The validated stream
//! is written once to process-local staging for the canonical linker. The old
//! `language-index.v2` remains only as a compatibility projection for legacy
//! framework consumers; it is never converted back into a second IR stream.
//! This module never calls AI and never guesses a symbol target.

mod adapter;
pub(crate) mod artifact;
mod capabilities;
mod definition_inventory;
mod definition_metadata;
mod direct;
mod imports;
mod provider;
mod source_coordinates;
pub(crate) mod syntax;
pub(crate) mod type_relations;

pub(crate) use direct::{
    emit_direct_language_ir, reconcile_provider_execution_contexts, DirectLanguageIrInput,
};

use codebase_fact_model::identity::Sha256Digest;
use sha2::{Digest, Sha256};

const EXECUTION_CONTEXT_SET_DOMAIN: &[u8] =
    b"codebase-workspace.provider-execution-context-set.v1\0";

fn execution_context_set_digest(contexts: &[(String, Sha256Digest)]) -> Sha256Digest {
    let mut contexts = contexts.to_vec();
    contexts.sort();
    let mut hasher = Sha256::new();
    hasher.update(EXECUTION_CONTEXT_SET_DOMAIN);
    for (unit_id, fingerprint) in contexts {
        hash_component(&mut hasher, unit_id.as_bytes());
        hash_component(&mut hasher, fingerprint.as_bytes());
    }
    Sha256Digest::parse(&format!("{:x}", hasher.finalize()))
        .expect("SHA-256 output is a canonical digest")
}

fn hash_component(hasher: &mut Sha256, value: &[u8]) {
    hasher.update((value.len() as u64).to_be_bytes());
    hasher.update(value);
}

#[cfg(test)]
mod tests;
