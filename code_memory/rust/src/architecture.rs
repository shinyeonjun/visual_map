use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::{DiagnosticCode, DocumentOutput, IndexOutput, SourceSnapshot};
mod builder;
mod helpers;
mod model;
pub(crate) use builder::*;
pub(crate) use helpers::*;
pub(crate) use model::*;

#[cfg(test)]
mod tests;

#[cfg(test)]
pub(crate) fn build(root: &Path, output: &IndexOutput) -> ArchitectureOutput {
    let mut snapshot = crate::load_source_snapshot(root);
    build_with_sources(root, output, &mut snapshot)
}

pub(crate) fn build_with_sources(
    root: &Path,
    output: &IndexOutput,
    snapshot: &mut SourceSnapshot,
) -> ArchitectureOutput {
    let mut source_texts = HashMap::new();
    let mut files = Vec::new();
    // The framework stage has already consumed the snapshot by borrow. Move
    // the source strings here instead of cloning every large file into a
    // second map before architecture analysis.
    for (path, source) in std::mem::take(&mut snapshot.files) {
        let Some(language) = language_for_path(&path) else {
            continue;
        };
        source_texts.insert(path.clone(), source);
        files.push((path.clone(), language.to_string()));
    }
    files.sort();
    let semantic_paths: HashSet<String> = output
        .documents
        .iter()
        .filter(|document| !document.symbols.is_empty() || !document.occurrences.is_empty())
        .map(|document| document.path.clone())
        .collect();
    let packages = load_packages(root);
    let mut builder = ArchitectureBuilder::new(root, source_texts, packages);
    for diagnostic in &output.diagnostics {
        builder.diagnostics.push(ArchitectureDiagnostic {
            kind: "provider".to_string(),
            code: diagnostic.code,
            path: diagnostic.path.clone(),
            detail: diagnostic.detail.clone(),
            message: match diagnostic.line {
                Some(line) => format!("{}:{}: {}", diagnostic.language, line, diagnostic.message),
                None => format!("{}: {}", diagnostic.language, diagnostic.message),
            },
            exclusion_reason: diagnostic.code.exclusion_reason().map(str::to_string),
            exclusion_scope: Some(if diagnostic.path.is_some() {
                "file".to_string()
            } else {
                "language".to_string()
            }),
        });
    }
    for coverage in &output.coverage {
        if coverage.status != "indexed" {
            builder.diagnostics.push(ArchitectureDiagnostic {
                kind: coverage.status.to_string(),
                code: DiagnosticCode::PartialCoverage,
                path: Some(coverage.path.clone()),
                detail: None,
                message: coverage
                    .reason
                    .as_deref()
                    .unwrap_or("not-indexed")
                    .to_string(),
                exclusion_reason: coverage
                    .reason
                    .as_deref()
                    .and_then(coverage_exclusion_reason),
                exclusion_scope: Some("file".to_string()),
            });
        }
    }
    builder.build_file_tree(&files, &semantic_paths);
    builder.build_symbol_index(&output.documents);
    builder.build_imports(&files);
    builder.emit_import_edges();
    builder.emit_project_import_edges(output);
    builder.emit_source_boundaries();
    builder.emit_call_boundaries(output);
    builder.emit_framework_boundaries(output);
    let mut architecture = builder.finish();
    architecture.provider_provenance = output.provider_provenance.clone();
    architecture.languages = output
        .languages
        .iter()
        .map(|language| ArchitectureLanguageSummary {
            id: language.id.clone(),
            name: language.name.clone(),
            provider: language.provider.to_string(),
            files_found: language.files_found,
            files_indexed: language.files_indexed,
            files_excluded: language.files_excluded,
            files_missing: language.files_missing,
            status: language.status.to_string(),
            exclusion_reason: language_exclusion_reason(
                &language.id,
                language.status,
                &output.diagnostics,
            ),
            exclusion_scope: (language.status == "excluded").then(|| "language".to_string()),
        })
        .collect();
    architecture.frameworks = output
        .frameworks
        .iter()
        .map(|framework| ArchitectureFrameworkSummary {
            id: framework.id.clone(),
            language: framework.language.clone(),
            name: framework.name.clone(),
            adapter: framework.adapter.clone(),
            status: framework.status.clone(),
            fact_count: framework.facts.len(),
            relation_count: output
                .framework_relations
                .iter()
                .filter(|relation| relation.framework == framework.id)
                .count(),
        })
        .collect();
    architecture
}

fn language_exclusion_reason(
    language: &str,
    status: &str,
    diagnostics: &[crate::Diagnostic],
) -> Option<String> {
    if status != "excluded" {
        return None;
    }
    diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.language == language)
        .find_map(|diagnostic| diagnostic.code.exclusion_reason().map(str::to_string))
}

fn coverage_exclusion_reason(reason: &str) -> Option<String> {
    match reason {
        "no-compile-context" => Some("missing-compile-context".to_string()),
        "provider-excluded" => Some("missing-dependency".to_string()),
        _ => None,
    }
}
