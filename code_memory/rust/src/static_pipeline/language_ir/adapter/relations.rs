//! Provider relation classification at the Language IR boundary.
//!
//! The provider supplies exact endpoints and source ranges. This module only
//! maps those facts into the closed product vocabulary when independent
//! syntax and definition evidence supports the mapping. It never invents a
//! target from a name or directory.

use super::dispatch::classify_execution_dispatch;
use super::{
    definition_base_name, is_type_owner_kind, ranges_equal, type_relation_site_key,
    AnalysisCapability, BTreeMap, BTreeSet, DefinitionDraft, DispatchKind, EvidenceKind,
    ExecutionControlContext, FactNodeKind, IrEndpoint, IrRelation, LanguageRelationKind,
    ProgrammingLanguage, ProviderProtocol, ProviderSymbolId, RelationOutput, RepositoryPath,
    SyntaxCallSite, SyntaxDefinition, SyntaxTypeRelationSite, SyntaxTypeUseSite,
    TypeRelationIntent,
};
use crate::static_pipeline::language_ir::syntax::range_contains;

#[derive(Clone, Debug)]
pub(super) struct ProviderRelationMapping {
    pub(super) kind: LanguageRelationKind,
    pub(super) capability: AnalysisCapability,
    pub(super) evidence_kind: EvidenceKind,
    pub(super) reverse_endpoints: bool,
    pub(super) evidence_range: Option<Vec<i32>>,
    pub(super) matched_explicit_site_key: Option<String>,
    pub(super) dispatch: DispatchKind,
    pub(super) execution: Option<ExecutionOccurrenceDraft>,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct ExecutionOccurrenceDraft {
    pub(super) lexical_ordinal: u32,
    pub(super) control: ExecutionControlContext,
}

pub(super) struct ProviderRelationClassificationContext<'a> {
    pub(super) language: ProgrammingLanguage,
    pub(super) protocol: ProviderProtocol,
    pub(super) definitions: &'a BTreeMap<ProviderSymbolId, DefinitionDraft>,
    pub(super) syntax_definitions: &'a BTreeMap<RepositoryPath, Vec<SyntaxDefinition>>,
    pub(super) syntax_call_sites: &'a BTreeMap<RepositoryPath, Vec<SyntaxCallSite>>,
    pub(super) syntax_type_relations: &'a BTreeMap<RepositoryPath, Vec<SyntaxTypeRelationSite>>,
    pub(super) syntax_type_uses: &'a BTreeMap<RepositoryPath, Vec<SyntaxTypeUseSite>>,
    pub(super) hierarchy_occurrence_ranges:
        &'a BTreeMap<String, BTreeMap<ProviderSymbolId, Vec<Vec<i32>>>>,
}

pub(super) fn provider_relation_capability(native: &str) -> Option<AnalysisCapability> {
    match native {
        // Imports/exports use the independent syntax-site denominator and
        // exact project resolver. Provider relations are deliberately not an
        // authority here because they commonly omit top-level import sites.
        "IMPORTS" => None,
        // Generic provider references are an internal resolution input, not a
        // product relation. Persist only a typed, visualization-relevant
        // relation such as calls, uses_type, tests, reads, or writes.
        "REFERENCES" | "SYMBOL_REFERENCE" => None,
        "CALLS" | "CONSTRUCTS" => Some(AnalysisCapability::DirectCalls),
        "IMPLEMENTATION"
        | "DEFINITION_OVERRIDE"
        | "DEFINITION"
        | "TYPE_DEFINITION"
        | "USES_TYPE" => Some(AnalysisCapability::TypeRelations),
        _ => None,
    }
}

pub(super) fn classify_provider_relation(
    relation: &RelationOutput,
    source: &IrEndpoint,
    target: &IrEndpoint,
    context: &ProviderRelationClassificationContext<'_>,
) -> Option<ProviderRelationMapping> {
    if matches!(relation.kind.as_str(), "CALLS" | "CONSTRUCTS") {
        return executable_relation_mapping(relation, source, target, context);
    }

    let (source_id, target_id) = match (source, target) {
        (
            IrEndpoint::NativeSymbol {
                symbol_id: source_id,
            },
            IrEndpoint::NativeSymbol {
                symbol_id: target_id,
            },
        ) => (source_id, target_id),
        _ => return None,
    };
    let source_definition = context.definitions.get(source_id)?;
    let target_definition = context.definitions.get(target_id)?;

    if matches!(relation.kind.as_str(), "TYPE_DEFINITION" | "USES_TYPE") {
        if !is_type_reference_target_kind(target_definition.canonical_kind_hint) {
            return None;
        }
        let site =
            explicit_type_use_site(context, relation, source_id, target_id, source_definition)?;
        return Some(type_relation_mapping(
            LanguageRelationKind::UsesType,
            Some(site.target_range(context.protocol).to_vec()),
            false,
            None,
        ));
    }

    if is_type_owner_kind(source_definition.canonical_kind_hint)
        && is_type_owner_kind(target_definition.canonical_kind_hint)
    {
        if relation.kind != "IMPLEMENTATION" {
            return None;
        }
        if context.language == ProgrammingLanguage::Go {
            return Some(type_relation_mapping(
                LanguageRelationKind::Implements,
                None,
                false,
                None,
            ));
        }
        let site = explicit_hierarchy_site(
            context,
            source_id,
            target_id,
            source_definition,
            target_definition,
        )?;
        let kind = hierarchy_kind(site.intent, source_definition, target_definition)?;
        return Some(type_relation_mapping(
            kind,
            Some(site.target_range(context.protocol).to_vec()),
            false,
            Some(type_relation_site_key(&source_definition.path, site)),
        ));
    }

    if is_override_pair(source_definition, target_definition, context.definitions) {
        return match relation.kind.as_str() {
            "IMPLEMENTATION" | "DEFINITION_OVERRIDE" => Some(type_relation_mapping(
                LanguageRelationKind::Overrides,
                None,
                false,
                None,
            )),
            // clangd reports a header declaration -> implementation pair as
            // `is_definition`. Only a cross-owner C++ method pair is an
            // override here; top-level prototype/definition pairs are not.
            "DEFINITION" if context.language == ProgrammingLanguage::Cpp => Some(
                type_relation_mapping(LanguageRelationKind::Overrides, None, true, None),
            ),
            _ => None,
        };
    }

    None
}

fn is_type_reference_target_kind(kind: FactNodeKind) -> bool {
    is_type_owner_kind(kind) || kind == FactNodeKind::TypeAlias
}

fn executable_relation_mapping(
    relation: &RelationOutput,
    source: &IrEndpoint,
    target: &IrEndpoint,
    context: &ProviderRelationClassificationContext<'_>,
) -> Option<ProviderRelationMapping> {
    let kind = match relation.kind.as_str() {
        "CALLS" => LanguageRelationKind::Calls,
        "CONSTRUCTS" => LanguageRelationKind::Constructs,
        _ => return None,
    };
    let IrEndpoint::NativeSymbol {
        symbol_id: target_id,
    } = target
    else {
        return None;
    };
    let target_definition = context.definitions.get(target_id)?;
    let path = RepositoryPath::parse(&relation.path).ok()?;
    let expected_names = executable_target_names(target_definition, context.definitions);
    let mut sites = context
        .syntax_call_sites
        .get(&path)?
        .iter()
        .filter(|site| site.matches_provider_range(&relation.range))
        .filter(|site| {
            call_site_matches_relation(
                site,
                kind,
                context.language,
                target_definition.canonical_kind_hint,
            )
        })
        .filter(|site| expected_names.contains(&site.callee_text))
        .filter(|site| call_site_belongs_to_source(site, source, &path, context));
    let site = sites.next()?;
    if sites.next().is_some() {
        // Multiple independently plausible call sites at one provider range
        // are not collapsed into a confirmed executable edge.
        return None;
    }
    Some(ProviderRelationMapping {
        kind,
        capability: AnalysisCapability::DirectCalls,
        evidence_kind: EvidenceKind::CallSite,
        reverse_endpoints: false,
        evidence_range: Some(site.callee_range(context.protocol)),
        matched_explicit_site_key: None,
        dispatch: classify_execution_dispatch(
            context.language,
            kind,
            site,
            target_definition,
            context.definitions,
        ),
        execution: Some(ExecutionOccurrenceDraft {
            lexical_ordinal: site.lexical_ordinal,
            control: site.control,
        }),
    })
}

fn executable_target_names(
    target: &DefinitionDraft,
    definitions: &BTreeMap<ProviderSymbolId, DefinitionDraft>,
) -> BTreeSet<String> {
    let mut names = BTreeSet::from([definition_base_name(&target.display_name)]);
    if target.canonical_kind_hint == FactNodeKind::Constructor {
        if let Some(owner) = target
            .parent_symbol_id
            .as_ref()
            .and_then(|owner_id| definitions.get(owner_id))
        {
            // Providers commonly resolve `new Box()` to `.ctor`, `<init>`, or
            // `__init__`. The source token is the owner type (`Box`), so both
            // exact semantic identities are needed for the same call site.
            names.insert(definition_base_name(&owner.display_name));
        }
    }
    names
}

fn call_site_matches_relation(
    site: &SyntaxCallSite,
    kind: LanguageRelationKind,
    language: ProgrammingLanguage,
    target_kind: FactNodeKind,
) -> bool {
    match kind {
        LanguageRelationKind::Calls => site.form != crate::CallSiteForm::Construct,
        LanguageRelationKind::Constructs => {
            site.form == crate::CallSiteForm::Construct
                || (matches!(
                    language,
                    ProgrammingLanguage::Python | ProgrammingLanguage::Dart
                ) && matches!(
                    target_kind,
                    FactNodeKind::Constructor
                        | FactNodeKind::Type
                        | FactNodeKind::Class
                        | FactNodeKind::Struct
                ))
        }
        _ => false,
    }
}

fn call_site_belongs_to_source(
    site: &SyntaxCallSite,
    source: &IrEndpoint,
    relation_path: &RepositoryPath,
    context: &ProviderRelationClassificationContext<'_>,
) -> bool {
    match source {
        IrEndpoint::File { path } => path == relation_path,
        IrEndpoint::NativeSymbol { symbol_id } => {
            let Some(source_definition) = context.definitions.get(symbol_id) else {
                return false;
            };
            if &source_definition.path != relation_path {
                return false;
            }
            let Some(source_syntax) = source_definition
                .syntax_match
                .and_then(|index| context.syntax_definitions.get(relation_path)?.get(index))
            else {
                return false;
            };
            match site.owner_name_range(context.protocol) {
                Some(owner_range) => {
                    ranges_equal(owner_range, source_syntax.name_range(context.protocol))
                }
                None => range_contains(
                    source_syntax.declaration_range(context.protocol),
                    site.expression_range(context.protocol),
                ),
            }
        }
        IrEndpoint::Structure { .. } => false,
    }
}

fn explicit_type_use_site<'a>(
    context: &'a ProviderRelationClassificationContext<'a>,
    relation: &RelationOutput,
    source_id: &ProviderSymbolId,
    target_id: &ProviderSymbolId,
    source: &DefinitionDraft,
) -> Option<&'a SyntaxTypeUseSite> {
    if source_id == target_id || source.path.as_str() != relation.path {
        return None;
    }
    let source_syntax = source
        .syntax_match
        .and_then(|index| context.syntax_definitions.get(&source.path)?.get(index))?;
    context
        .syntax_type_uses
        .get(&source.path)?
        .iter()
        .find(|site| {
            ranges_equal(
                site.source_name_range(context.protocol),
                source_syntax.name_range(context.protocol),
            ) && site.matches_target_range(&relation.range, context.protocol)
        })
}

fn type_relation_mapping(
    kind: LanguageRelationKind,
    evidence_range: Option<Vec<i32>>,
    reverse_endpoints: bool,
    matched_explicit_site_key: Option<String>,
) -> ProviderRelationMapping {
    ProviderRelationMapping {
        kind,
        capability: AnalysisCapability::TypeRelations,
        evidence_kind: EvidenceKind::TypeRelation,
        reverse_endpoints,
        evidence_range,
        matched_explicit_site_key,
        dispatch: DispatchKind::NotApplicable,
        execution: None,
    }
}

fn hierarchy_kind(
    intent: TypeRelationIntent,
    source: &DefinitionDraft,
    target: &DefinitionDraft,
) -> Option<LanguageRelationKind> {
    match intent {
        TypeRelationIntent::Exact(kind) => Some(kind),
        TypeRelationIntent::CSharpBase => {
            if source.canonical_kind_hint == FactNodeKind::Interface {
                return (target.canonical_kind_hint == FactNodeKind::Interface)
                    .then_some(LanguageRelationKind::Extends);
            }
            match target.canonical_kind_hint {
                FactNodeKind::Interface | FactNodeKind::Trait => {
                    Some(LanguageRelationKind::Implements)
                }
                FactNodeKind::Class | FactNodeKind::Struct => Some(LanguageRelationKind::Extends),
                _ => None,
            }
        }
    }
}

fn explicit_hierarchy_site<'a>(
    context: &'a ProviderRelationClassificationContext<'a>,
    source_id: &ProviderSymbolId,
    target_id: &ProviderSymbolId,
    source: &DefinitionDraft,
    target: &DefinitionDraft,
) -> Option<&'a SyntaxTypeRelationSite> {
    let source_syntax = source
        .syntax_match
        .and_then(|index| context.syntax_definitions.get(&source.path)?.get(index))?;
    let sites = context.syntax_type_relations.get(&source.path)?;
    let ranges_for = |symbol_id: &ProviderSymbolId| {
        context
            .hierarchy_occurrence_ranges
            .get(source.path.as_str())
            .and_then(|ranges| ranges.get(symbol_id))
            .map(|ranges| ranges.iter().map(Vec::as_slice).collect::<Vec<_>>())
            .unwrap_or_default()
    };
    let exact_source_ranges = ranges_for(source_id);
    let exact_target_ranges = ranges_for(target_id);
    let target_name = definition_base_name(&target.display_name);

    sites.iter().find(|site| {
        (ranges_equal(
            site.source_range(context.protocol),
            source_syntax.name_range(context.protocol),
        ) || exact_source_ranges
            .iter()
            .any(|range| ranges_equal(range, site.source_range(context.protocol))))
            && (exact_target_ranges
                .iter()
                .any(|range| ranges_equal(range, site.target_range(context.protocol)))
                || site.target_name == target_name)
    })
}

fn is_override_pair(
    source: &DefinitionDraft,
    target: &DefinitionDraft,
    definitions: &BTreeMap<ProviderSymbolId, DefinitionDraft>,
) -> bool {
    if source.canonical_kind_hint != FactNodeKind::Method
        || target.canonical_kind_hint != FactNodeKind::Method
    {
        return false;
    }
    let (Some(source_owner), Some(target_owner)) = (
        source.parent_symbol_id.as_ref(),
        target.parent_symbol_id.as_ref(),
    ) else {
        return false;
    };
    source_owner != target_owner
        && definitions
            .get(source_owner)
            .is_some_and(|owner| is_type_owner_kind(owner.canonical_kind_hint))
        && definitions
            .get(target_owner)
            .is_some_and(|owner| is_type_owner_kind(owner.canonical_kind_hint))
}

pub(super) fn endpoint(raw: &str) -> Result<IrEndpoint, String> {
    if let Some(path) = raw.strip_prefix("file:") {
        return RepositoryPath::parse(path)
            .map(|path| IrEndpoint::File { path })
            .map_err(|error| format!("invalid provider file endpoint: {error}"));
    }
    ProviderSymbolId::from_provider_native(raw)
        .map(|symbol_id| IrEndpoint::NativeSymbol { symbol_id })
        .map_err(|error| format!("invalid provider symbol endpoint: {error}"))
}

pub(super) fn remap_provider_alias(
    endpoint: IrEndpoint,
    aliases: &BTreeMap<ProviderSymbolId, ProviderSymbolId>,
) -> IrEndpoint {
    match endpoint {
        IrEndpoint::NativeSymbol { symbol_id } => IrEndpoint::NativeSymbol {
            symbol_id: aliases.get(&symbol_id).cloned().unwrap_or(symbol_id),
        },
        endpoint => endpoint,
    }
}

/// A provider may use its zero-width file sentinel as the caller for a
/// top-level callback (notably test callbacks). The sentinel is deliberately
/// not a canonical definition, but the provider's exact call-site range still
/// proves a file-scoped call. Re-anchor only that source endpoint to the exact
/// manifest file. A discarded target, or any non-call relation involving a
/// discarded endpoint, remains unresolved and is not emitted.
pub(super) fn retain_relation_endpoints(
    relation: &RelationOutput,
    capability: AnalysisCapability,
    source: IrEndpoint,
    target: IrEndpoint,
    discarded_definition_ids: &BTreeSet<ProviderSymbolId>,
) -> Option<(IrEndpoint, IrEndpoint)> {
    let target_discarded = matches!(
        &target,
        IrEndpoint::NativeSymbol { symbol_id }
            if discarded_definition_ids.contains(symbol_id)
    );
    if target_discarded {
        return None;
    }
    let source_discarded = matches!(
        &source,
        IrEndpoint::NativeSymbol { symbol_id }
            if discarded_definition_ids.contains(symbol_id)
    );
    if !source_discarded {
        return Some((source, target));
    }
    if capability != AnalysisCapability::DirectCalls {
        return None;
    }
    let path = RepositoryPath::parse(&relation.path).ok()?;
    Some((IrEndpoint::File { path }, target))
}

pub(super) fn relation_sort_key(relation: &IrRelation) -> (String, String, u8, String) {
    (
        endpoint_key(&relation.source),
        endpoint_key(&relation.target),
        relation_kind_rank(relation.kind),
        relation
            .evidence_ids
            .first()
            .map(ToString::to_string)
            .unwrap_or_default(),
    )
}

pub(super) fn endpoint_key(endpoint: &IrEndpoint) -> String {
    match endpoint {
        IrEndpoint::NativeSymbol { symbol_id } => format!("symbol:{}", symbol_id.as_str()),
        IrEndpoint::File { path } => format!("file:{}", path.as_str()),
        IrEndpoint::Structure {
            unit_id,
            kind,
            qualified_name,
        } => format!(
            "structure:{}:{}:{}",
            unit_id.as_str(),
            kind.as_str(),
            qualified_name
        ),
    }
}

pub(super) fn relation_kind_rank(kind: LanguageRelationKind) -> u8 {
    use LanguageRelationKind::*;
    // These are stable wire-order IDs, not compact enum ordinals. Rank 5 was
    // the retired generic `references` relation. Never renumber surviving
    // kinds when a relation is removed: capability-specific set digests must
    // not change because an unrelated vocabulary entry was hard-cut.
    match kind {
        Contains => 0,
        Declares => 1,
        BelongsTo => 2,
        Imports => 3,
        Exports => 4,
        Calls => 6,
        Constructs => 7,
        Extends => 8,
        Implements => 9,
        MixesIn => 10,
        Overrides => 11,
        UsesType => 12,
        Tests => 13,
        ExecutesQuery => 14,
        Reads => 15,
        Writes => 16,
    }
}
