use super::definitions::{
    canonical_definition_kind, definition_display_name, is_variable_definition_kind,
    normalized_optional_text, reconcile_definition_drafts, reconcile_definition_inventory,
    source_flags, DefinitionDraft,
};
use super::{
    emit_unit_timing, evidence_producer, DefinitionAudit, DefinitionMetadataAuditEntry,
    UnitAdapterInput,
};
use crate::{normalize_scip_language, DocumentOutput, OccurrenceOutput};
use codebase_fact_model::analysis::AnalysisUnit;
use codebase_fact_model::evidence::{EvidenceKind, EvidenceLocation, FactEvidence};
use codebase_fact_model::fact_graph::{FactNodeKind, Visibility};
use codebase_fact_model::identity::{EvidenceId, ProviderSymbolId};
use codebase_fact_model::language_ir::IrDefinition;
use codebase_fact_model::source::{RepositoryPath, SourceSpan};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::time::Instant;

pub(super) struct ReconciledUnitDefinitions<'a> {
    pub(super) unit_documents: Vec<&'a DocumentOutput>,
    pub(super) document_paths: BTreeSet<&'a str>,
    pub(super) evidence: BTreeMap<EvidenceId, FactEvidence>,
    pub(super) definitions: BTreeMap<ProviderSymbolId, DefinitionDraft>,
    pub(super) definition_spans: BTreeMap<ProviderSymbolId, SourceSpan>,
    pub(super) provider_definition_aliases: BTreeMap<ProviderSymbolId, ProviderSymbolId>,
    pub(super) discarded_definition_ids: BTreeSet<ProviderSymbolId>,
    pub(super) omitted_definition_count: u64,
    pub(super) definition_records: Vec<IrDefinition>,
    pub(super) definition_audit: DefinitionAudit,
}

pub(super) fn reconcile_unit_definitions<'a>(
    input: &'a UnitAdapterInput<'a>,
    assigned: &BTreeSet<RepositoryPath>,
    syntax_definitions: &BTreeMap<RepositoryPath, Vec<super::SyntaxDefinition>>,
    mut definition_audit: DefinitionAudit,
    timing_enabled: bool,
    unit_started: Instant,
) -> Result<ReconciledUnitDefinitions<'a>, String> {
    let mut phase_started = Instant::now();
    let mut unit_documents = input
        .documents
        .iter()
        .filter(|document| {
            normalize_scip_language(&document.language, input.unit.language.as_str())
                == input.unit.language.as_str()
                && RepositoryPath::parse(&document.path).is_ok_and(|path| assigned.contains(&path))
        })
        .collect::<Vec<_>>();
    unit_documents.sort_by(|left, right| left.path.cmp(&right.path));
    let document_paths = unit_documents
        .iter()
        .map(|document| document.path.as_str())
        .collect::<BTreeSet<_>>();
    let mut evidence = BTreeMap::<EvidenceId, FactEvidence>::new();
    let mut definitions = BTreeMap::<ProviderSymbolId, DefinitionDraft>::new();
    let mut definition_spans = BTreeMap::<ProviderSymbolId, SourceSpan>::new();
    let mut ignored_provider_definition_ids = BTreeSet::<ProviderSymbolId>::new();
    let mut omitted_definition_count = 0_u64;

    for document in &unit_documents {
        let path = RepositoryPath::parse(&document.path)
            .map_err(|error| format!("provider returned invalid document path: {error}"))?;
        let manifest_file = input
            .manifest_files
            .get(&path)
            .copied()
            .ok_or_else(|| format!("provider document is absent from manifest: {path}"))?;
        let coordinates = super::SourceCoordinates::load(input.project_root, manifest_file)?;
        let definition_occurrences_by_symbol = document
            .occurrences
            .iter()
            .filter(|occurrence| occurrence.definition)
            .fold(
                HashMap::<&str, &OccurrenceOutput>::new(),
                |mut index, occurrence| {
                    index
                        .entry(occurrence.symbol.as_str())
                        .and_modify(|current| {
                            if occurrence.range < current.range {
                                *current = occurrence;
                            }
                        })
                        .or_insert(occurrence);
                    index
                },
            );
        let mut symbols = document.symbols.iter().collect::<Vec<_>>();
        symbols.sort_by(|left, right| left.symbol.cmp(&right.symbol));
        for symbol in symbols {
            let kind = canonical_definition_kind(&symbol.kind);
            let field_candidate = kind.is_none() && is_variable_definition_kind(&symbol.kind);
            let Some(kind) = kind.or(field_candidate.then_some(FactNodeKind::Field)) else {
                continue;
            };
            let Some(occurrence) = definition_occurrences_by_symbol
                .get(symbol.symbol.as_str())
                .copied()
            else {
                omitted_definition_count += 1;
                continue;
            };
            let Ok(symbol_id) = ProviderSymbolId::from_provider_native(&symbol.symbol) else {
                // An empty provider identity cannot be joined safely. Omit only
                // that definition; one malformed provider record must not abort
                // every other language and unit in the repository.
                omitted_definition_count += 1;
                continue;
            };
            let span = match coordinates.span(&occurrence.range, input.provider.protocol) {
                Ok(span) => span,
                Err(_) => {
                    omitted_definition_count += 1;
                    continue;
                }
            };
            // Zero-width document sentinels are not source definitions and
            // cannot carry verifiable definition evidence.
            if span.start.byte_offset == span.end.byte_offset {
                omitted_definition_count += 1;
                ignored_provider_definition_ids.insert(symbol_id);
                continue;
            }
            let fact_evidence = FactEvidence::new(
                EvidenceKind::SourceDefinition,
                evidence_producer(input.provider, "provider-definition"),
                EvidenceLocation::Source { span: span.clone() },
                None,
            )
            .map_err(|error| format!("cannot build definition evidence: {error}"))?;
            let evidence_id = fact_evidence.id.clone();
            evidence.insert(evidence_id.clone(), fact_evidence);
            let parent = symbol
                .enclosing_symbol
                .as_deref()
                .and_then(|raw| ProviderSymbolId::from_provider_native(raw).ok());
            definitions
                .entry(symbol_id.clone())
                .or_insert_with(|| DefinitionDraft {
                    symbol_id: symbol_id.clone(),
                    native_kind: symbol.kind.clone(),
                    canonical_kind_hint: kind,
                    // Provider-native identities are opaque. Contract-safe
                    // values remain readable; unsafe values use the exact same
                    // deterministic identity as every relation endpoint.
                    qualified_name: symbol_id.as_str().to_string(),
                    display_name: definition_display_name(symbol),
                    signature: normalized_optional_text(symbol.signature.as_deref()),
                    visibility: Visibility::Unknown,
                    parent_symbol_id: parent,
                    definition_evidence_id: evidence_id,
                    flags: source_flags(manifest_file.file_kind),
                    field_candidate,
                    path: path.clone(),
                    provider_range: occurrence.range.clone(),
                    syntax_match: None,
                });
            definition_spans.entry(symbol_id).or_insert(span);
        }
    }
    emit_unit_timing(
        timing_enabled,
        input.unit,
        "provider_definitions",
        phase_started,
        unit_started,
    );
    phase_started = Instant::now();

    let provider_definition_ids = definitions.keys().cloned().collect::<BTreeSet<_>>();
    let provider_definition_aliases = reconcile_definition_inventory(
        input.unit,
        input.provider.protocol,
        syntax_definitions,
        &mut definitions,
        &mut definition_audit,
    );
    reconcile_definition_drafts(input.unit.language, &mut definitions);
    record_definition_metadata(input.unit, &definitions, &mut definition_audit);
    omitted_definition_count += definition_audit.blocking_count();
    let definition_ids = definitions.keys().cloned().collect::<BTreeSet<_>>();
    let mut discarded_definition_ids = provider_definition_ids
        .difference(&definition_ids)
        .cloned()
        .collect::<BTreeSet<_>>();
    discarded_definition_ids.extend(ignored_provider_definition_ids);
    let retained_definition_evidence = definitions
        .values()
        .map(|draft| draft.definition_evidence_id.clone())
        .collect::<BTreeSet<_>>();
    evidence.retain(|id, _| retained_definition_evidence.contains(id));
    let mut definition_records = definitions
        .values()
        .cloned()
        .map(|draft| IrDefinition {
            unit_id: input.unit.id.clone(),
            symbol_id: draft.symbol_id,
            native_kind: draft.native_kind,
            canonical_kind_hint: draft.canonical_kind_hint,
            qualified_name: draft.qualified_name,
            display_name: draft.display_name,
            signature: draft.signature,
            visibility: draft.visibility,
            parent_symbol_id: draft
                .parent_symbol_id
                .filter(|parent| definition_ids.contains(parent)),
            definition_evidence_id: draft.definition_evidence_id,
            flags: draft.flags,
        })
        .collect::<Vec<_>>();
    definition_records.sort_by(|left, right| left.symbol_id.cmp(&right.symbol_id));
    emit_unit_timing(
        timing_enabled,
        input.unit,
        "definition_reconciliation",
        phase_started,
        unit_started,
    );

    Ok(ReconciledUnitDefinitions {
        unit_documents,
        document_paths,
        evidence,
        definitions,
        definition_spans,
        provider_definition_aliases,
        discarded_definition_ids,
        omitted_definition_count,
        definition_records,
        definition_audit,
    })
}

fn record_definition_metadata(
    unit: &AnalysisUnit,
    definitions: &BTreeMap<ProviderSymbolId, DefinitionDraft>,
    audit: &mut DefinitionAudit,
) {
    let owners = definitions
        .iter()
        .map(|(id, draft)| (id, draft.display_name.as_str()))
        .collect::<BTreeMap<_, _>>();
    let mut entries = definitions
        .values()
        .map(|draft| DefinitionMetadataAuditEntry {
            unit_id: unit.id.as_str().to_string(),
            language: unit.language,
            path: draft.path.clone(),
            kind: draft.canonical_kind_hint,
            name: draft.display_name.clone(),
            owner: draft
                .parent_symbol_id
                .as_ref()
                .and_then(|parent| owners.get(parent).copied())
                .map(str::to_string),
            signature: draft.signature.clone(),
            visibility: draft.visibility,
        })
        .collect::<Vec<_>>();
    entries.sort();
    for entry in &entries {
        let callable = matches!(
            entry.kind,
            FactNodeKind::Function | FactNodeKind::Method | FactNodeKind::Constructor
        );
        audit.metadata_definition_count += 1;
        audit.callable_definition_count += u64::from(callable);
        audit.callable_signature_count += u64::from(callable && entry.signature.is_some());
        audit.known_visibility_count += u64::from(entry.visibility != Visibility::Unknown);
        audit.metadata_keys.push(definition_metadata_key(entry));
    }
    audit.metadata_keys.sort();
    audit.metadata_keys.dedup();
    audit.metadata_entries.extend(entries);
    audit.metadata_entries.sort();
    audit.metadata_entries.dedup();
}

fn definition_metadata_key(entry: &DefinitionMetadataAuditEntry) -> String {
    format!(
        "{}\t{}\t{}\t{}\t{}\t{}",
        entry.path.as_str(),
        entry.kind.as_str(),
        entry.name,
        entry.owner.as_deref().unwrap_or("-"),
        visibility_name(entry.visibility),
        entry.signature.as_deref().unwrap_or("-")
    )
}

const fn visibility_name(visibility: Visibility) -> &'static str {
    match visibility {
        Visibility::Public => "public",
        Visibility::Protected => "protected",
        Visibility::Internal => "internal",
        Visibility::Package => "package",
        Visibility::Private => "private",
        Visibility::Unknown => "unknown",
    }
}
