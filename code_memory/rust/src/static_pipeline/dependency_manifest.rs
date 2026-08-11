//! Exact reverse-import inventory for incremental provider invalidation.
//!
//! The old architecture JSON used to be re-read solely to recover this map.
//! Language IR already contains the authoritative import facts, so incremental
//! invalidation now consumes that same sealed stream instead of rebuilding a
//! second architecture graph.

use crate::static_pipeline::language_ir::artifact::visit_language_ir_records;
use codebase_fact_model::analysis_plan::AnalysisPlan;
use codebase_fact_model::evidence::EvidenceLocation;
use codebase_fact_model::identity::{AnalysisUnitId, EvidenceId, ProviderSymbolId};
use codebase_fact_model::language_ir::{
    IrEndpoint, IrRelation, LanguageIrRecord, LanguageRelationKind,
};
use codebase_fact_model::source::RepositoryPath;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

pub(crate) fn collect_reverse_imports(
    language_ir_path: &Path,
    plan: &AnalysisPlan,
) -> Result<HashMap<String, Vec<String>>, String> {
    let mut evidence_paths = BTreeMap::<EvidenceId, RepositoryPath>::new();
    let mut definition_evidence = BTreeMap::<(AnalysisUnitId, ProviderSymbolId), EvidenceId>::new();
    let mut import_relations = Vec::<IrRelation>::new();
    visit_language_ir_records(language_ir_path, |record| {
        match record {
            LanguageIrRecord::Evidence(evidence) => {
                if let EvidenceLocation::Source { span } = evidence.location {
                    evidence_paths.insert(evidence.id, span.path);
                }
            }
            LanguageIrRecord::Definition(definition) => {
                definition_evidence.insert(
                    (definition.unit_id, definition.symbol_id),
                    definition.definition_evidence_id,
                );
            }
            LanguageIrRecord::Relation(relation)
                if relation.kind == LanguageRelationKind::Imports =>
            {
                import_relations.push(relation);
            }
            _ => {}
        }
        Ok(())
    })?;

    let definition_paths = definition_evidence
        .into_iter()
        .map(|(identity, evidence_id)| {
            let path = evidence_paths.get(&evidence_id).cloned().ok_or_else(|| {
                format!(
                    "import dependency definition {} has no source evidence",
                    identity.1
                )
            })?;
            Ok((identity, path))
        })
        .collect::<Result<BTreeMap<_, _>, String>>()?;
    let mut global_definition_paths = BTreeMap::<ProviderSymbolId, BTreeSet<RepositoryPath>>::new();
    for ((_, symbol_id), path) in &definition_paths {
        global_definition_paths
            .entry(symbol_id.clone())
            .or_default()
            .insert(path.clone());
    }
    let paths_by_unit = plan.assignments.iter().fold(
        BTreeMap::<AnalysisUnitId, BTreeSet<RepositoryPath>>::new(),
        |mut paths, assignment| {
            for unit_id in &assignment.unit_ids {
                paths
                    .entry(unit_id.clone())
                    .or_default()
                    .insert(assignment.path.clone());
            }
            paths
        },
    );

    let mut reverse = BTreeMap::<RepositoryPath, BTreeSet<RepositoryPath>>::new();
    for relation in import_relations {
        let IrEndpoint::File { path: source } = &relation.source else {
            return Err("Language IR import source is not an exact file".to_string());
        };
        let targets = import_target_paths(
            &relation,
            &definition_paths,
            &global_definition_paths,
            &paths_by_unit,
        )?;
        for target in targets {
            if &target != source {
                reverse.entry(target).or_default().insert(source.clone());
            }
        }
    }

    Ok(reverse
        .into_iter()
        .map(|(target, importers)| {
            (
                target.as_str().to_string(),
                importers
                    .into_iter()
                    .map(|path| path.as_str().to_string())
                    .collect(),
            )
        })
        .collect())
}

fn import_target_paths(
    relation: &IrRelation,
    definitions: &BTreeMap<(AnalysisUnitId, ProviderSymbolId), RepositoryPath>,
    global_definitions: &BTreeMap<ProviderSymbolId, BTreeSet<RepositoryPath>>,
    paths_by_unit: &BTreeMap<AnalysisUnitId, BTreeSet<RepositoryPath>>,
) -> Result<BTreeSet<RepositoryPath>, String> {
    match &relation.target {
        IrEndpoint::File { path } => Ok(BTreeSet::from([path.clone()])),
        IrEndpoint::Structure { unit_id, .. } => paths_by_unit
            .get(unit_id)
            .cloned()
            .ok_or_else(|| format!("Language IR import targets unknown Analysis Unit {unit_id}")),
        IrEndpoint::NativeSymbol { symbol_id } => {
            if let Some(path) = definitions.get(&(relation.unit_id.clone(), symbol_id.clone())) {
                return Ok(BTreeSet::from([path.clone()]));
            }
            let candidates = global_definitions
                .get(symbol_id)
                .cloned()
                .unwrap_or_default();
            if candidates.len() == 1 {
                return Ok(candidates);
            }
            Err(format!(
                "Language IR import target {symbol_id} does not resolve to exactly one source file"
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use codebase_fact_model::{
        fact_graph::{DispatchKind, FactTruth, ResolutionMethod},
        identity::SemanticContextId,
    };

    #[test]
    fn reverse_import_target_uses_exact_unit_identity_before_global_candidates() {
        let unit = AnalysisUnitId::from_components(&["typescript", "app"]).unwrap();
        let symbol = ProviderSymbolId::parse("scip-typescript target#").unwrap();
        let exact = RepositoryPath::parse("src/target.ts").unwrap();
        let other = RepositoryPath::parse("vendor/target.ts").unwrap();
        let relation = import_relation(
            unit.clone(),
            IrEndpoint::NativeSymbol {
                symbol_id: symbol.clone(),
            },
        );
        let definitions = BTreeMap::from([((unit, symbol.clone()), exact.clone())]);
        let globals = BTreeMap::from([(symbol, BTreeSet::from([exact.clone(), other]))]);

        assert_eq!(
            import_target_paths(&relation, &definitions, &globals, &BTreeMap::new()).unwrap(),
            BTreeSet::from([exact])
        );
    }

    #[test]
    fn reverse_import_target_rejects_an_ambiguous_global_symbol() {
        let unit = AnalysisUnitId::from_components(&["typescript", "app"]).unwrap();
        let symbol = ProviderSymbolId::parse("scip-typescript target#").unwrap();
        let relation = import_relation(
            unit,
            IrEndpoint::NativeSymbol {
                symbol_id: symbol.clone(),
            },
        );
        let globals = BTreeMap::from([(
            symbol,
            BTreeSet::from([
                RepositoryPath::parse("src/target.ts").unwrap(),
                RepositoryPath::parse("vendor/target.ts").unwrap(),
            ]),
        )]);

        let error = import_target_paths(&relation, &BTreeMap::new(), &globals, &BTreeMap::new())
            .unwrap_err();
        assert!(error.contains("exactly one source file"));
    }

    fn import_relation(unit_id: AnalysisUnitId, target: IrEndpoint) -> IrRelation {
        IrRelation {
            unit_id,
            source: IrEndpoint::File {
                path: RepositoryPath::parse("src/importer.ts").unwrap(),
            },
            target,
            kind: LanguageRelationKind::Imports,
            truth: FactTruth::Confirmed,
            resolution: ResolutionMethod::ProjectModel,
            dispatch: DispatchKind::NotApplicable,
            semantic_context_id: SemanticContextId::from_components(&["typescript", "app"])
                .unwrap(),
            execution: None,
            evidence_ids: vec![EvidenceId::from_components(&["import", "site"]).unwrap()],
        }
    }
}
