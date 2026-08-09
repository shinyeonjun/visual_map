use std::collections::BTreeSet;

use codebase_fact_model::analysis::ProgrammingLanguage;
use codebase_fact_model::coverage::GapCode;
use codebase_fact_model::fact_graph::ResolutionMethod;
use codebase_fact_model::language_ir::IrEndpoint;
use codebase_fact_model::source::RepositoryPath;

use super::project_index::{
    join_repository_path, parent_path, ProjectImportIndex, StructureTarget,
};
use super::{ImportForm, ImportSite};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::static_pipeline::language_ir) enum ImportResolution {
    Internal {
        target: IrEndpoint,
        method: ResolutionMethod,
    },
    KnownExternal,
    Unresolved {
        gap: GapCode,
    },
    Ambiguous {
        candidate_count: u64,
    },
}

impl ProjectImportIndex {
    pub(in crate::static_pipeline::language_ir) fn resolve(
        &self,
        source_path: &RepositoryPath,
        site: &ImportSite,
    ) -> ImportResolution {
        match site.language {
            ProgrammingLanguage::TypeScript | ProgrammingLanguage::JavaScript => {
                self.resolve_ecmascript(source_path, site)
            }
            ProgrammingLanguage::Python => self.resolve_python(source_path, site),
            ProgrammingLanguage::Java => self.resolve_java(site),
            ProgrammingLanguage::CSharp => self.resolve_csharp(site),
            ProgrammingLanguage::C | ProgrammingLanguage::Cpp => {
                self.resolve_c_family(source_path, site)
            }
            ProgrammingLanguage::Go => self.resolve_go(site),
            ProgrammingLanguage::Rust => self.resolve_rust(source_path, site),
            ProgrammingLanguage::Dart => self.resolve_dart(source_path, site),
        }
    }

    fn resolve_ecmascript(
        &self,
        source_path: &RepositoryPath,
        site: &ImportSite,
    ) -> ImportResolution {
        let key = (site.language, source_path.clone(), site.specifier.clone());
        if let Some(targets) = self.project_model_targets.get(&key) {
            return select_files(source_path, targets, ResolutionMethod::ProjectModel);
        }
        if self.project_model_files.contains(source_path) {
            return if is_relative_specifier(&site.specifier)
                || site.specifier.starts_with(['/', '#'])
            {
                unresolved(GapCode::UnresolvedTarget)
            } else {
                // The compiler project model visited this source and emitted
                // every project-local target. A remaining bare specifier is a
                // measured external boundary, not a guessed internal edge.
                ImportResolution::KnownExternal
            };
        }
        if is_relative_specifier(&site.specifier) {
            let candidates = self
                .resolution_files
                .get(&site.language)
                .unwrap_or(&self.included_files);
            let targets = resolve_relative_candidates(
                source_path,
                &site.specifier,
                candidates,
                &[
                    ".ts", ".tsx", ".mts", ".cts", ".js", ".jsx", ".mjs", ".cjs", ".d.ts",
                ],
                true,
            );
            return select_files(source_path, &targets, ResolutionMethod::SyntaxExact);
        }
        if site.specifier.starts_with("node:") {
            ImportResolution::KnownExternal
        } else {
            unresolved(GapCode::MissingDependencyMetadata)
        }
    }

    fn resolve_python(&self, source_path: &RepositoryPath, site: &ImportSite) -> ImportResolution {
        let module_names = if site.specifier.starts_with('.') {
            python_relative_module_names(
                source_path,
                self.python_source_modules.get(source_path),
                &site.specifier,
            )
        } else {
            vec![site.specifier.clone()]
        };
        if module_names.is_empty() {
            return unresolved(GapCode::UnresolvedTarget);
        }
        let mut files = Vec::new();
        for module in &module_names {
            files.extend(
                self.python_modules
                    .get(module)
                    .into_iter()
                    .flatten()
                    .cloned(),
            );
        }
        canonicalize_paths(&mut files);
        if !files.is_empty() {
            return select_files(source_path, &files, ResolutionMethod::SyntaxExact);
        }
        let mut structures = Vec::new();
        for module in &module_names {
            structures.extend(
                self.python_packages
                    .get(module)
                    .into_iter()
                    .flatten()
                    .cloned(),
            );
        }
        if !structures.is_empty() {
            select_structures(&structures, ResolutionMethod::SyntaxExact)
        } else if site.specifier.starts_with('.') {
            unresolved(GapCode::UnresolvedTarget)
        } else {
            unresolved(GapCode::MissingDependencyMetadata)
        }
    }

    fn resolve_java(&self, site: &ImportSite) -> ImportResolution {
        let ImportForm::Java {
            static_import,
            wildcard,
        } = site.form
        else {
            return unresolved(GapCode::UnresolvedTarget);
        };
        if wildcard && !static_import {
            if let Some(packages) = self.java_packages.get(&site.specifier) {
                return select_structures(packages, ResolutionMethod::SyntaxExact);
            }
        } else if let Some(paths) = longest_qualified_file_match(&self.java_types, &site.specifier)
        {
            return select_files_without_source(paths, ResolutionMethod::SyntaxExact);
        }
        if site.specifier == "java" || site.specifier.starts_with("java.") {
            ImportResolution::KnownExternal
        } else {
            unresolved(GapCode::MissingDependencyMetadata)
        }
    }

    fn resolve_csharp(&self, site: &ImportSite) -> ImportResolution {
        let ImportForm::CSharp {
            static_import,
            alias,
        } = site.form
        else {
            return unresolved(GapCode::UnresolvedTarget);
        };
        if static_import || alias {
            if let Some(paths) = longest_qualified_file_match(&self.csharp_types, &site.specifier) {
                return select_files_without_source(paths, ResolutionMethod::SyntaxExact);
            }
        }
        if let Some(namespaces) = self.csharp_namespaces.get(&site.specifier) {
            return select_structures(namespaces, ResolutionMethod::SyntaxExact);
        }
        if site.specifier == "System" || site.specifier.starts_with("System.") {
            ImportResolution::KnownExternal
        } else {
            unresolved(GapCode::MissingDependencyMetadata)
        }
    }

    fn resolve_c_family(
        &self,
        source_path: &RepositoryPath,
        site: &ImportSite,
    ) -> ImportResolution {
        let ImportForm::CInclude { system, literal } = site.form else {
            return unresolved(GapCode::UnresolvedTarget);
        };
        if !literal {
            return unresolved(GapCode::MissingCompileContext);
        }
        if !system {
            let candidates = self
                .resolution_files
                .get(&site.language)
                .unwrap_or(&self.included_files);
            let targets =
                resolve_relative_candidates(source_path, &site.specifier, candidates, &[], false);
            if !targets.is_empty() {
                return select_files(source_path, &targets, ResolutionMethod::SyntaxExact);
            }
        }
        // Angle includes and quote includes found only through -I/-iquote need
        // the exact compilation command. Never turn a unique suffix into truth.
        unresolved(GapCode::MissingCompileContext)
    }

    fn resolve_go(&self, site: &ImportSite) -> ImportResolution {
        if let Some(packages) = self.go_packages.get(&site.specifier) {
            return select_structures(packages, ResolutionMethod::ProjectModel);
        }
        let first = site.specifier.split('/').next().unwrap_or_default();
        if !first.is_empty() && !first.contains('.') {
            ImportResolution::KnownExternal
        } else {
            unresolved(GapCode::MissingDependencyMetadata)
        }
    }

    fn resolve_rust(&self, source_path: &RepositoryPath, site: &ImportSite) -> ImportResolution {
        match site.form {
            ImportForm::RustExternCrate => {
                if matches!(site.specifier.as_str(), "std" | "core" | "alloc") {
                    return ImportResolution::KnownExternal;
                }
                if let Some(files) = self.rust_crates.get(&site.specifier.replace('-', "_")) {
                    return select_files(source_path, files, ResolutionMethod::ProjectModel);
                }
                unresolved(GapCode::MissingDependencyMetadata)
            }
            ImportForm::RustUse => {
                if starts_with_rust_standard_crate(&site.specifier) {
                    return ImportResolution::KnownExternal;
                }
                let Some(unit_id) = self.owner(ProgrammingLanguage::Rust, source_path) else {
                    return unresolved(GapCode::UnresolvedTarget);
                };
                let source_modules = self
                    .rust_source_modules
                    .get(source_path)
                    .cloned()
                    .unwrap_or_default();
                let candidates = rust_import_candidates(&site.specifier, &source_modules);
                for candidate in candidates {
                    if let Some(paths) =
                        longest_rust_module_match(&self.rust_modules, unit_id, &candidate)
                    {
                        return select_files(source_path, paths, ResolutionMethod::SyntaxExact);
                    }
                    let first = candidate
                        .strip_prefix("crate::")
                        .unwrap_or(&candidate)
                        .split("::")
                        .next()
                        .unwrap_or_default();
                    if let Some(paths) = self.rust_crates.get(first) {
                        return select_files(source_path, paths, ResolutionMethod::ProjectModel);
                    }
                }
                unresolved(GapCode::UnresolvedTarget)
            }
            _ => unresolved(GapCode::UnresolvedTarget),
        }
    }

    fn resolve_dart(&self, source_path: &RepositoryPath, site: &ImportSite) -> ImportResolution {
        let ImportForm::DartUri { conditional } = site.form else {
            return unresolved(GapCode::UnresolvedTarget);
        };
        if conditional {
            return unresolved(GapCode::MissingProjectMetadata);
        }
        if site.specifier.starts_with("dart:") {
            return ImportResolution::KnownExternal;
        }
        if site.specifier.starts_with("package:") {
            if let Some(paths) = self.dart_package_files.get(&site.specifier) {
                return select_files(source_path, paths, ResolutionMethod::ProjectModel);
            }
            return unresolved(GapCode::MissingDependencyMetadata);
        }
        let targets = resolve_relative_candidates(
            source_path,
            &site.specifier,
            self.resolution_files
                .get(&ProgrammingLanguage::Dart)
                .unwrap_or(&self.included_files),
            &[],
            false,
        );
        select_files(source_path, &targets, ResolutionMethod::SyntaxExact)
    }
}

fn unresolved(gap: GapCode) -> ImportResolution {
    ImportResolution::Unresolved { gap }
}

fn select_files(
    source: &RepositoryPath,
    paths: &[RepositoryPath],
    method: ResolutionMethod,
) -> ImportResolution {
    let mut paths = paths
        .iter()
        .filter(|path| *path != source)
        .cloned()
        .collect::<Vec<_>>();
    canonicalize_paths(&mut paths);
    select_files_without_source(&paths, method)
}

fn select_files_without_source(
    paths: &[RepositoryPath],
    method: ResolutionMethod,
) -> ImportResolution {
    let mut paths = paths.to_vec();
    canonicalize_paths(&mut paths);
    match paths.as_slice() {
        [path] => ImportResolution::Internal {
            target: IrEndpoint::File { path: path.clone() },
            method,
        },
        [] => unresolved(GapCode::UnresolvedTarget),
        many => ImportResolution::Ambiguous {
            candidate_count: many.len() as u64,
        },
    }
}

fn select_structures(structures: &[StructureTarget], method: ResolutionMethod) -> ImportResolution {
    let mut structures = structures.to_vec();
    structures.sort_by(|left, right| {
        (&left.unit_id, left.kind, &left.qualified_name).cmp(&(
            &right.unit_id,
            right.kind,
            &right.qualified_name,
        ))
    });
    structures.dedup();
    match structures.as_slice() {
        [target] => ImportResolution::Internal {
            target: target.endpoint(),
            method,
        },
        [] => unresolved(GapCode::UnresolvedTarget),
        many => ImportResolution::Ambiguous {
            candidate_count: many.len() as u64,
        },
    }
}

fn is_relative_specifier(value: &str) -> bool {
    value == "." || value == ".." || value.starts_with("./") || value.starts_with("../")
}

fn resolve_relative_candidates(
    source_path: &RepositoryPath,
    specifier: &str,
    included_files: &BTreeSet<RepositoryPath>,
    extensions: &[&str],
    index_files: bool,
) -> Vec<RepositoryPath> {
    let base = parent_path(source_path);
    let Some(candidate) = normalize_join(&base, specifier) else {
        return Vec::new();
    };
    let mut candidates = Vec::new();
    if included_files.contains(&candidate) {
        candidates.push(candidate.clone());
    }
    if !has_extension(candidate.as_str()) {
        for extension in extensions {
            let Some(path) =
                RepositoryPath::parse(format!("{}{extension}", candidate.as_str())).ok()
            else {
                continue;
            };
            if included_files.contains(&path) {
                candidates.push(path);
            }
        }
        if index_files {
            for extension in extensions {
                let Some(path) = join_repository_path(&candidate, &format!("index{extension}"))
                else {
                    continue;
                };
                if included_files.contains(&path) {
                    candidates.push(path);
                }
            }
        }
    }
    canonicalize_paths(&mut candidates);
    candidates
}

fn normalize_join(base: &RepositoryPath, specifier: &str) -> Option<RepositoryPath> {
    if specifier.starts_with('/') {
        return None;
    }
    let mut parts = if base.is_root() {
        Vec::new()
    } else {
        base.as_str().split('/').map(str::to_string).collect()
    };
    let normalized = specifier.replace('\\', "/");
    for part in normalized.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            value if value.contains('\0') => return None,
            value => parts.push(value.to_string()),
        }
    }
    if parts.is_empty() {
        return None;
    }
    RepositoryPath::parse(parts.join("/")).ok()
}

fn has_extension(path: &str) -> bool {
    path.rsplit('/')
        .next()
        .is_some_and(|file| file.contains('.'))
}

fn python_relative_module_names(
    source_path: &RepositoryPath,
    source_modules: Option<&Vec<String>>,
    specifier: &str,
) -> Vec<String> {
    let dot_count = specifier
        .chars()
        .take_while(|character| *character == '.')
        .count();
    let suffix = specifier[dot_count..].trim_matches('.');
    let source_is_package = source_path
        .as_str()
        .rsplit('/')
        .next()
        .is_some_and(|file| matches!(file, "__init__.py" | "__init__.pyi"));
    let mut resolved = Vec::new();
    for module in source_modules.into_iter().flatten() {
        let mut parts = module.split('.').collect::<Vec<_>>();
        if !source_is_package {
            parts.pop();
        }
        for _ in 1..dot_count {
            if parts.pop().is_none() {
                parts.clear();
                break;
            }
        }
        if parts.is_empty() && suffix.is_empty() {
            continue;
        }
        let base = parts.join(".");
        let qualified = match (base.is_empty(), suffix.is_empty()) {
            (true, _) => suffix.to_string(),
            (_, true) => base,
            _ => format!("{base}.{suffix}"),
        };
        if !qualified.is_empty() {
            resolved.push(qualified);
        }
    }
    resolved.sort();
    resolved.dedup();
    resolved
}

fn longest_qualified_file_match<'a>(
    index: &'a std::collections::BTreeMap<String, Vec<RepositoryPath>>,
    qualified: &str,
) -> Option<&'a Vec<RepositoryPath>> {
    let mut candidate = qualified;
    loop {
        if let Some(paths) = index.get(candidate) {
            return Some(paths);
        }
        candidate = candidate.rsplit_once('.')?.0;
    }
}

fn starts_with_rust_standard_crate(value: &str) -> bool {
    matches!(
        value.split("::").next().unwrap_or_default(),
        "std" | "core" | "alloc"
    )
}

fn rust_import_candidates(specifier: &str, source_modules: &[String]) -> Vec<String> {
    let mut raw = specifier
        .split(" as ")
        .next()
        .unwrap_or(specifier)
        .trim()
        .trim_end_matches("::*")
        .to_string();
    if let Some((prefix, _)) = raw.split_once("::{") {
        if matches!(prefix, "crate" | "self" | "super") {
            return Vec::new();
        }
        raw = prefix.to_string();
    }
    let mut candidates = Vec::new();
    if raw == "crate" || raw.starts_with("crate::") {
        candidates.push(raw);
    } else if raw == "self" || raw.starts_with("self::") {
        let suffix = raw
            .strip_prefix("self")
            .unwrap_or(&raw)
            .trim_start_matches("::");
        for source in source_modules {
            candidates.push(if suffix.is_empty() {
                source.clone()
            } else {
                format!("{source}::{suffix}")
            });
        }
    } else if raw == "super" || raw.starts_with("super::") {
        for source in source_modules {
            let mut base = source.split("::").collect::<Vec<_>>();
            let mut suffix = raw.as_str();
            while suffix == "super" || suffix.starts_with("super::") {
                if base.len() <= 1 {
                    base.clear();
                    break;
                }
                base.pop();
                suffix = suffix
                    .strip_prefix("super")
                    .unwrap_or(suffix)
                    .trim_start_matches("::");
            }
            if !base.is_empty() {
                let base = base.join("::");
                candidates.push(if suffix.is_empty() {
                    base
                } else {
                    format!("{base}::{suffix}")
                });
            }
        }
    } else {
        candidates.push(format!("crate::{raw}"));
        candidates.push(raw);
    }
    candidates.sort();
    candidates.dedup();
    candidates
}

fn longest_rust_module_match<'a>(
    index: &'a std::collections::BTreeMap<
        (codebase_fact_model::identity::AnalysisUnitId, String),
        Vec<RepositoryPath>,
    >,
    unit_id: &codebase_fact_model::identity::AnalysisUnitId,
    module: &str,
) -> Option<&'a Vec<RepositoryPath>> {
    let mut candidate = module;
    loop {
        if let Some(paths) = index.get(&(unit_id.clone(), candidate.to_string())) {
            return Some(paths);
        }
        candidate = candidate.rsplit_once("::")?.0;
    }
}

fn canonicalize_paths(paths: &mut Vec<RepositoryPath>) {
    paths.sort();
    paths.dedup();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_path_normalization_never_escapes_repository_root() {
        let base = RepositoryPath::parse("src/features").unwrap();
        assert_eq!(
            normalize_join(&base, "../shared/types").unwrap().as_str(),
            "src/shared/types"
        );
        assert!(normalize_join(&RepositoryPath::root(), "../outside").is_none());
    }

    #[test]
    fn python_relative_import_uses_package_not_filename_guessing() {
        let source = RepositoryPath::parse("src/pkg/main.py").unwrap();
        let modules = vec!["pkg.main".to_string(), "src.pkg.main".to_string()];
        assert_eq!(
            python_relative_module_names(&source, Some(&modules), ".helpers"),
            vec!["pkg.helpers", "src.pkg.helpers"]
        );
    }

    #[test]
    fn rust_group_with_multiple_root_modules_stays_unresolved() {
        assert!(rust_import_candidates("crate::{a::A, b::B}", &["crate".to_string()]).is_empty());
        assert_eq!(
            rust_import_candidates("crate::a::{A, B}", &["crate".to_string()]),
            vec!["crate::a"]
        );
    }
}
