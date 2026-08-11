//! Provider job inputs plus deterministic batch rebasing and aggregation.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::{
    AnalysisCachePolicy, Diagnostic, DocumentOutput, LanguageOutput, LanguageSpec,
    ProviderUnitBatch, ProviderWorkspaceBinding, RelationOutput,
};

#[derive(Clone)]
pub(crate) struct LanguageJob {
    pub(crate) lang: LanguageSpec,
    pub(crate) project_root: PathBuf,
    pub(crate) files: Vec<PathBuf>,
    pub(crate) cache_key: String,
    pub(crate) root: PathBuf,
    pub(crate) work: PathBuf,
    pub(crate) providers_root: Option<PathBuf>,
    pub(crate) execution_scope_id: String,
    pub(crate) provider_config: Option<PathBuf>,
    pub(crate) call_ranges: Arc<HashMap<String, Vec<Vec<i32>>>>,
    pub(crate) project_excluded_files: usize,
    /// Largest source file admitted by the repository-wide Source Census.
    /// Project-aware providers may read files outside one planner shard, so a
    /// per-shard ceiling would silently discard legitimate project members.
    pub(crate) max_project_source_file_bytes: u64,
    pub(crate) writable_workspace: Option<ProviderWorkspaceBinding>,
    pub(crate) cache_policy: AnalysisCachePolicy,
}

pub(crate) fn rebase_provider_batch(
    analysis: &mut ProviderUnitBatch,
    module_root: &Path,
    project_root: &Path,
) {
    let mut symbol_prefixes = HashMap::new();
    for document in &analysis.documents {
        let global_path = rebase_relative_path(module_root, project_root, &document.path);
        let old_prefix = format!("lsp . . . {}", document.path.replace(['/', '\\'], "."));
        let new_prefix = format!("lsp . . . {}", global_path.replace(['/', '\\'], "."));
        symbol_prefixes.insert(old_prefix, new_prefix);
    }
    for document in &mut analysis.documents {
        document.path = rebase_relative_path(module_root, project_root, &document.path);
        for symbol in &mut document.symbols {
            symbol.symbol = rebase_symbol_id(&symbol.symbol, &symbol_prefixes);
        }
        for occurrence in &mut document.occurrences {
            occurrence.symbol = rebase_symbol_id(&occurrence.symbol, &symbol_prefixes);
        }
    }
    for relation in &mut analysis.relations {
        relation.path = rebase_relative_path(module_root, project_root, &relation.path);
        relation.from = rebase_symbol_id(&relation.from, &symbol_prefixes);
        relation.to = rebase_symbol_id(&relation.to, &symbol_prefixes);
    }
    for diagnostic in &mut analysis.diagnostics {
        if let Some(path) = diagnostic.path.as_mut() {
            *path = rebase_relative_path(module_root, project_root, path);
        }
    }
}

/// Attaches the exact provider request scope after a module-local result has
/// been rebased to the repository root. Invalid/out-of-root paths are omitted;
/// the direct Language IR adapter treats any resulting scope mismatch as a
/// hard validation error instead of inventing coverage.
pub(crate) fn assign_provider_batch_scope(
    batch: &mut ProviderUnitBatch,
    project_root: &Path,
    files: &[PathBuf],
) {
    batch.source_files = files
        .iter()
        .filter_map(|file| file.strip_prefix(project_root).ok())
        .map(|path| path.to_string_lossy().replace('\\', "/"))
        .filter(|path| !path.is_empty())
        .collect();
    batch.source_files.sort();
    batch.source_files.dedup();
}

fn rebase_symbol_id(symbol: &str, prefixes: &HashMap<String, String>) -> String {
    let Some((old_prefix, suffix)) = symbol.split_once('#') else {
        return symbol.to_string();
    };
    prefixes
        .get(old_prefix)
        .map(|new_prefix| format!("{new_prefix}#{suffix}"))
        .unwrap_or_else(|| symbol.to_string())
}

fn rebase_relative_path(module_root: &Path, project_root: &Path, raw: &str) -> String {
    let path = Path::new(raw);
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        module_root.join(path)
    };
    absolute
        .strip_prefix(project_root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

pub(crate) fn merge_provider_batches(
    analyses: Vec<ProviderUnitBatch>,
) -> (
    Vec<LanguageOutput>,
    Vec<DocumentOutput>,
    Vec<RelationOutput>,
    Vec<Diagnostic>,
) {
    let mut grouped: HashMap<String, Vec<ProviderUnitBatch>> = HashMap::new();
    for analysis in analyses {
        grouped
            .entry(analysis.language.id.clone())
            .or_default()
            .push(analysis);
    }

    let mut languages = Vec::new();
    let mut documents = Vec::new();
    let mut relations = Vec::new();
    let mut diagnostics = Vec::new();
    let mut language_ids: Vec<_> = grouped.keys().cloned().collect();
    language_ids.sort();

    for language_id in language_ids {
        let mut entries = grouped.remove(&language_id).unwrap_or_default();
        entries.sort_by(|left, right| left.language.name.cmp(&right.language.name));
        let first = entries.first().expect("language group is not empty");
        let mut language = LanguageOutput {
            id: language_id,
            name: first.language.name.clone(),
            provider: first.language.provider,
            files_found: 0,
            files_indexed: 0,
            files_excluded: 0,
            files_missing: 0,
            status: "indexed",
        };
        let mut status = first.language.status;
        let mut seen_documents = HashSet::new();
        let mut seen_relations = HashSet::new();
        for entry in entries {
            language.files_found += entry.language.files_found + entry.project_excluded_files;
            language.files_indexed += entry.language.files_indexed;
            language.files_excluded += entry.language.files_excluded + entry.project_excluded_files;
            language.files_missing += entry.language.files_missing;
            status = merge_language_status(status, entry.language.status);
            if entry.project_excluded_files > 0 && status == "indexed" {
                status = "indexed-partial";
            }
            for document in entry.documents {
                if seen_documents.insert(document.path.clone()) {
                    documents.push(document);
                }
            }
            for relation in entry.relations {
                let key = format!(
                    "{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{:?}",
                    relation.from, relation.to, relation.kind, relation.path, relation.range
                );
                if seen_relations.insert(key) {
                    relations.push(relation);
                }
            }
            diagnostics.extend(entry.diagnostics);
        }
        // A provider can return a valid partial index without reporting an
        // error. Coverage is the source of truth for the final status.
        if status == "indexed" && language.files_missing > 0 {
            status = "indexed-partial";
        }
        language.status = status;
        languages.push(language);
    }
    documents.sort_by(|left, right| left.path.cmp(&right.path));
    merge_provider_documents(&mut documents);
    dedupe_provider_relations(&mut relations);
    relations.sort_by(|left, right| {
        (&left.path, &left.range, &left.from, &left.to, &left.kind).cmp(&(
            &right.path,
            &right.range,
            &right.from,
            &right.to,
            &right.kind,
        ))
    });
    (languages, documents, relations, diagnostics)
}

pub(crate) fn dedupe_provider_relations(relations: &mut Vec<RelationOutput>) {
    let mut unique = Vec::with_capacity(relations.len());
    // Preserve the former first-match semantics, but only inspect relations
    // that can possibly be duplicates.  The old implementation scanned every
    // retained relation for every input relation, making a compiler-sized C#
    // index quadratic even though almost all relations have different
    // endpoints.  Group indexes remain in insertion order, so replacement of
    // overlapping CALLS ranges is byte-for-byte equivalent to the old scan.
    let mut indexes_by_identity = HashMap::<(String, String, String, String), Vec<usize>>::new();
    for relation in relations.drain(..) {
        let identity = (
            relation.from.clone(),
            relation.to.clone(),
            relation.kind.clone(),
            relation.path.clone(),
        );
        let duplicate = indexes_by_identity.get(&identity).and_then(|indexes| {
            indexes.iter().copied().find(|index| {
                let existing: &RelationOutput = &unique[*index];
                existing.range == relation.range
                    || (relation.kind == "CALLS"
                        && relation_ranges_overlap(&existing.range, &relation.range))
            })
        });
        if let Some(index) = duplicate {
            if relation.kind == "CALLS"
                && relation_range_size(&relation.range) < relation_range_size(&unique[index].range)
            {
                unique[index] = relation;
            }
        } else {
            indexes_by_identity
                .entry(identity)
                .or_default()
                .push(unique.len());
            unique.push(relation);
        }
    }
    *relations = unique;
}

fn relation_ranges_overlap(left: &[i32], right: &[i32]) -> bool {
    let Some((left_start, left_end)) = relation_range_bounds(left) else {
        return false;
    };
    let Some((right_start, right_end)) = relation_range_bounds(right) else {
        return false;
    };
    left_start <= right_end && right_start <= left_end
}

fn relation_range_bounds(range: &[i32]) -> Option<((i32, i32), (i32, i32))> {
    match range {
        [line, start, end] => Some(((*line, *start), (*line, *end))),
        [start_line, start_column, end_line, end_column] => {
            Some(((*start_line, *start_column), (*end_line, *end_column)))
        }
        _ => None,
    }
}

fn relation_range_size(range: &[i32]) -> i64 {
    let Some((start, end)) = relation_range_bounds(range) else {
        return i64::MAX;
    };
    i64::from(end.0 - start.0) * 1_000_000 + i64::from(end.1 - start.1)
}

pub(crate) fn merge_provider_documents(documents: &mut Vec<DocumentOutput>) {
    let mut merged = Vec::with_capacity(documents.len());
    let mut positions = HashMap::with_capacity(documents.len());
    for document in documents.drain(..) {
        let key = (document.language.clone(), document.path.clone());
        let Some(index) = positions.get(&key).copied() else {
            positions.insert(key, merged.len());
            merged.push(document);
            continue;
        };
        let existing = &mut merged[index];
        for symbol in document.symbols {
            if !existing.symbols.iter().any(|item| {
                item.symbol == symbol.symbol
                    && item.kind == symbol.kind
                    && item.signature == symbol.signature
                    && item.enclosing_symbol == symbol.enclosing_symbol
            }) {
                existing.symbols.push(symbol);
            }
        }
        for occurrence in document.occurrences {
            if !existing.occurrences.iter().any(|item| {
                item.symbol == occurrence.symbol
                    && item.range == occurrence.range
                    && item.definition == occurrence.definition
                    && item.import == occurrence.import
                    && item.read == occurrence.read
                    && item.write == occurrence.write
            }) {
                existing.occurrences.push(occurrence);
            }
        }
    }
    merged.sort_by(|left, right| (&left.language, &left.path).cmp(&(&right.language, &right.path)));
    *documents = merged;
}

pub(crate) fn merge_language_status(left: &'static str, right: &'static str) -> &'static str {
    let rank = |status| match status {
        "indexer-failed" | "invalid-output" | "missing-tool" => 5,
        "indexed-partial" => 4,
        "indexed" => 3,
        "excluded" | "excluded-by-project-config" => 2,
        "empty-semantic" => 1,
        _ => 5,
    };
    if rank(right) > rank(left) {
        right
    } else {
        left
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(range: Vec<i32>) -> RelationOutput {
        RelationOutput {
            from: "handler".to_string(),
            to: "service".to_string(),
            kind: "CALLS".to_string(),
            path: "src/routes.rs".to_string(),
            range,
            confidence: Some(1.0),
            strategy: Some("test".to_string()),
        }
    }

    #[test]
    fn overlapping_call_ranges_keep_one_most_precise_relation() {
        let mut relations = vec![call(vec![4, 4, 4, 25]), call(vec![4, 13, 4, 25])];
        dedupe_provider_relations(&mut relations);
        assert_eq!(relations.len(), 1);
        assert_eq!(relations[0].range, vec![4, 13, 4, 25]);
    }

    #[test]
    fn separate_call_sites_to_the_same_target_remain_distinct() {
        let mut relations = vec![call(vec![4, 4, 4, 12]), call(vec![8, 4, 8, 12])];
        dedupe_provider_relations(&mut relations);
        assert_eq!(relations.len(), 2);
    }

    #[test]
    fn indexed_relation_dedupe_preserves_the_stable_first_match_contract() {
        fn stable_first_match(mut relations: Vec<RelationOutput>) -> Vec<RelationOutput> {
            let mut unique = Vec::with_capacity(relations.len());
            for relation in relations.drain(..) {
                let duplicate = unique.iter().position(|existing: &RelationOutput| {
                    existing.from == relation.from
                        && existing.to == relation.to
                        && existing.kind == relation.kind
                        && existing.path == relation.path
                        && (existing.range == relation.range
                            || (relation.kind == "CALLS"
                                && relation_ranges_overlap(&existing.range, &relation.range)))
                });
                if let Some(index) = duplicate {
                    if relation.kind == "CALLS"
                        && relation_range_size(&relation.range)
                            < relation_range_size(&unique[index].range)
                    {
                        unique[index] = relation;
                    }
                } else {
                    unique.push(relation);
                }
            }
            unique
        }

        let mut inputs = vec![
            call(vec![4, 4, 4, 25]),
            call(vec![8, 4, 8, 12]),
            call(vec![4, 13, 4, 25]),
            call(vec![8, 4, 8, 12]),
        ];
        let mut another_target = call(vec![4, 4, 4, 25]);
        another_target.to = "repository".to_string();
        inputs.insert(1, another_target);
        let expected = serde_json::to_value(stable_first_match(inputs.clone())).unwrap();

        dedupe_provider_relations(&mut inputs);

        assert_eq!(serde_json::to_value(inputs).unwrap(), expected);
    }

    #[test]
    fn shared_header_keeps_distinct_c_and_cpp_semantic_documents() {
        let document = |language: &str| DocumentOutput {
            language: language.to_string(),
            path: "include/types.h".to_string(),
            symbols: Vec::new(),
            occurrences: Vec::new(),
        };
        let mut documents = vec![document("c"), document("cpp")];
        merge_provider_documents(&mut documents);
        assert_eq!(documents.len(), 2);
        assert_eq!(documents[0].language, "c");
        assert_eq!(documents[1].language, "cpp");
    }
}
