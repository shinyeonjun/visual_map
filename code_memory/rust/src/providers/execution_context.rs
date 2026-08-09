#[cfg(test)]
use codebase_fact_model::analysis::ContextDimensionKind;
use codebase_fact_model::analysis::{
    ContextDimension, ProgrammingLanguage, ProviderConfigArtifact, ProviderConfigUse,
    ProviderExecutionContext, ProviderExecutionMode,
};
use codebase_fact_model::identity::Sha256Digest;
use codebase_fact_model::source::RepositoryPath;
#[cfg(test)]
use codebase_fact_model::validation::Validate;
use std::collections::{BTreeSet, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::static_pipeline::context_dimensions::{
    extract_context_dimensions, required_context_dimensions, ContextDimensionInput,
};
use crate::{Diagnostic, LanguageSpec};

const MAX_CONTEXT_FILE_BYTES: u64 = 8 * 1024 * 1024;
const SOURCE_SCOPE_DIGEST_DOMAIN: &[u8] = b"codebase-workspace.provider-source-scope.v1\0";
const GENERATED_CONTEXT_DIGEST_DOMAIN: &[u8] =
    b"codebase-workspace.generated-provider-context.v1\0";

pub(crate) struct ProviderRunOutcome {
    pub(crate) diagnostics: Vec<Diagnostic>,
    pub(crate) execution_context: ProviderExecutionContext,
}

#[derive(Clone, Copy)]
pub(crate) struct ProviderRoots<'a> {
    pub(crate) project: &'a Path,
    pub(crate) analysis: &'a Path,
}

impl<'a> ProviderRoots<'a> {
    pub(crate) const fn new(project: &'a Path, analysis: &'a Path) -> Self {
        Self { project, analysis }
    }
}

pub(crate) struct ExecutedProviderContextInput<'a> {
    pub(crate) project_root: &'a Path,
    pub(crate) language: &'a LanguageSpec,
    pub(crate) mode: ProviderExecutionMode,
    pub(crate) analysis_root: &'a Path,
    pub(crate) source_files: &'a [PathBuf],
    pub(crate) config_files: Vec<(PathBuf, ProviderConfigUse)>,
    pub(crate) generated_context_digest: Option<Sha256Digest>,
    pub(crate) dimensions: Vec<ContextDimension>,
}

pub(crate) fn executed_provider_context(
    input: ExecutedProviderContextInput<'_>,
) -> Result<ProviderExecutionContext, String> {
    let ExecutedProviderContextInput {
        project_root,
        language: lang,
        mode,
        analysis_root,
        source_files,
        config_files,
        generated_context_digest,
        mut dimensions,
    } = input;
    let canonical_analysis_root = analysis_root.canonicalize().map_err(|error| {
        format!(
            "cannot resolve {} provider analysis root {}: {error}",
            lang.name,
            analysis_root.display()
        )
    })?;
    let analysis_root = repository_path(project_root, analysis_root, true)?;
    let mut source_paths = source_files
        .iter()
        .map(|path| {
            let canonical = path.canonicalize().map_err(|error| {
                format!(
                    "cannot resolve {} provider source {}: {error}",
                    lang.name,
                    path.display()
                )
            })?;
            if !canonical.starts_with(&canonical_analysis_root) {
                return Err(format!(
                    "{} provider source is outside its analysis root: {}",
                    lang.name,
                    path.display()
                ));
            }
            repository_path(project_root, path, false)
        })
        .collect::<Result<Vec<_>, _>>()?;
    source_paths.sort();
    source_paths.dedup();
    if source_paths.is_empty() {
        return Err(format!(
            "{} provider execution has an empty source scope",
            lang.name
        ));
    }
    let source_scope_digest = source_scope_digest(&source_paths);
    let mut artifacts = Vec::new();
    for (path, usage) in config_files {
        let repository_path = repository_path(project_root, &path, false)?;
        let metadata = fs::symlink_metadata(&path).map_err(|error| {
            format!(
                "cannot inspect {} provider context file {}: {error}",
                lang.name,
                path.display()
            )
        })?;
        if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
            return Err(format!(
                "{} provider context is not a regular in-repository file: {}",
                lang.name,
                path.display()
            ));
        }
        if metadata.len() > MAX_CONTEXT_FILE_BYTES {
            return Err(format!(
                "{} provider context file exceeds {} bytes: {}",
                lang.name,
                MAX_CONTEXT_FILE_BYTES,
                path.display()
            ));
        }
        let bytes = fs::read(&path).map_err(|error| {
            format!(
                "cannot read {} provider context file {}: {error}",
                lang.name,
                path.display()
            )
        })?;
        artifacts.push(ProviderConfigArtifact {
            path: repository_path,
            content_digest: Sha256Digest::of_bytes(&bytes),
            usage,
        });
    }
    if mode != ProviderExecutionMode::SourceOnlyFallback {
        let config_paths = artifacts
            .iter()
            .map(|artifact| artifact.path.clone())
            .collect::<Vec<_>>();
        dimensions.extend(extract_context_dimensions(ContextDimensionInput {
            language: lang.contract_language,
            project_root,
            config_paths: &config_paths,
            source_paths: &source_paths,
        }));
    }
    dimensions.sort();
    dimensions.dedup();
    let known = dimensions
        .iter()
        .map(|dimension| dimension.kind)
        .collect::<HashSet<_>>();
    let missing_dimensions = required_context_dimensions(lang.contract_language)
        .iter()
        .copied()
        .filter(|kind| !known.contains(kind))
        .collect::<Vec<_>>();
    ProviderExecutionContext::executed(
        mode,
        analysis_root,
        source_scope_digest,
        source_paths.len() as u64,
        artifacts,
        generated_context_digest,
        dimensions,
        missing_dimensions,
    )
    .map_err(|error| format!("invalid {} provider execution context: {error}", lang.name))
}

pub(crate) fn not_executed_provider_context(lang: &LanguageSpec) -> ProviderExecutionContext {
    ProviderExecutionContext::not_executed(
        required_context_dimensions(lang.contract_language).to_vec(),
    )
    .expect("closed provider execution context is valid")
}

pub(crate) fn workspace_context_files(
    language: ProgrammingLanguage,
    project_root: &Path,
    analysis_root: &Path,
    source_files: &[PathBuf],
) -> Vec<PathBuf> {
    let mut directories = BTreeSet::from([analysis_root.to_path_buf()]);
    for ancestor in analysis_root.ancestors() {
        if !ancestor.starts_with(project_root) {
            break;
        }
        directories.insert(ancestor.to_path_buf());
        if ancestor == project_root {
            break;
        }
    }
    for source in source_files {
        for ancestor in source.ancestors() {
            if !ancestor.starts_with(analysis_root) {
                break;
            }
            directories.insert(ancestor.to_path_buf());
            if ancestor == analysis_root {
                break;
            }
        }
    }
    let mut files = BTreeSet::new();
    for directory in directories {
        let Ok(entries) = fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            if !entry.file_type().is_ok_and(|file_type| file_type.is_file()) {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
            if is_context_file(language, &name) {
                files.insert(entry.path());
            }
        }
        if language == ProgrammingLanguage::Dart {
            let package_config = directory.join(".dart_tool").join("package_config.json");
            if package_config.is_file() {
                files.insert(package_config);
            }
        }
        if language == ProgrammingLanguage::Rust {
            for name in ["config.toml", "config"] {
                let config = directory.join(".cargo").join(name);
                if config.is_file() {
                    files.insert(config);
                }
            }
        }
        if language == ProgrammingLanguage::Python {
            for environment in [".venv", "venv"] {
                let config = directory.join(environment).join("pyvenv.cfg");
                if config.is_file() {
                    files.insert(config);
                }
            }
        }
    }
    files.into_iter().collect()
}

pub(crate) fn generated_context_digest(parts: &[Vec<u8>]) -> Sha256Digest {
    let mut bytes = Vec::new();
    append_component(&mut bytes, GENERATED_CONTEXT_DIGEST_DOMAIN);
    for part in parts {
        append_component(&mut bytes, part);
    }
    Sha256Digest::of_bytes(&bytes)
}

pub(crate) fn generated_context_digest_from_files(paths: &[PathBuf]) -> Option<Sha256Digest> {
    let mut paths = paths.to_vec();
    paths.sort();
    let parts = paths
        .iter()
        .map(fs::read)
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    Some(generated_context_digest(&parts))
}

fn repository_path(
    project_root: &Path,
    path: &Path,
    allow_root: bool,
) -> Result<RepositoryPath, String> {
    let canonical_root = project_root.canonicalize().map_err(|error| {
        format!(
            "cannot resolve provider project root {}: {error}",
            project_root.display()
        )
    })?;
    let canonical = path
        .canonicalize()
        .map_err(|error| format!("cannot resolve provider path {}: {error}", path.display()))?;
    let relative = canonical.strip_prefix(&canonical_root).map_err(|_| {
        format!(
            "provider semantic context escaped the selected project root: {}",
            path.display()
        )
    })?;
    if relative.as_os_str().is_empty() {
        if allow_root {
            return Ok(RepositoryPath::root());
        }
        return Err(format!(
            "provider context expected a file but resolved the project root: {}",
            path.display()
        ));
    }
    RepositoryPath::parse(relative.to_string_lossy().replace('\\', "/"))
        .map_err(|error| format!("provider path is not canonical: {error}"))
}

pub(crate) fn source_scope_digest(paths: &[RepositoryPath]) -> Sha256Digest {
    let mut paths = paths.to_vec();
    paths.sort();
    paths.dedup();
    let mut bytes = Vec::new();
    append_component(&mut bytes, SOURCE_SCOPE_DIGEST_DOMAIN);
    for path in paths {
        append_component(&mut bytes, path.as_str().as_bytes());
    }
    Sha256Digest::of_bytes(&bytes)
}

fn append_component(output: &mut Vec<u8>, value: &[u8]) {
    output.extend_from_slice(&(value.len() as u64).to_be_bytes());
    output.extend_from_slice(value);
}

fn is_context_file(language: ProgrammingLanguage, name: &str) -> bool {
    match language {
        ProgrammingLanguage::TypeScript | ProgrammingLanguage::JavaScript => {
            name == "package.json"
                || ((name.starts_with("tsconfig") || name.starts_with("jsconfig"))
                    && name.ends_with(".json"))
        }
        ProgrammingLanguage::Python => {
            matches!(
                name,
                "pyproject.toml" | "pyrightconfig.json" | "setup.py" | "setup.cfg" | "pyvenv.cfg"
            ) || name.starts_with("requirements") && name.ends_with(".txt")
        }
        ProgrammingLanguage::Java => matches!(
            name,
            "pom.xml"
                | "settings.gradle"
                | "settings.gradle.kts"
                | "build.gradle"
                | "build.gradle.kts"
        ),
        ProgrammingLanguage::CSharp => {
            name == "global.json"
                || name.eq_ignore_ascii_case("nuget.config")
                || name.ends_with(".sln")
                || name.ends_with(".slnx")
                || name.ends_with(".csproj")
                || name.ends_with(".props")
                || name.ends_with(".targets")
                || name.ends_with(".ruleset")
        }
        ProgrammingLanguage::C | ProgrammingLanguage::Cpp => matches!(
            name,
            "compile_commands.json" | "compile_flags.txt" | ".clangd"
        ),
        ProgrammingLanguage::Go => matches!(name, "go.mod" | "go.work"),
        ProgrammingLanguage::Rust => {
            matches!(name, "cargo.toml" | "config.toml" | "config")
        }
        ProgrammingLanguage::Dart => {
            matches!(
                name,
                "pubspec.yaml" | "analysis_options.yaml" | "package_config.json"
            )
        }
    }
}

#[cfg(test)]
fn validate_execution_context(context: &ProviderExecutionContext) -> Result<(), String> {
    context
        .validate()
        .map_err(|error| format!("invalid provider execution context: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn not_executed_context_keeps_unknown_dimensions_typed() {
        let language = crate::LANGUAGES
            .iter()
            .find(|language| language.id == "go")
            .unwrap();
        let context = not_executed_provider_context(language);
        assert_eq!(context.mode, ProviderExecutionMode::NotExecuted);
        assert!(context
            .missing_dimensions
            .contains(&ContextDimensionKind::BuildTag));
        validate_execution_context(&context).unwrap();
    }

    #[test]
    fn generated_context_digest_is_order_sensitive_and_stable() {
        let first = generated_context_digest(&[b"a".to_vec(), b"bc".to_vec()]);
        let repeat = generated_context_digest(&[b"a".to_vec(), b"bc".to_vec()]);
        let reversed = generated_context_digest(&[b"bc".to_vec(), b"a".to_vec()]);
        assert_eq!(first, repeat);
        assert_ne!(first, reversed);
    }
}
