//! Promotes exact SQL literals into typed Language IR query/table facts.
//!
//! The syntax inventory has already rejected interpolation and dynamic string
//! construction.  This adapter resolves only the enclosing declaration by an
//! exact tree-sitter declaration range; if no declaration owns the literal,
//! the source file remains the truthful execution owner.

use super::definitions::{source_flags, DefinitionDraft};
use super::{
    BTreeMap, EvidenceId, EvidenceKind, EvidenceLocation, EvidenceProducer, EvidenceProducerKind,
    FactEvidence, FactNodeKind, FactTruth, IrDefinition, IrEndpoint, IrRelation,
    LanguageRelationKind, ProviderSymbolId, RepositoryPath, ResolutionMethod, SourceCoordinates,
    SqlQuerySite, SqlTableAccessKind, SyntaxDefinition, UnitAdapterInput, Visibility,
};
use crate::static_pipeline::language_ir::syntax::range_contains;
use codebase_fact_model::fact_graph::DispatchKind;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct SqlQueryAudit {
    pub(super) eligible_query_count: u64,
    pub(super) accepted_query_count: u64,
    pub(super) query_definition_count: u64,
    pub(super) table_definition_count: u64,
    pub(super) emitted_relation_count: u64,
    pub(super) callable_owner_count: u64,
    pub(super) inventory_failed_file_count: u64,
}

pub(super) struct SqlQueryEmission {
    pub(super) evidence: BTreeMap<EvidenceId, FactEvidence>,
    pub(super) definitions: Vec<IrDefinition>,
    pub(super) relations: Vec<IrRelation>,
    pub(super) audit: SqlQueryAudit,
}

pub(super) fn emit_sql_query_facts(
    input: &UnitAdapterInput<'_>,
    syntax_definitions: &BTreeMap<RepositoryPath, Vec<SyntaxDefinition>>,
    query_sites: &BTreeMap<RepositoryPath, Vec<SqlQuerySite>>,
    definitions: &BTreeMap<ProviderSymbolId, DefinitionDraft>,
) -> Result<SqlQueryEmission, String> {
    let mut evidence = BTreeMap::new();
    let mut query_definitions = Vec::new();
    let mut table_definitions = BTreeMap::<ProviderSymbolId, IrDefinition>::new();
    let mut relations = Vec::new();
    let mut audit = SqlQueryAudit::default();

    for (path, sites) in query_sites {
        let manifest_file =
            input.manifest_files.get(path).copied().ok_or_else(|| {
                format!("SQL literal path is absent from Source Manifest: {path}")
            })?;
        let coordinates = SourceCoordinates::load(input.project_root, manifest_file)?;
        for site in sites {
            audit.eligible_query_count += 1;
            /*
              A syntax site whose range does not land on a character boundary
              is an internal disagreement between the scanner and the verified
              source, not a reason to lose the whole repository's analysis. The
              site is dropped and the eligible/accepted gap already reports it,
              which is what the capability receipt is for.
            */
            let Ok(span) = coordinates.utf8_span(&site.utf8_range) else {
                continue;
            };
            let query_evidence = FactEvidence::new(
                EvidenceKind::QueryLiteral,
                EvidenceProducer {
                    kind: EvidenceProducerKind::SyntaxParser,
                    name: "tree-sitter-sql-literal-inventory".to_string(),
                    version: Some("1".to_string()),
                    strategy: Some("exact-static-literal".to_string()),
                },
                EvidenceLocation::Source { span },
                Some("Static SQL literal with an exact table reference".to_string()),
            )
            .map_err(|error| format!("cannot build SQL literal evidence: {error}"))?;
            let evidence_id = query_evidence.id.clone();
            evidence.insert(evidence_id.clone(), query_evidence);

            let query_symbol_id = query_symbol_id(input, path, site)?;
            let line = site.utf8_range.first().copied().unwrap_or_default();
            let column = site.utf8_range.get(1).copied().unwrap_or_default();
            let query_name = format!(
                "sql.query:{}:{line}:{column}:{}",
                path.as_str(),
                site.digest
            );
            query_definitions.push(IrDefinition {
                unit_id: input.unit.id.clone(),
                symbol_id: query_symbol_id.clone(),
                native_kind: "sql_query_literal".to_string(),
                canonical_kind_hint: FactNodeKind::Query,
                qualified_name: query_name,
                display_name: site.display_name(),
                signature: None,
                visibility: Visibility::Unknown,
                parent_symbol_id: None,
                definition_evidence_id: evidence_id.clone(),
                flags: source_flags(manifest_file.file_kind),
            });
            audit.accepted_query_count += 1;
            audit.query_definition_count += 1;

            let owner =
                exact_enclosing_definition(path, &site.utf8_range, syntax_definitions, definitions);
            if owner.is_some() {
                audit.callable_owner_count += 1;
            }
            relations.push(sql_relation(
                input,
                owner
                    .map(|symbol_id| IrEndpoint::NativeSymbol { symbol_id })
                    .unwrap_or_else(|| IrEndpoint::File { path: path.clone() }),
                IrEndpoint::NativeSymbol {
                    symbol_id: query_symbol_id.clone(),
                },
                LanguageRelationKind::ExecutesQuery,
                evidence_id.clone(),
            ));

            for access in &site.tables {
                let table_symbol_id = table_symbol_id(input, &access.table)?;
                table_definitions
                    .entry(table_symbol_id.clone())
                    .or_insert_with(|| IrDefinition {
                        unit_id: input.unit.id.clone(),
                        symbol_id: table_symbol_id.clone(),
                        native_kind: "sql_table_reference".to_string(),
                        // Source text proves only that the application refers
                        // to this name. It does not prove that a table exists
                        // in a connected database snapshot.
                        canonical_kind_hint: FactNodeKind::TableReference,
                        qualified_name: format!("sql.table:{}", access.table),
                        display_name: access.table.clone(),
                        signature: None,
                        visibility: Visibility::Unknown,
                        parent_symbol_id: None,
                        definition_evidence_id: evidence_id.clone(),
                        flags: source_flags(manifest_file.file_kind),
                    });
                relations.push(sql_relation(
                    input,
                    IrEndpoint::NativeSymbol {
                        symbol_id: query_symbol_id.clone(),
                    },
                    IrEndpoint::NativeSymbol {
                        symbol_id: table_symbol_id,
                    },
                    match access.kind {
                        SqlTableAccessKind::Read => LanguageRelationKind::Reads,
                        SqlTableAccessKind::Write => LanguageRelationKind::Writes,
                    },
                    evidence_id.clone(),
                ));
            }
        }
    }

    audit.table_definition_count = table_definitions.len() as u64;
    query_definitions.extend(table_definitions.into_values());
    query_definitions.sort_by(|left, right| left.symbol_id.cmp(&right.symbol_id));
    relations.sort_by(|left, right| {
        (
            endpoint_key(&left.source),
            endpoint_key(&left.target),
            left.kind,
            &left.evidence_ids,
        )
            .cmp(&(
                endpoint_key(&right.source),
                endpoint_key(&right.target),
                right.kind,
                &right.evidence_ids,
            ))
    });
    relations.dedup();
    audit.emitted_relation_count = relations.len() as u64;
    Ok(SqlQueryEmission {
        evidence,
        definitions: query_definitions,
        relations,
        audit,
    })
}

fn exact_enclosing_definition(
    path: &RepositoryPath,
    query_range: &[i32],
    syntax_definitions: &BTreeMap<RepositoryPath, Vec<SyntaxDefinition>>,
    definitions: &BTreeMap<ProviderSymbolId, DefinitionDraft>,
) -> Option<ProviderSymbolId> {
    let syntax = syntax_definitions.get(path)?;
    definitions
        .values()
        .filter_map(|definition| {
            if &definition.path != path {
                return None;
            }
            let index = definition.syntax_match?;
            let declaration = syntax.get(index)?.declaration_utf8_range.as_slice();
            range_contains(declaration, query_range)
                .then_some((range_weight(declaration), definition.symbol_id.clone()))
        })
        .min_by(|left, right| left.cmp(right))
        .map(|(_, symbol_id)| symbol_id)
}

fn range_weight(range: &[i32]) -> (i64, i64) {
    match range {
        [_line, start, end] => (0, i64::from(end.saturating_sub(*start))),
        [start_line, start_column, end_line, end_column, ..] => (
            i64::from(end_line.saturating_sub(*start_line)),
            i64::from(end_column.saturating_sub(*start_column)),
        ),
        _ => (i64::MAX, i64::MAX),
    }
}

fn query_symbol_id(
    input: &UnitAdapterInput<'_>,
    path: &RepositoryPath,
    site: &SqlQuerySite,
) -> Result<ProviderSymbolId, String> {
    ProviderSymbolId::parse(format!(
        "static-sql-query:{}:{}:{}:{}:{}",
        input.unit.id,
        path,
        site.utf8_range.first().copied().unwrap_or_default(),
        site.utf8_range.get(1).copied().unwrap_or_default(),
        site.digest
    ))
    .map_err(|error| format!("cannot build SQL query provider identity: {error}"))
}

fn table_symbol_id(input: &UnitAdapterInput<'_>, table: &str) -> Result<ProviderSymbolId, String> {
    ProviderSymbolId::parse(format!("static-sql-table:{}:{table}", input.unit.id))
        .map_err(|error| format!("cannot build SQL table provider identity: {error}"))
}

fn sql_relation(
    input: &UnitAdapterInput<'_>,
    source: IrEndpoint,
    target: IrEndpoint,
    kind: LanguageRelationKind,
    evidence_id: EvidenceId,
) -> IrRelation {
    IrRelation {
        unit_id: input.unit.id.clone(),
        source,
        target,
        kind,
        truth: FactTruth::Confirmed,
        resolution: ResolutionMethod::SyntaxExact,
        dispatch: DispatchKind::NotApplicable,
        semantic_context_id: input.unit.context.id.clone(),
        execution: None,
        evidence_ids: vec![evidence_id],
    }
}

fn endpoint_key(endpoint: &IrEndpoint) -> String {
    match endpoint {
        IrEndpoint::NativeSymbol { symbol_id } => format!("symbol:{}", symbol_id.as_str()),
        IrEndpoint::File { path } => format!("file:{}", path.as_str()),
        IrEndpoint::Structure {
            unit_id,
            kind,
            qualified_name,
        } => format!("structure:{unit_id}:{}:{qualified_name}", kind.as_str()),
    }
}
