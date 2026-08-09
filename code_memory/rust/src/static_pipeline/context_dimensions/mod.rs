//! Exact, non-secret context dimensions that can change project-local static
//! semantics. The module deliberately ignores presentation-only metadata and
//! values already pinned by the provider artifact itself.

mod csharp;
mod dart;
mod go;
mod java;
mod native;
mod python;
mod rust;
mod typescript;

pub(crate) use go::execution_environment as go_execution_environment;

use codebase_fact_model::analysis::{ContextDimension, ContextDimensionKind, ProgrammingLanguage};
use codebase_fact_model::source::RepositoryPath;
use std::fs;
use std::path::{Path, PathBuf};

const MAX_CONTEXT_FILE_BYTES: u64 = 8 * 1024 * 1024;

/// Inputs are repository-relative and already owned by one Analysis Unit.
/// Keeping source scope beside config scope is required for dimensions such as
/// Java source sets and per-translation-unit C/C++ flags.
pub(crate) struct ContextDimensionInput<'a> {
    pub(crate) language: ProgrammingLanguage,
    pub(crate) project_root: &'a Path,
    pub(crate) config_paths: &'a [RepositoryPath],
    pub(crate) source_paths: &'a [RepositoryPath],
}

pub(crate) fn extract_context_dimensions(
    input: ContextDimensionInput<'_>,
) -> Vec<ContextDimension> {
    let mut dimensions = Vec::new();
    for repository_path in input.config_paths {
        let path = native_path(input.project_root, repository_path);
        let Ok(metadata) = fs::metadata(&path) else {
            continue;
        };
        if !metadata.is_file() || metadata.len() > MAX_CONTEXT_FILE_BYTES {
            continue;
        }
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        let name = file_name(repository_path);
        match input.language {
            ProgrammingLanguage::TypeScript | ProgrammingLanguage::JavaScript => {
                typescript::collect(&name, &text, &mut dimensions)
            }
            ProgrammingLanguage::Python => python::collect(&name, &text, &mut dimensions),
            ProgrammingLanguage::Java => java::collect_config(&name, &text, &mut dimensions),
            ProgrammingLanguage::CSharp => csharp::collect(&name, &text, &mut dimensions),
            ProgrammingLanguage::C | ProgrammingLanguage::Cpp => native::collect(
                input.language,
                input.project_root,
                repository_path,
                &text,
                input.source_paths,
                &mut dimensions,
            ),
            ProgrammingLanguage::Go => go::collect_config(&name, &text, &mut dimensions),
            ProgrammingLanguage::Rust => {
                rust::collect_config(repository_path, &text, &mut dimensions)
            }
            ProgrammingLanguage::Dart => dart::collect(&name, &text, &mut dimensions),
        }
    }

    match input.language {
        ProgrammingLanguage::Java => java::collect_source_sets(input.source_paths, &mut dimensions),
        ProgrammingLanguage::Go => go::collect_execution_environment(&mut dimensions),
        ProgrammingLanguage::Rust => rust::complete_defaults(&mut dimensions),
        _ => {}
    }
    dimensions.sort();
    dimensions.dedup();
    dimensions
}

/// Only axes that can change source inclusion or project-local endpoint
/// resolution are release blocking. Provider/tool versions are already pinned
/// by signed artifact provenance and are not duplicated here.
pub(crate) fn required_context_dimensions(
    language: ProgrammingLanguage,
) -> &'static [ContextDimensionKind] {
    use ContextDimensionKind as Kind;
    match language {
        ProgrammingLanguage::TypeScript | ProgrammingLanguage::JavaScript => {
            &[Kind::ModuleMode, Kind::Target]
        }
        ProgrammingLanguage::Python => &[Kind::LanguageVersion, Kind::Platform],
        ProgrammingLanguage::Java => &[Kind::LanguageVersion, Kind::SourceSet],
        ProgrammingLanguage::CSharp => &[Kind::TargetFramework, Kind::Profile, Kind::Platform],
        ProgrammingLanguage::C | ProgrammingLanguage::Cpp => &[Kind::LanguageVersion, Kind::Target],
        ProgrammingLanguage::Go => &[
            Kind::LanguageVersion,
            Kind::BuildTag,
            Kind::Platform,
            Kind::Architecture,
        ],
        ProgrammingLanguage::Rust => &[Kind::LanguageVersion, Kind::Target, Kind::Feature],
        ProgrammingLanguage::Dart => &[Kind::LanguageVersion],
    }
}

pub(crate) fn context_dimension(
    kind: ContextDimensionKind,
    value: impl Into<String>,
) -> Option<ContextDimension> {
    let value = value.into().trim().to_ascii_lowercase();
    (!value.is_empty()).then_some(ContextDimension { kind, value })
}

pub(crate) fn push_dimension(
    output: &mut Vec<ContextDimension>,
    kind: ContextDimensionKind,
    value: impl Into<String>,
) {
    if let Some(value) = context_dimension(kind, value) {
        output.push(value);
    }
}

pub(crate) fn literal_assignment_in_section<'a>(
    text: &'a str,
    section: &str,
    key: &str,
) -> Option<&'a str> {
    let mut active_section = "";
    for raw_line in text.lines() {
        let line = raw_line.split('#').next()?.trim();
        if line.starts_with('[') && line.ends_with(']') {
            active_section = line.trim_matches(['[', ']']).trim();
            continue;
        }
        if active_section != section {
            continue;
        }
        let (candidate, value) = line.split_once('=')?;
        if candidate.trim() != key {
            continue;
        }
        let value = unquote(value);
        if !value.is_empty() && !value.contains("$(") {
            return Some(value);
        }
    }
    None
}

pub(crate) fn unquote(value: &str) -> &str {
    value.trim().trim_matches(['\'', '"'])
}

pub(crate) fn xml_tag_values<'a>(text: &'a str, tag: &str) -> Vec<&'a str> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let mut remaining = text;
    let mut values = Vec::new();
    while let Some((_, after_open)) = remaining.split_once(&open) {
        let Some((value, after_close)) = after_open.split_once(&close) else {
            break;
        };
        values.push(value.trim());
        remaining = after_close;
    }
    values
}

pub(crate) fn native_path(root: &Path, path: &RepositoryPath) -> PathBuf {
    path.as_str()
        .split('/')
        .fold(root.to_path_buf(), |path, component| path.join(component))
}

fn file_name(path: &RepositoryPath) -> String {
    path.as_str()
        .rsplit('/')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn required_dimensions_exclude_non_visualization_axes() {
        assert_eq!(
            required_context_dimensions(ProgrammingLanguage::Go),
            &[
                ContextDimensionKind::LanguageVersion,
                ContextDimensionKind::BuildTag,
                ContextDimensionKind::Platform,
                ContextDimensionKind::Architecture,
            ]
        );
        assert!(!required_context_dimensions(ProgrammingLanguage::Rust)
            .contains(&ContextDimensionKind::Profile));
        assert!(!required_context_dimensions(ProgrammingLanguage::Cpp)
            .contains(&ContextDimensionKind::Architecture));
    }

    #[test]
    fn context_dimension_rejects_blank_and_canonicalizes_case() {
        assert!(context_dimension(ContextDimensionKind::Target, "  ").is_none());
        assert_eq!(
            context_dimension(ContextDimensionKind::Target, " ES2022 ")
                .unwrap()
                .value,
            "es2022"
        );
    }
}
