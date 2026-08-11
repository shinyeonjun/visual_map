use codebase_fact_model::analysis::{ProgrammingLanguage, ProviderProtocol};
use codebase_fact_model::coverage::{
    AnalysisCapability, DeclaredSupport, EvidencePrecision, GapCode,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum AdapterMeasurement {
    Full,
    Partial(GapCode),
    NotApplicable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct CapabilityPolicy {
    pub(super) capability: AnalysisCapability,
    pub(super) declared_support: DeclaredSupport,
    pub(super) measurement: AdapterMeasurement,
    pub(super) precision: EvidencePrecision,
    pub(super) file_denominator: bool,
}

/// Closed capability profile for the current provider-to-IR bridge. This is
/// intentionally more conservative than the final product support contract:
/// a capability stays partial or not-run until the current decoder actually
/// preserves enough evidence to prove otherwise.
pub(super) fn capability_policies(
    language: ProgrammingLanguage,
    _protocol: ProviderProtocol,
) -> Vec<CapabilityPolicy> {
    use AdapterMeasurement::{Full, NotApplicable, Partial};
    use AnalysisCapability::{
        Definitions, DirectCalls, EventExternal, Exports, Imports, OrmQuery, Overrides,
        ProjectStructure, TypeRelations,
    };
    use DeclaredSupport::{Conditional, Required, Unsupported};
    use EvidencePrecision::{ExactRange, Manifest, None, Symbol};

    // Imports/exports are measured by the independent syntax-site inventory
    // and exact project resolver, never by provider relation counts. The
    // adapter downgrades each receipt from this full baseline when a concrete
    // site is unresolved/ambiguous or its denominator cannot be enumerated.
    let imports = (Required, Full, ExactRange);
    let exports = if matches!(
        language,
        ProgrammingLanguage::TypeScript
            | ProgrammingLanguage::JavaScript
            | ProgrammingLanguage::Dart
    ) {
        (Conditional, Full, ExactRange)
    } else {
        (Unsupported, NotApplicable, None)
    };
    let overrides = if language == ProgrammingLanguage::C {
        (Unsupported, NotApplicable, None)
    } else {
        (Conditional, Partial(GapCode::MissingTypeMetadata), Symbol)
    };

    vec![
        policy(ProjectStructure, Required, Full, Manifest, true),
        policy(Definitions, Required, Full, ExactRange, true),
        policy(Imports, imports.0, imports.1, imports.2, true),
        policy(Exports, exports.0, exports.1, exports.2, true),
        policy(
            DirectCalls,
            Conditional,
            Partial(GapCode::DynamicDispatch),
            ExactRange,
            false,
        ),
        policy(
            TypeRelations,
            Conditional,
            Partial(GapCode::MissingTypeMetadata),
            Symbol,
            false,
        ),
        policy(Overrides, overrides.0, overrides.1, overrides.2, false),
        policy(OrmQuery, Conditional, Full, ExactRange, false),
        policy(EventExternal, Unsupported, NotApplicable, None, false),
    ]
}

const fn policy(
    capability: AnalysisCapability,
    declared_support: DeclaredSupport,
    measurement: AdapterMeasurement,
    precision: EvidencePrecision,
    file_denominator: bool,
) -> CapabilityPolicy {
    CapabilityPolicy {
        capability,
        declared_support,
        measurement,
        precision,
        file_denominator,
    }
}
