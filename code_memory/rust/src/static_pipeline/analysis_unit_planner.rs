use codebase_fact_model::analysis::{
    AnalysisUnit, ProgrammingLanguage, SemanticContext, SemanticContextKind,
};
use codebase_fact_model::analysis_plan::{AnalysisPlan, FileAnalysisAssignment};
use codebase_fact_model::coverage::{AnalysisCapability, AnalysisGap, AnalysisScope, GapCode};
use codebase_fact_model::identity::Sha256Digest;
use codebase_fact_model::source::RepositoryPath;
use codebase_fact_model::source_manifest::{SourceEntryState, SourceManifest};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use super::context_dimensions::{extract_context_dimensions, ContextDimensionInput};

const CONFIG_FINGERPRINT_DOMAIN: &[u8] = b"codebase-workspace.semantic-context.config.v1\0";
const MAX_CONTEXT_FILE_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Clone, Debug)]
struct ConfigArtifact {
    path: RepositoryPath,
    owner_root: RepositoryPath,
    digest: Sha256Digest,
}

/// Builds one deterministic plan for all ten active language contracts. Exact
/// provider project models may refine these boundaries later, but they may not
/// leave a census candidate unowned.
pub(crate) fn plan_analysis_units(
    root: &Path,
    manifest: &SourceManifest,
) -> Result<AnalysisPlan, String> {
    let mut units = Vec::new();
    let mut assignments = Vec::new();
    let mut gaps = Vec::new();

    for language in all_languages() {
        let candidates = manifest
            .files
            .iter()
            .filter(|file| {
                file.state == SourceEntryState::Included && file.languages.contains(&language)
            })
            .map(|file| file.path.clone())
            .collect::<Vec<_>>();
        if candidates.is_empty() {
            continue;
        }
        let artifacts = config_artifacts(root, manifest, language, &candidates)?;
        let roots = analysis_roots(root, language, &candidates, &artifacts)?;
        let collapsed = root_authority(root, language, &artifacts)?;
        let effective_roots = if collapsed {
            vec![RepositoryPath::root()]
        } else {
            roots
        };
        let mut grouped = BTreeMap::<RepositoryPath, Vec<RepositoryPath>>::new();
        for candidate in &candidates {
            let owner = effective_roots
                .iter()
                .filter(|unit_root| path_is_within(candidate, unit_root))
                .max_by_key(|unit_root| path_depth(unit_root))
                .cloned()
                .unwrap_or_else(RepositoryPath::root);
            grouped.entry(owner).or_default().push(candidate.clone());
        }

        for (unit_root, mut files) in grouped {
            files.sort();
            files.dedup();
            let context_artifacts = artifacts
                .iter()
                .filter(|artifact| collapsed || path_is_within(&unit_root, &artifact.owner_root))
                .cloned()
                .collect::<Vec<_>>();
            let fingerprint = context_fingerprint(
                manifest.manifest_digest,
                language,
                &unit_root,
                &context_artifacts,
            );
            let config_paths = context_artifacts
                .iter()
                .map(|artifact| artifact.path.clone())
                .collect::<Vec<_>>();
            let dimensions = extract_context_dimensions(ContextDimensionInput {
                language,
                project_root: root,
                config_paths: &config_paths,
                source_paths: &files,
            });
            let context = SemanticContext::new(
                semantic_context_kind(language),
                fingerprint,
                context_artifacts
                    .iter()
                    .map(|artifact| artifact.path.clone())
                    .collect(),
                dimensions,
            )
            .map_err(|error| format!("cannot create {} context: {error}", language.as_str()))?;
            let unit = AnalysisUnit::new(
                manifest.workspace_id.clone(),
                language,
                unit_root.clone(),
                context,
                files.len() as u64,
            )
            .map_err(|error| {
                format!("cannot create {} analysis unit: {error}", language.as_str())
            })?;
            if !has_authoritative_context(language, &context_artifacts) {
                let (code, message) = if matches!(
                    language,
                    ProgrammingLanguage::C | ProgrammingLanguage::Cpp
                ) {
                    (
                        GapCode::MissingCompileContext,
                        format!(
                            "{} unit {} has no compilation database, compile flags, or .clangd context",
                            language.as_str(),
                            unit_root.as_str()
                        ),
                    )
                } else {
                    (
                        GapCode::MissingProjectMetadata,
                        format!(
                            "{} unit {} uses a deterministic fallback because no authoritative project marker was found",
                            language.as_str(),
                            unit_root.as_str()
                        ),
                    )
                };
                gaps.push(AnalysisGap {
                    code,
                    scope: AnalysisScope::AnalysisUnit {
                        unit_id: unit.id.clone(),
                    },
                    capability: Some(AnalysisCapability::ProjectStructure),
                    evidence_ids: vec![],
                    message,
                });
            }
            for file in files {
                assignments.push(FileAnalysisAssignment {
                    path: file,
                    language,
                    unit_ids: vec![unit.id.clone()],
                });
            }
            units.push(unit);
        }
    }

    let plan = AnalysisPlan::new(
        manifest.workspace_id.clone(),
        manifest.manifest_digest,
        units,
        assignments,
        gaps,
    )
    .map_err(|error| format!("cannot seal analysis plan: {error}"))?;
    plan.validate_against(manifest)
        .map_err(|error| format!("analysis plan does not cover source manifest: {error}"))?;
    Ok(plan)
}

fn all_languages() -> [ProgrammingLanguage; 10] {
    [
        ProgrammingLanguage::TypeScript,
        ProgrammingLanguage::JavaScript,
        ProgrammingLanguage::Python,
        ProgrammingLanguage::Java,
        ProgrammingLanguage::CSharp,
        ProgrammingLanguage::C,
        ProgrammingLanguage::Cpp,
        ProgrammingLanguage::Go,
        ProgrammingLanguage::Rust,
        ProgrammingLanguage::Dart,
    ]
}

fn config_artifacts(
    root: &Path,
    manifest: &SourceManifest,
    language: ProgrammingLanguage,
    candidates: &[RepositoryPath],
) -> Result<Vec<ConfigArtifact>, String> {
    let mut artifacts = manifest
        .files
        .iter()
        .filter(|file| file.state == SourceEntryState::Included)
        .filter_map(|file| {
            let digest = file.content_digest?;
            let owner_root = marker_owner(language, &file.path)?;
            Some(ConfigArtifact {
                path: file.path.clone(),
                owner_root,
                digest,
            })
        })
        .collect::<Vec<_>>();
    artifacts.extend(exact_hidden_contexts(root, language, candidates)?);
    artifacts.sort_by(|left, right| left.path.cmp(&right.path));
    artifacts.dedup_by(|left, right| left.path == right.path);
    Ok(artifacts)
}

fn analysis_roots(
    _root: &Path,
    language: ProgrammingLanguage,
    candidates: &[RepositoryPath],
    artifacts: &[ConfigArtifact],
) -> Result<Vec<RepositoryPath>, String> {
    let mut roots = BTreeSet::from([RepositoryPath::root()]);
    roots.extend(
        artifacts
            .iter()
            .filter(|artifact| defines_analysis_unit_root(language, &artifact.path))
            .map(|artifact| artifact.owner_root.clone()),
    );
    roots.retain(|unit_root| {
        candidates
            .iter()
            .any(|file| path_is_within(file, unit_root))
    });
    Ok(roots.into_iter().collect())
}

fn defines_analysis_unit_root(language: ProgrammingLanguage, path: &RepositoryPath) -> bool {
    let Some(name) = file_name(path) else {
        return false;
    };
    match language {
        ProgrammingLanguage::TypeScript | ProgrammingLanguage::JavaScript => {
            name == "package.json"
                || ((name.starts_with("tsconfig") || name.starts_with("jsconfig"))
                    && name.ends_with(".json"))
        }
        ProgrammingLanguage::Python => matches!(
            name.as_str(),
            "pyproject.toml" | "pyrightconfig.json" | "setup.py" | "setup.cfg" | "pyvenv.cfg"
        ),
        ProgrammingLanguage::Java => matches!(
            name.as_str(),
            "pom.xml"
                | "settings.gradle"
                | "settings.gradle.kts"
                | "build.gradle"
                | "build.gradle.kts"
        ),
        ProgrammingLanguage::CSharp => {
            name.ends_with(".sln") || name.ends_with(".slnx") || name.ends_with(".csproj")
        }
        ProgrammingLanguage::C | ProgrammingLanguage::Cpp => matches!(
            name.as_str(),
            "compile_commands.json" | "compile_flags.txt" | ".clangd"
        ),
        ProgrammingLanguage::Go => matches!(name.as_str(), "go.work" | "go.mod"),
        ProgrammingLanguage::Rust => name == "cargo.toml",
        // Analyzer options can enable language experiments and change semantic
        // resolution for every Dart file below their directory. Treat a
        // nested options file as a real execution boundary so the plan owns
        // exactly the configuration that the Dart analysis server will read.
        ProgrammingLanguage::Dart => matches!(
            name.as_str(),
            "pubspec.yaml" | "analysis_options.yaml" | "package_config.json"
        ),
    }
}

fn root_authority(
    root: &Path,
    language: ProgrammingLanguage,
    artifacts: &[ConfigArtifact],
) -> Result<bool, String> {
    let has_root = |predicate: fn(&str) -> bool| {
        artifacts.iter().any(|artifact| {
            artifact.owner_root.is_root()
                && file_name(&artifact.path).is_some_and(|name| predicate(&name))
        })
    };
    match language {
        ProgrammingLanguage::Java => {
            if has_root(|name| matches!(name, "settings.gradle" | "settings.gradle.kts")) {
                return Ok(true);
            }
            let pom = root.join("pom.xml");
            Ok(has_root(|name| name == "pom.xml")
                && bounded_text(&pom)?
                    .is_some_and(|text| text.contains("<modules>") && text.contains("<module>")))
        }
        ProgrammingLanguage::CSharp => Ok(has_root(|name| {
            name.ends_with(".sln") || name.ends_with(".slnx")
        })),
        ProgrammingLanguage::Go => Ok(has_root(|name| name == "go.work")),
        ProgrammingLanguage::Rust => {
            let cargo = root.join("Cargo.toml");
            Ok(has_root(|name| name == "cargo.toml")
                && bounded_text(&cargo)?.is_some_and(|text| text.contains("[workspace]")))
        }
        _ => Ok(false),
    }
}

fn marker_owner(language: ProgrammingLanguage, path: &RepositoryPath) -> Option<RepositoryPath> {
    let name = file_name(path)?;
    let name = name.as_str();
    let matches = match language {
        ProgrammingLanguage::TypeScript | ProgrammingLanguage::JavaScript => {
            (name.starts_with("tsconfig") || name.starts_with("jsconfig"))
                && name.ends_with(".json")
                || name == "package.json"
        }
        ProgrammingLanguage::Python => {
            matches!(
                name,
                "pyproject.toml"
                    | "pyrightconfig.json"
                    | "setup.py"
                    | "setup.cfg"
                    | "requirements.txt"
                    | "pipfile"
                    | "poetry.lock"
                    | "pyvenv.cfg"
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
        ProgrammingLanguage::C | ProgrammingLanguage::Cpp => {
            matches!(
                name,
                "compile_commands.json"
                    | "compile_flags.txt"
                    | ".clangd"
                    | "cmakelists.txt"
                    | "meson.build"
            ) || name.ends_with(".vcxproj")
        }
        ProgrammingLanguage::Go => matches!(name, "go.work" | "go.mod"),
        ProgrammingLanguage::Rust => name == "cargo.toml",
        ProgrammingLanguage::Dart => {
            matches!(
                name,
                "pubspec.yaml" | "analysis_options.yaml" | "package_config.json"
            )
        }
    };
    matches.then(|| parent_path(path))
}

fn exact_hidden_contexts(
    root: &Path,
    language: ProgrammingLanguage,
    candidates: &[RepositoryPath],
) -> Result<Vec<ConfigArtifact>, String> {
    if !matches!(
        language,
        ProgrammingLanguage::C
            | ProgrammingLanguage::Cpp
            | ProgrammingLanguage::Dart
            | ProgrammingLanguage::Python
            | ProgrammingLanguage::Rust
    ) {
        return Ok(Vec::new());
    }
    let mut owners = BTreeSet::new();
    for candidate in candidates {
        owners.extend(parent_ancestors(candidate));
    }
    let mut artifacts = Vec::new();
    for owner_root in owners {
        let names: &[&str] = match language {
            ProgrammingLanguage::C | ProgrammingLanguage::Cpp => &[
                "compile_commands.json",
                "compile_flags.txt",
                ".clangd",
                "build/compile_commands.json",
                "out/compile_commands.json",
            ],
            ProgrammingLanguage::Dart => &[".dart_tool/package_config.json"],
            ProgrammingLanguage::Python => &["pyvenv.cfg", ".venv/pyvenv.cfg", "venv/pyvenv.cfg"],
            ProgrammingLanguage::Rust => &[".cargo/config.toml", ".cargo/config"],
            _ => &[],
        };
        for name in names {
            let relative = join_repository_path(&owner_root, name)?;
            let absolute = root.join(repository_path_to_native(&relative));
            let Some(digest) = digest_regular_context_file(root, &absolute)? else {
                continue;
            };
            artifacts.push(ConfigArtifact {
                path: relative,
                owner_root: owner_root.clone(),
                digest,
            });
        }
    }
    Ok(artifacts)
}

fn digest_regular_context_file(root: &Path, path: &Path) -> Result<Option<Sha256Digest>, String> {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return Ok(None);
    };
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Ok(None);
    }
    if metadata.len() > MAX_CONTEXT_FILE_BYTES {
        return Err(format!(
            "semantic context file exceeds {} bytes: {}",
            MAX_CONTEXT_FILE_BYTES,
            path.display()
        ));
    }
    let canonical = path
        .canonicalize()
        .map_err(|error| format!("cannot resolve context file {}: {error}", path.display()))?;
    let canonical_root = root
        .canonicalize()
        .map_err(|error| format!("cannot resolve project root {}: {error}", root.display()))?;
    if !canonical.starts_with(canonical_root) {
        return Err(format!(
            "semantic context file escaped project root: {}",
            path.display()
        ));
    }
    let bytes = fs::read(path)
        .map_err(|error| format!("cannot read context file {}: {error}", path.display()))?;
    Ok(Some(Sha256Digest::of_bytes(&bytes)))
}

fn has_authoritative_context(language: ProgrammingLanguage, artifacts: &[ConfigArtifact]) -> bool {
    artifacts.iter().any(|artifact| {
        let Some(name) = file_name(&artifact.path) else {
            return false;
        };
        let name = name.as_str();
        match language {
            ProgrammingLanguage::TypeScript | ProgrammingLanguage::JavaScript => {
                (name.starts_with("tsconfig") || name.starts_with("jsconfig"))
                    && name.ends_with(".json")
            }
            ProgrammingLanguage::Python => matches!(
                name,
                "pyproject.toml" | "pyrightconfig.json" | "setup.py" | "setup.cfg" | "pyvenv.cfg"
            ),
            ProgrammingLanguage::Java => matches!(
                name,
                "pom.xml"
                    | "settings.gradle"
                    | "settings.gradle.kts"
                    | "build.gradle"
                    | "build.gradle.kts"
            ),
            ProgrammingLanguage::CSharp => {
                name.ends_with(".sln") || name.ends_with(".slnx") || name.ends_with(".csproj")
            }
            ProgrammingLanguage::C | ProgrammingLanguage::Cpp => matches!(
                name,
                "compile_commands.json" | "compile_flags.txt" | ".clangd"
            ),
            ProgrammingLanguage::Go => matches!(name, "go.work" | "go.mod"),
            ProgrammingLanguage::Rust => name == "cargo.toml",
            ProgrammingLanguage::Dart => {
                matches!(name, "pubspec.yaml" | "package_config.json")
            }
        }
    })
}

fn semantic_context_kind(language: ProgrammingLanguage) -> SemanticContextKind {
    match language {
        ProgrammingLanguage::TypeScript
        | ProgrammingLanguage::JavaScript
        | ProgrammingLanguage::Java
        | ProgrammingLanguage::CSharp
        | ProgrammingLanguage::C
        | ProgrammingLanguage::Cpp => SemanticContextKind::CompilerProject,
        ProgrammingLanguage::Python
        | ProgrammingLanguage::Go
        | ProgrammingLanguage::Rust
        | ProgrammingLanguage::Dart => SemanticContextKind::Package,
    }
}

fn context_fingerprint(
    manifest_digest: Sha256Digest,
    language: ProgrammingLanguage,
    root: &RepositoryPath,
    artifacts: &[ConfigArtifact],
) -> Sha256Digest {
    let mut bytes = Vec::new();
    append_component(&mut bytes, CONFIG_FINGERPRINT_DOMAIN);
    append_component(&mut bytes, language.as_str().as_bytes());
    append_component(&mut bytes, root.as_str().as_bytes());
    if artifacts.is_empty() {
        append_component(&mut bytes, b"fallback");
        append_component(&mut bytes, manifest_digest.to_hex().as_bytes());
    } else {
        for artifact in artifacts {
            append_component(&mut bytes, artifact.path.as_str().as_bytes());
            append_component(&mut bytes, artifact.digest.to_hex().as_bytes());
        }
    }
    Sha256Digest::of_bytes(&bytes)
}

fn append_component(output: &mut Vec<u8>, value: &[u8]) {
    output.extend_from_slice(&(value.len() as u64).to_be_bytes());
    output.extend_from_slice(value);
}

fn file_name(path: &RepositoryPath) -> Option<String> {
    path.as_str()
        .rsplit('/')
        .next()
        .map(str::to_ascii_lowercase)
}

fn parent_path(path: &RepositoryPath) -> RepositoryPath {
    path.as_str()
        .rsplit_once('/')
        .and_then(|(parent, _)| RepositoryPath::parse(parent).ok())
        .unwrap_or_else(RepositoryPath::root)
}

fn parent_ancestors(path: &RepositoryPath) -> Vec<RepositoryPath> {
    let parent = parent_path(path);
    if parent.is_root() {
        return vec![parent];
    }
    let segments = parent.as_str().split('/').collect::<Vec<_>>();
    let mut output = vec![RepositoryPath::root()];
    for end in 1..=segments.len() {
        if let Ok(path) = RepositoryPath::parse(segments[..end].join("/")) {
            output.push(path);
        }
    }
    output
}

fn join_repository_path(root: &RepositoryPath, suffix: &str) -> Result<RepositoryPath, String> {
    let value = if root.is_root() {
        suffix.to_string()
    } else {
        format!("{}/{}", root.as_str(), suffix)
    };
    RepositoryPath::parse(value).map_err(|error| format!("invalid context path: {error}"))
}

fn repository_path_to_native(path: &RepositoryPath) -> PathBuf {
    path.as_str().split('/').collect()
}

fn path_depth(path: &RepositoryPath) -> usize {
    if path.is_root() {
        0
    } else {
        path.as_str().matches('/').count() + 1
    }
}

fn path_is_within(path: &RepositoryPath, root: &RepositoryPath) -> bool {
    if root.is_root() {
        return true;
    }
    path.as_str() == root.as_str()
        || path
            .as_str()
            .strip_prefix(root.as_str())
            .is_some_and(|suffix| suffix.starts_with('/'))
}

fn bounded_text(path: &Path) -> Result<Option<String>, String> {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return Ok(None);
    };
    if !metadata.file_type().is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() > MAX_CONTEXT_FILE_BYTES
    {
        return Ok(None);
    }
    fs::read_to_string(path)
        .map(Some)
        .map_err(|error| format!("cannot read project marker {}: {error}", path.display()))
}
