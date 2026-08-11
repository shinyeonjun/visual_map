use super::{
    collect_import_drafts, record_definition_inventory_failure, record_import_file_failure,
    DefinitionAudit, ImportAudit, ImportAuditOutcome, ImportDraft, UnitAdapterInput,
    UnitSourceInventory,
};
use crate::static_pipeline::language_ir::definition_inventory::{
    inventory_definitions_from_root, SyntaxDefinition,
};
use crate::static_pipeline::language_ir::imports::inventory_imports_from_root;
use crate::static_pipeline::language_ir::source_coordinates::SourceCoordinates;
use crate::static_pipeline::language_ir::sql_literals::{
    inventory_sql_query_literals_from_root, SqlQuerySite,
};
use crate::static_pipeline::language_ir::syntax::parse_tree;
use crate::static_pipeline::language_ir::type_relations::{
    inventory_type_relation_sites_from_root, inventory_type_use_sites_from_root,
    SyntaxTypeRelationSite, SyntaxTypeUseSite,
};
use crate::{inventory_call_sites_from_root, SyntaxCallSite};
use codebase_fact_model::coverage::GapCode;
use codebase_fact_model::source::RepositoryPath;
use codebase_fact_model::source_manifest::SourceManifestFile;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::env;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    mpsc,
};
use std::thread;
use std::time::{Duration, Instant};

pub(super) fn inventory_unit_sources(
    input: &UnitAdapterInput<'_>,
    assigned: &BTreeSet<RepositoryPath>,
    timing_enabled: bool,
) -> Result<UnitSourceInventory, String> {
    let inventory_started = Instant::now();
    let worker_count = source_inventory_worker_count(input, assigned);
    let file_inventories = inventory_source_files(input, assigned, worker_count)?;
    let mut definition_audit = DefinitionAudit::default();
    let mut syntax_definitions = BTreeMap::<RepositoryPath, Vec<SyntaxDefinition>>::new();
    let mut syntax_call_sites = BTreeMap::<RepositoryPath, Vec<SyntaxCallSite>>::new();
    let mut syntax_type_relations = BTreeMap::<RepositoryPath, Vec<SyntaxTypeRelationSite>>::new();
    let mut syntax_type_uses = BTreeMap::<RepositoryPath, Vec<SyntaxTypeUseSite>>::new();
    let mut syntax_sql_queries = BTreeMap::<RepositoryPath, Vec<SqlQuerySite>>::new();
    let mut type_relation_inventory_failed_files = BTreeSet::<RepositoryPath>::new();
    let mut import_audit = ImportAudit::for_language(input.unit.language);
    let mut import_drafts = Vec::new();
    let mut timings = SourceInventoryTimings::default();

    // Results are returned in repository-path order. Merging them in that
    // stable order preserves the exact serial stream while allowing the
    // expensive file load/parse/inventory work to run concurrently.
    for mut file in file_inventories {
        timings.merge(file.timings);
        definition_audit.absorb(file.definition_audit);
        import_audit.absorb(file.import_audit);
        import_drafts.append(&mut file.import_drafts);
        if file.type_relation_inventory_failed {
            type_relation_inventory_failed_files.insert(file.path.clone());
        }
        if let Some(definitions) = file.definitions {
            syntax_definitions.insert(file.path.clone(), definitions);
        }
        if let Some(call_sites) = file.call_sites {
            syntax_call_sites.insert(file.path.clone(), call_sites);
        }
        if let Some(type_relations) = file.type_relations {
            syntax_type_relations.insert(file.path.clone(), type_relations);
        }
        if let Some(type_uses) = file.type_uses {
            syntax_type_uses.insert(file.path.clone(), type_uses);
        }
        if let Some(sql_queries) = file.sql_queries {
            if !sql_queries.is_empty() {
                syntax_sql_queries.insert(file.path, sql_queries);
            }
        }
    }

    // Canonicalize cumulative inventories once. Re-sorting after every file
    // made large Java and C# units effectively quadratic without changing the
    // accepted facts.
    definition_audit.canonicalize();
    import_audit.canonicalize();
    if timing_enabled {
        eprintln!(
            "timing stage=language_ir_source_inventory language={} unit={} workers={} wall_ms={} load_cpu_ms={} parse_cpu_ms={} definitions_cpu_ms={} call_sites_cpu_ms={} type_relations_cpu_ms={} type_uses_cpu_ms={} sql_literals_cpu_ms={} imports_cpu_ms={} import_resolution_cpu_ms={}",
            input.unit.language.as_str(),
            input.unit.id.as_str(),
            worker_count,
            inventory_started.elapsed().as_millis(),
            timings.source_load.as_millis(),
            timings.source_parse.as_millis(),
            timings.definition_inventory.as_millis(),
            timings.call_site_inventory.as_millis(),
            timings.type_relation_inventory.as_millis(),
            timings.type_use_inventory.as_millis(),
            timings.sql_literal_inventory.as_millis(),
            timings.import_inventory.as_millis(),
            timings.import_resolution.as_millis(),
        );
    }

    Ok(UnitSourceInventory {
        definition_audit,
        syntax_definitions,
        syntax_call_sites,
        syntax_type_relations,
        syntax_type_uses,
        syntax_sql_queries,
        type_relation_inventory_failed_files,
        import_audit,
        import_drafts,
    })
}

#[derive(Default)]
struct SourceInventoryTimings {
    source_load: Duration,
    source_parse: Duration,
    definition_inventory: Duration,
    call_site_inventory: Duration,
    type_relation_inventory: Duration,
    type_use_inventory: Duration,
    sql_literal_inventory: Duration,
    import_inventory: Duration,
    import_resolution: Duration,
}

impl SourceInventoryTimings {
    fn merge(&mut self, other: Self) {
        self.source_load += other.source_load;
        self.source_parse += other.source_parse;
        self.definition_inventory += other.definition_inventory;
        self.call_site_inventory += other.call_site_inventory;
        self.type_relation_inventory += other.type_relation_inventory;
        self.type_use_inventory += other.type_use_inventory;
        self.sql_literal_inventory += other.sql_literal_inventory;
        self.import_inventory += other.import_inventory;
        self.import_resolution += other.import_resolution;
    }
}

struct SourceFileInventory {
    path: RepositoryPath,
    definition_audit: DefinitionAudit,
    definitions: Option<Vec<SyntaxDefinition>>,
    call_sites: Option<Vec<SyntaxCallSite>>,
    type_relations: Option<Vec<SyntaxTypeRelationSite>>,
    type_uses: Option<Vec<SyntaxTypeUseSite>>,
    sql_queries: Option<Vec<SqlQuerySite>>,
    type_relation_inventory_failed: bool,
    import_audit: ImportAudit,
    import_drafts: Vec<ImportDraft>,
    timings: SourceInventoryTimings,
}

fn source_inventory_worker_count(
    input: &UnitAdapterInput<'_>,
    assigned: &BTreeSet<RepositoryPath>,
) -> usize {
    const MIN_PARALLEL_FILES: usize = 32;
    const AST_BASELINE_MEMORY_MB: usize = 256;
    const AST_SOURCE_EXPANSION_FACTOR: u64 = 24;
    const MEBIBYTE: u64 = 1024 * 1024;

    if assigned.len() < MIN_PARALLEL_FILES {
        return 1;
    }
    let detected = thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(1);
    let cpu_workers = detected.saturating_sub(1).clamp(1, 8);
    let requested = env::var("CODE_MEMORY_MAX_LANGUAGE_IR_WORKERS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| (1..=16).contains(value))
        .unwrap_or(cpu_workers);
    let largest_source_bytes = assigned
        .iter()
        .filter_map(|path| input.manifest_files.get(path).copied())
        .map(|file| file.byte_size)
        .max()
        .unwrap_or(0);
    let estimated_worker_mb = largest_source_bytes
        .saturating_mul(AST_SOURCE_EXPANSION_FACTOR)
        .div_ceil(MEBIBYTE)
        .max(AST_BASELINE_MEMORY_MB as u64) as usize;
    let memory_workers = crate::provider_memory_budget_mb()
        .map(|budget| (budget / estimated_worker_mb).max(1))
        .unwrap_or(2);
    requested
        .min(cpu_workers)
        .min(memory_workers)
        .min(assigned.len())
        .max(1)
}

fn inventory_source_files(
    input: &UnitAdapterInput<'_>,
    assigned: &BTreeSet<RepositoryPath>,
    worker_count: usize,
) -> Result<Vec<SourceFileInventory>, String> {
    let files = assigned
        .iter()
        .filter_map(|path| {
            input
                .manifest_files
                .get(path)
                .copied()
                .map(|manifest_file| (path, manifest_file))
        })
        .collect::<Vec<_>>();
    if worker_count == 1 {
        return Ok(files
            .into_iter()
            .map(|(path, manifest_file)| inventory_source_file(input, path, manifest_file))
            .collect());
    }

    let next = AtomicUsize::new(0);
    let (sender, receiver) = mpsc::channel();
    thread::scope(|scope| -> Result<(), String> {
        let mut workers = Vec::with_capacity(worker_count);
        for _ in 0..worker_count {
            let sender = sender.clone();
            let next = &next;
            let files = &files;
            workers.push(scope.spawn(move || loop {
                let index = next.fetch_add(1, Ordering::Relaxed);
                let Some((path, manifest_file)) = files.get(index).copied() else {
                    break;
                };
                let result = inventory_source_file(input, path, manifest_file);
                if sender.send((index, result)).is_err() {
                    break;
                }
            }));
        }
        drop(sender);
        for worker in workers {
            worker
                .join()
                .map_err(|_| "Language IR source-inventory worker panicked".to_string())?;
        }
        Ok(())
    })?;

    let mut ordered = std::iter::repeat_with(|| None)
        .take(files.len())
        .collect::<Vec<_>>();
    for (index, result) in receiver {
        let slot = ordered.get_mut(index).ok_or_else(|| {
            format!("Language IR source-inventory worker returned invalid index {index}")
        })?;
        if slot.replace(result).is_some() {
            return Err(format!(
                "Language IR source-inventory worker returned duplicate index {index}"
            ));
        }
    }
    ordered
        .into_iter()
        .enumerate()
        .map(|(index, result)| {
            result.ok_or_else(|| {
                format!("Language IR source-inventory file {index} produced no result")
            })
        })
        .collect()
}

fn inventory_source_file(
    input: &UnitAdapterInput<'_>,
    path: &RepositoryPath,
    manifest_file: &SourceManifestFile,
) -> SourceFileInventory {
    let mut result = SourceFileInventory {
        path: path.clone(),
        definition_audit: DefinitionAudit::default(),
        definitions: None,
        call_sites: None,
        type_relations: None,
        type_uses: None,
        sql_queries: None,
        type_relation_inventory_failed: false,
        import_audit: ImportAudit::for_language(input.unit.language),
        import_drafts: Vec::new(),
        timings: SourceInventoryTimings::default(),
    };
    let operation_started = Instant::now();
    let coordinates = match SourceCoordinates::load(input.project_root, manifest_file) {
        Ok(coordinates) => coordinates,
        Err(_) => {
            result.timings.source_load += operation_started.elapsed();
            record_definition_inventory_failure(input.unit, path, &mut result.definition_audit);
            record_import_file_failure(
                input.unit,
                path,
                ImportAuditOutcome::InventoryFailed,
                GapCode::ProviderExecutionIncomplete,
                &mut result.import_audit,
            );
            result.type_relation_inventory_failed = true;
            return result;
        }
    };
    result.timings.source_load += operation_started.elapsed();

    let operation_started = Instant::now();
    let tree = match parse_tree(
        input.unit.language.as_str(),
        path.as_str(),
        coordinates.text(),
        "shared-static-inventory",
    ) {
        Ok(tree) => tree,
        Err(_) => {
            result.timings.source_parse += operation_started.elapsed();
            record_definition_inventory_failure(input.unit, path, &mut result.definition_audit);
            record_import_file_failure(
                input.unit,
                path,
                ImportAuditOutcome::InventoryFailed,
                GapCode::ProviderExecutionIncomplete,
                &mut result.import_audit,
            );
            if input
                .import_index
                .metadata_failed(input.unit.language, path)
            {
                record_import_file_failure(
                    input.unit,
                    path,
                    ImportAuditOutcome::MetadataUnavailable,
                    GapCode::MissingProjectMetadata,
                    &mut result.import_audit,
                );
            }
            result.type_relation_inventory_failed = true;
            return result;
        }
    };
    result.timings.source_parse += operation_started.elapsed();

    let operation_started = Instant::now();
    let definitions = inventory_definitions_from_root(
        input.unit.language.as_str(),
        tree.root_node(),
        coordinates.text(),
    );
    result.timings.definition_inventory += operation_started.elapsed();
    let definition_names_by_range =
        definitions
            .iter()
            .fold(HashMap::<Vec<i32>, &str>::new(), |mut index, definition| {
                index
                    .entry(definition.name_utf8_range.clone())
                    .or_insert(definition.name.as_str());
                index
            });
    for definition in &definitions {
        let parent = definition
            .parent_name_utf8_range
            .as_ref()
            .and_then(|parent_range| definition_names_by_range.get(parent_range).copied())
            .unwrap_or("-");
        result.definition_audit.definition_keys.push(format!(
            "{}\t{}\t{}\t{parent}",
            path.as_str(),
            definition.kind.as_str(),
            definition.name
        ));
    }
    result.definition_audit.syntax_definition_count = definitions.len() as u64;
    result.definition_audit.owned_syntax_definition_count = definitions
        .iter()
        .filter(|definition| definition.parent_name_utf8_range.is_some())
        .count() as u64;

    let operation_started = Instant::now();
    let call_sites = inventory_call_sites_from_root(
        input.unit.language.as_str(),
        tree.root_node(),
        coordinates.text(),
    );
    result.timings.call_site_inventory += operation_started.elapsed();

    let operation_started = Instant::now();
    let type_relations = inventory_type_relation_sites_from_root(
        input.unit.language,
        tree.root_node(),
        coordinates.text(),
    );
    result.timings.type_relation_inventory += operation_started.elapsed();
    let operation_started = Instant::now();
    let type_uses = inventory_type_use_sites_from_root(
        input.unit.language,
        tree.root_node(),
        coordinates.text(),
        &definitions,
        &type_relations,
    );
    result.timings.type_use_inventory += operation_started.elapsed();
    let operation_started = Instant::now();
    let sql_queries = inventory_sql_query_literals_from_root(
        input.unit.language.as_str(),
        tree.root_node(),
        coordinates.text(),
    );
    result.timings.sql_literal_inventory += operation_started.elapsed();
    let operation_started = Instant::now();
    let import_sites =
        inventory_imports_from_root(input.unit.language, tree.root_node(), coordinates.text());
    result.timings.import_inventory += operation_started.elapsed();

    if input
        .import_index
        .metadata_failed(input.unit.language, path)
    {
        record_import_file_failure(
            input.unit,
            path,
            ImportAuditOutcome::MetadataUnavailable,
            GapCode::MissingProjectMetadata,
            &mut result.import_audit,
        );
    }
    let operation_started = Instant::now();
    collect_import_drafts(
        input.unit,
        input.import_index,
        path,
        &coordinates,
        import_sites,
        &mut result.import_audit,
        &mut result.import_drafts,
    );
    result.timings.import_resolution += operation_started.elapsed();
    result.definitions = Some(definitions);
    result.call_sites = Some(call_sites);
    result.type_relations = Some(type_relations);
    result.type_uses = Some(type_uses);
    result.sql_queries = Some(sql_queries);
    result
}
