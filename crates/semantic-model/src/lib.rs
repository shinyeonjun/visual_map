//! Provider-neutral contracts for the AI-derived semantic layer.
//!
//! Static facts remain authoritative in `codebase-fact-model`. This crate may
//! reference them but may never redefine their identity, evidence, truth,
//! counts, or path order.

#![forbid(unsafe_code)]
#![warn(unreachable_pub)]

pub mod approved;
pub mod identity;
pub mod input;
pub mod output;

pub use approved::{
    ApprovedRegionAssignment, ApprovedSemanticArea, ApprovedSemanticRevision,
    SemanticRevisionProvider,
};
pub use identity::{
    ProposalKey, RegionId, RelationBundleId, SemanticAreaId, SemanticIdError, SemanticRevisionId,
    TracePathId,
};
pub use input::{
    AiProviderDescriptor, AiProviderKind, AnchorFactSummary, BaseSemanticInput, BaseSemanticPacket,
    BoundaryRelationCount, BoundaryRelationSummary, EvidenceExcerpt, OutputLanguage,
    PreviousAreaSummary, PreviousRegionAssignment, PreviousSemanticRevisionSummary,
    ProjectSemanticContext, ReasoningEffort, ScopeReceipt, SemanticTask, StaticRegionKind,
    StaticRegionSummary, TracePathState, TracePathSummary,
};
pub use output::{
    AreaCategory, AreaProposal, LabelSource, ProjectSemanticProposal, RegionAssignment,
    SemanticFallbackReason, SemanticRevisionProposal, UnassignedReason, UnassignedRegion,
};

/// Version of the base semantic input and output contracts.
pub const BASE_SEMANTIC_SCHEMA_VERSION: u16 = 2;
