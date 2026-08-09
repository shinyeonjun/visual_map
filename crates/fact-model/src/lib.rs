//! Shared, versioned contracts for the Codebase Workspace static fact pipeline.
//!
//! The crate contains provider-neutral data types and fail-closed validation.
//! It intentionally contains no filesystem, provider-process, database, UI, or
//! AI behavior.

#![forbid(unsafe_code)]
#![warn(unreachable_pub)]

pub mod analysis;
pub mod analysis_plan;
pub mod coverage;
pub mod evidence;
pub mod fact_graph;
pub mod identity;
pub mod language_ir;
pub mod source;
pub mod source_manifest;
pub mod validation;

use serde::{Deserialize, Serialize};

/// The closed schema identities accepted by the first canonical import
/// contract. Database storage schema versions are intentionally separate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ContractSchema {
    /// Deterministic repository census before provider planning.
    #[serde(rename = "codebase-workspace.source-manifest.v1")]
    SourceManifestV1,
    /// Deterministic mapping from source files to semantic analysis units.
    #[serde(rename = "codebase-workspace.analysis-plan.v1")]
    AnalysisPlanV1,
    /// Transient provider-to-normalizer records.
    #[serde(rename = "codebase-workspace.language-ir.v1")]
    LanguageIrV1,
    /// Language IR whose header seals the provider execution context actually
    /// used for the unit, not only the planner's intended context.
    #[serde(rename = "codebase-workspace.language-ir.v2")]
    LanguageIrV2,
    /// Canonical rows carried by an immutable import bundle.
    #[serde(rename = "codebase-workspace.canonical-fact.v1")]
    CanonicalFactV1,
}

impl ContractSchema {
    /// Returns the stable serialized schema name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SourceManifestV1 => "codebase-workspace.source-manifest.v1",
            Self::AnalysisPlanV1 => "codebase-workspace.analysis-plan.v1",
            Self::LanguageIrV1 => "codebase-workspace.language-ir.v1",
            Self::LanguageIrV2 => "codebase-workspace.language-ir.v2",
            Self::CanonicalFactV1 => "codebase-workspace.canonical-fact.v1",
        }
    }
}
