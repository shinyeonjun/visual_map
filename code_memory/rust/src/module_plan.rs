use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::{
    active_c_family_files, is_excluded_source_dir, Diagnostic, DocumentOutput, LanguageAnalysis,
    LanguageOutput, LanguageSpec, RelationOutput,
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
    pub(crate) module_id: String,
    pub(crate) provider_config: Option<PathBuf>,
    pub(crate) allow_js: bool,
    pub(crate) call_ranges: Arc<HashMap<String, Vec<Vec<i32>>>>,
    pub(crate) project_excluded_files: usize,
    pub(crate) project_config_digest: u64,
}

#[derive(Clone)]
pub(crate) struct ModulePlan {
    pub(crate) id: String,
    pub(crate) root: PathBuf,
    pub(crate) files: Vec<PathBuf>,
    pub(crate) provider_config: Option<PathBuf>,
    pub(crate) allow_js: bool,
    pub(crate) project_excluded_files: usize,
}

pub(crate) fn plan_language_modules(
    root: &Path,
    lang: LanguageSpec,
    files: &[PathBuf],
) -> Vec<ModulePlan> {
    // Keep each explicit project/package marker as a restartable unit. Native
    // providers are expensive, but sending a whole workspace in one session
    // makes one bad crate/package abort the entire map.
    if !matches!(
        lang.id,
        "go" | "java" | "rust" | "dart" | "ruby" | "python" | "c" | "cpp"
    ) {
        return vec![ModulePlan {
            id: String::from("root"),
            root: root.to_path_buf(),
            files: files.to_vec(),
            provider_config: None,
            allow_js: false,
            project_excluded_files: 0,
        }];
    }
    if lang.id == "java" && java_reactor_root(root) {
        // One JDTLS session must own a Maven/Gradle reactor. Splitting its
        // included builds starts several JVMs and loses cross-module symbols;
        // the provider already knows how to resolve the reactor from its root.
        return vec![ModulePlan {
            id: String::from("root"),
            root: root.to_path_buf(),
            files: files.to_vec(),
            provider_config: None,
            allow_js: false,
            project_excluded_files: 0,
        }];
    }
    let markers = module_markers(lang.id);
    let mut roots = HashSet::from([root.to_path_buf()]);
    if !markers.is_empty() {
        collect_module_marker_roots(root, markers, &mut roots);
    }

    let mut roots: Vec<PathBuf> = roots
        .into_iter()
        .filter(|candidate| files.iter().any(|file| file.starts_with(candidate)))
        .collect();
    if matches!(lang.id, "c" | "cpp") {
        // CMake/Meson/VCXPROJ marker files describe structure, not compiler
        // flags. Only split when a nested directory owns an actual context;
        // otherwise one deterministic failure is better than one failure per
        // CMakeLists.txt.
        roots.retain(|candidate| candidate == root || has_local_compile_context(candidate));
    }
    roots.sort_by_key(|path| path.components().count());

    let mut modules = roots
        .iter()
        .map(|module_root| ModulePlan {
            id: module_id(root, module_root),
            root: module_root.clone(),
            files: Vec::new(),
            provider_config: None,
            allow_js: false,
            project_excluded_files: 0,
        })
        .collect::<Vec<_>>();

    for file in files {
        let Some(module_index) = roots
            .iter()
            .enumerate()
            .filter(|(_, module_root)| file.starts_with(module_root))
            .max_by_key(|(_, module_root)| module_root.components().count())
            .map(|(index, _)| index)
        else {
            continue;
        };
        modules[module_index].files.push(file.clone());
    }

    if matches!(lang.id, "c" | "cpp") {
        for module in &mut modules {
            let (active_files, excluded) = active_c_family_files(&module.root, &module.files);
            module.files = active_files;
            module.project_excluded_files = excluded;
        }
    }
    modules.retain(|module| !module.files.is_empty());
    if lang.id == "dart" {
        // Dart analysis_server can stall while opening a very large package.
        // Keep each session below the provider's workspace pressure point,
        // but preserve the package root so package imports remain resolvable.
        const MAX_DART_FILES_PER_PROVIDER: usize = 512;
        let mut split = Vec::new();
        for module in modules {
            if module.files.len() <= MAX_DART_FILES_PER_PROVIDER {
                split.push(module);
                continue;
            }
            for (chunk_index, files) in module.files.chunks(MAX_DART_FILES_PER_PROVIDER).enumerate()
            {
                split.push(ModulePlan {
                    id: format!("{}:chunk-{chunk_index}", module.id),
                    root: module.root.clone(),
                    files: files.to_vec(),
                    provider_config: module.provider_config.clone(),
                    allow_js: module.allow_js,
                    project_excluded_files: if chunk_index == 0 {
                        module.project_excluded_files
                    } else {
                        0
                    },
                });
            }
        }
        return split;
    }
    modules
}

pub(crate) fn plan_typescript_modules(
    root: &Path,
    lang: LanguageSpec,
    files: &[PathBuf],
    units: &[crate::project_model::ProjectModelUnit],
) -> Vec<ModulePlan> {
    let allowed: HashSet<String> = files
        .iter()
        .filter_map(|file| file.strip_prefix(root).ok())
        .map(|file| file.to_string_lossy().replace('\\', "/"))
        .collect();
    let mut order: Vec<usize> = (0..units.len()).collect();
    order.sort_by_key(|index| {
        let unit = &units[*index];
        let config_depth = unit
            .config
            .as_deref()
            .or(unit.base_config.as_deref())
            .map(|path| path.matches('/').count())
            .unwrap_or(0);
        (unit.synthetic, std::cmp::Reverse(config_depth))
    });
    let mut assigned = HashSet::new();
    let mut modules = Vec::new();
    for index in order {
        let unit = &units[index];
        let mut unit_files = Vec::new();
        for relative in &unit.files {
            let normalized = relative.replace('\\', "/");
            if !allowed.contains(&normalized)
                || assigned.contains(&normalized)
                || !lang.extensions.iter().any(|extension| {
                    normalized
                        .to_ascii_lowercase()
                        .ends_with(&format!(".{extension}"))
                })
            {
                continue;
            }
            assigned.insert(normalized.clone());
            unit_files.push(root.join(relative));
        }
        if unit_files.is_empty() {
            continue;
        }
        unit_files.sort();
        modules.push(ModulePlan {
            id: format!("tsjs:{}", unit.id.replace([':', '\\', '/'], "_")),
            root: root.to_path_buf(),
            files: unit_files,
            provider_config: unit
                .generated_config
                .clone()
                .or_else(|| unit.config.as_ref().map(|path| root.join(path))),
            allow_js: unit.allow_js,
            project_excluded_files: 0,
        });
    }
    modules
}

pub(crate) fn typescript_config_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_typescript_config_files(root, &mut files);
    files.sort();
    files
}

fn collect_typescript_config_files(dir: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() {
            if !is_excluded_config_dir(&entry.file_name().to_string_lossy()) {
                collect_typescript_config_files(&path, files);
            }
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
        if (name == "tsconfig.json" || name == "jsconfig.json")
            || ((name.starts_with("tsconfig.") || name.starts_with("jsconfig."))
                && name.ends_with(".json"))
        {
            files.push(path);
        }
    }
}

fn is_excluded_config_dir(name: &str) -> bool {
    matches!(
        name,
        ".git"
            | ".code_memory"
            | ".cache"
            | "node_modules"
            | "vendor"
            | "build"
            | "dist"
            | "coverage"
            | "target"
            | "out"
            | "generated"
            | "gen"
            | "tmp"
    )
}

fn module_markers(language: &str) -> &'static [&'static str] {
    match language {
        "go" => &["go.work", "go.mod"],
        "java" => &[
            "pom.xml",
            "settings.gradle",
            "settings.gradle.kts",
            "build.gradle",
            "build.gradle.kts",
        ],
        "rust" => &["Cargo.toml"],
        "dart" => &["pubspec.yaml"],
        "ruby" => &["Gemfile", ".ruby-version"],
        "python" => &[
            "pyproject.toml",
            "pyrightconfig.json",
            "setup.py",
            "setup.cfg",
        ],
        "c" | "cpp" => &[
            "compile_commands.json",
            "compile_flags.txt",
            ".clangd",
            "CMakeLists.txt",
            "meson.build",
            ".vcxproj",
        ],
        "csharp" => &[".sln", ".slnx", ".csproj"],
        "typescript" | "javascript" => &["tsconfig.json", "jsconfig.json"],
        "php" => &["composer.json"],
        _ => &[],
    }
}

fn collect_module_marker_roots(dir: &Path, markers: &[&str], roots: &mut HashSet<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let entries: Vec<_> = entries.flatten().collect();
    let mut has_marker = false;
    for entry in &entries {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_file() {
            let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
            if markers.iter().any(|marker| {
                let marker = marker.to_ascii_lowercase();
                name == marker || (marker.starts_with('.') && name.ends_with(&marker))
            }) {
                has_marker = true;
            }
        }
    }
    if has_marker {
        roots.insert(dir.to_path_buf());
    }
    for entry in &entries {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() && !is_excluded_source_dir(&entry.file_name().to_string_lossy()) {
            collect_module_marker_roots(&path, markers, roots);
        }
    }
}

fn module_id(project_root: &Path, module_root: &Path) -> String {
    if project_root == module_root {
        return String::from("root");
    }
    module_root
        .strip_prefix(project_root)
        .unwrap_or(module_root)
        .to_string_lossy()
        .replace('\\', "/")
}

fn has_local_compile_context(root: &Path) -> bool {
    if root.join("compile_commands.json").is_file()
        || root.join("compile_flags.txt").is_file()
        || root.join(".clangd").is_file()
    {
        return true;
    }
    let Ok(entries) = fs::read_dir(root) else {
        return false;
    };
    entries.flatten().any(|entry| {
        entry.file_type().is_ok_and(|file_type| file_type.is_dir())
            && entry.path().join("compile_commands.json").is_file()
    })
}

fn java_reactor_root(root: &Path) -> bool {
    if root.join("settings.gradle").is_file() || root.join("settings.gradle.kts").is_file() {
        return true;
    }
    fs::read_to_string(root.join("pom.xml"))
        .ok()
        .is_some_and(|source| source.contains("<modules>") && source.contains("<module>"))
}

pub(crate) fn rebase_language_analysis(
    analysis: &mut LanguageAnalysis,
    module_root: &Path,
    project_root: &Path,
) {
    let mut symbol_prefixes = HashMap::new();
    for document in &analysis.documents {
        let global_path = rebase_relative_path(module_root, project_root, &document.path);
        let old_prefix = format!(
            "lsp . . . {}",
            document.path.replace('/', ".").replace('\\', ".")
        );
        let new_prefix = format!(
            "lsp . . . {}",
            global_path.replace('/', ".").replace('\\', ".")
        );
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

pub(crate) fn merge_language_analyses(
    analyses: Vec<LanguageAnalysis>,
) -> (
    Vec<LanguageOutput>,
    Vec<DocumentOutput>,
    Vec<RelationOutput>,
    Vec<Diagnostic>,
) {
    let mut grouped: HashMap<String, Vec<LanguageAnalysis>> = HashMap::new();
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
    merge_duplicate_documents(&mut documents);
    let mut unique_relations = HashSet::new();
    relations.retain(|relation| {
        unique_relations.insert((
            relation.from.clone(),
            relation.to.clone(),
            relation.kind.clone(),
            relation.path.clone(),
            relation.range.clone(),
        ))
    });
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

fn merge_duplicate_documents(documents: &mut Vec<DocumentOutput>) {
    let mut merged = Vec::with_capacity(documents.len());
    let mut positions = HashMap::with_capacity(documents.len());
    for document in documents.drain(..) {
        let path = document.path.clone();
        let Some(index) = positions.get(&path).copied() else {
            positions.insert(path, merged.len());
            merged.push(document);
            continue;
        };
        let existing = &mut merged[index];
        // C++ clangd usually has the richer interpretation of a shared C/C++
        // header. Keep one path while retaining facts returned by either pass.
        if document.language == "cpp" && existing.language == "c" {
            existing.language = document.language.clone();
        }
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
    merged.sort_by(|left, right| left.path.cmp(&right.path));
    *documents = merged;
}

fn merge_language_status(left: &'static str, right: &'static str) -> &'static str {
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
