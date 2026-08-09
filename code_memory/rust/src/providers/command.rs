use codebase_fact_model::identity::Sha256Digest;
use serde_json::Value;

use std::collections::{HashMap, HashSet};
use std::env;

use std::fs;

use std::path::{Path, PathBuf};

use std::process::Command;
use std::sync::{Mutex, OnceLock};

use crate::{LanguageSpec, ProviderKind, ProviderProvenance};

static COMPILE_DATABASE_FILES_CACHE: OnceLock<Mutex<HashMap<PathBuf, HashSet<PathBuf>>>> =
    OnceLock::new();

fn find_on_path(program: &str) -> Option<PathBuf> {
    let path_value = env::var_os("PATH")?;
    let candidates = if cfg!(windows) {
        vec![
            format!("{program}.exe"),
            format!("{program}.cmd"),
            format!("{program}.bat"),
            program.to_string(),
            format!("{program}.ps1"),
        ]
    } else {
        vec![program.to_string()]
    };
    for directory in env::split_paths(&path_value) {
        for candidate in &candidates {
            let path = directory.join(candidate);
            if path.is_file() {
                return Some(path);
            }
        }
    }
    None
}

pub(crate) fn find_tool(program: &str, providers_root: Option<&Path>) -> Option<PathBuf> {
    resolve_tool(program, providers_root).path
}

#[derive(Clone, Debug)]
pub(crate) struct ProviderResolution {
    pub(crate) path: Option<PathBuf>,
    pub(crate) origin: &'static str,
    pub(crate) version: Option<String>,
    /// Expected digest from the managed provider catalog when available. The
    /// Language IR boundary still hashes the actual file and rejects a
    /// mismatch before this value can become provenance.
    pub(crate) artifact_digest: Option<Sha256Digest>,
}

pub(crate) fn resolve_tool(program: &str, providers_root: Option<&Path>) -> ProviderResolution {
    if let Some(root) = providers_root {
        if let Some(entry) = provider_manifest_entry(root, program) {
            return ProviderResolution {
                path: Some(entry.path),
                origin: "managed-manifest",
                version: entry.version,
                artifact_digest: entry.artifact_digest,
            };
        }
        if let Some(path) = find_managed_tool(root, program) {
            return ProviderResolution {
                path: Some(path),
                origin: "managed-root",
                version: None,
                artifact_digest: None,
            };
        }
    }
    let path = find_on_path(program);
    ProviderResolution {
        origin: if path.is_some() { "path" } else { "missing" },
        path,
        version: None,
        artifact_digest: None,
    }
}

fn find_managed_tool(root: &Path, program: &str) -> Option<PathBuf> {
    let root = managed_provider_root(root);
    let candidates = if cfg!(windows) {
        vec![
            format!("{program}.exe"),
            format!("{program}.cmd"),
            format!("{program}.bat"),
            program.to_string(),
            format!("{program}.ps1"),
        ]
    } else {
        vec![program.to_string()]
    };
    let bases = [
        root.to_path_buf(),
        root.join(program).join("bin"),
        root.join(program),
        root.join("bin"),
    ];
    for base in bases {
        for candidate in &candidates {
            let path = base.join(candidate);
            if path.is_file() {
                return Some(path);
            }
        }
    }
    None
}

struct ProviderManifestEntry {
    path: PathBuf,
    version: Option<String>,
    artifact_digest: Option<Sha256Digest>,
    runtime_paths: Vec<PathBuf>,
    environment: Vec<(String, String)>,
}

fn provider_manifest_entry(root: &Path, program: &str) -> Option<ProviderManifestEntry> {
    let root = managed_provider_root(root);
    let manifest = root.join("manifest.json");
    let value: Value = serde_json::from_slice(&fs::read(manifest).ok()?).ok()?;
    let providers = value.get("providers")?.as_array()?;
    let provider = providers
        .iter()
        .find(|provider| provider.get("command").and_then(Value::as_str) == Some(program))?;
    let path = root.join(provider.get("path").and_then(Value::as_str)?);
    if !path.is_file() {
        return None;
    }
    let runtime_paths = provider
        .get("runtime_paths")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(|path| root.join(path))
        .filter(|path| path.is_dir())
        .collect();
    let environment = provider
        .get("environment")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
        .filter_map(|(key, value)| {
            Some((
                key.clone(),
                value.as_str().map(|value| {
                    let path = root.join(value);
                    if path.exists() {
                        path.to_string_lossy().to_string()
                    } else {
                        value.to_string()
                    }
                })?,
            ))
        })
        .collect();
    Some(ProviderManifestEntry {
        path,
        version: provider
            .get("version")
            .and_then(Value::as_str)
            .map(str::to_string),
        artifact_digest: provider
            .get("sha256")
            .and_then(Value::as_str)
            .map(str::to_ascii_lowercase)
            .and_then(|digest| Sha256Digest::parse(&digest).ok()),
        runtime_paths,
        environment,
    })
}

pub(crate) fn managed_provider_root(root: &Path) -> PathBuf {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    root.to_string_lossy()
        .strip_prefix("\\\\?\\")
        .map(PathBuf::from)
        .unwrap_or(root)
}

pub(crate) fn tool_command(
    program: &str,
    providers_root: Option<&Path>,
) -> Result<Command, String> {
    let path = find_tool(program, providers_root)
        .ok_or_else(|| format!("program not found: {program}"))?;
    let manifest_entry = providers_root.and_then(|root| provider_manifest_entry(root, program));
    let runtime_paths = manifest_entry
        .as_ref()
        .map(|entry| entry.runtime_paths.clone())
        .unwrap_or_default();
    let mut command = if matches!(
        path.extension().and_then(|value| value.to_str()),
        Some("cmd") | Some("bat")
    ) {
        let mut command = Command::new("cmd.exe");
        command.args(["/D", "/C"]).arg(path);
        command
    } else if path.extension().and_then(|value| value.to_str()) == Some("ps1") {
        let mut command = Command::new("powershell.exe");
        command
            .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
            .arg(path);
        command
    } else {
        Command::new(path)
    };
    if !runtime_paths.is_empty() {
        let mut path_entries = runtime_paths;
        if let Some(path) = env::var_os("PATH") {
            path_entries.extend(env::split_paths(&path));
        }
        if let Ok(path) = env::join_paths(path_entries) {
            command.env("PATH", path);
        }
    }
    if let Some(entry) = manifest_entry {
        for (key, value) in entry.environment {
            command.env(key, value);
        }
    }
    apply_provider_environment(&mut command);
    hide_console_window(&mut command);
    Ok(command)
}

/// Resolves the SDK exactly as the managed `scip-dotnet` process will see it.
/// Running this inexpensive probe before restore distinguishes an unsupported
/// repository SDK from an indexer failure and avoids spending minutes on a
/// build that cannot possibly load.
pub(crate) fn probe_dotnet_sdk(
    working_directory: &Path,
    providers_root: Option<&Path>,
) -> Result<String, String> {
    let manifest_entry =
        providers_root.and_then(|root| provider_manifest_entry(root, "scip-dotnet"));
    let dotnet = manifest_entry
        .as_ref()
        .and_then(|entry| {
            entry
                .environment
                .iter()
                .find(|(key, _)| key.eq_ignore_ascii_case("DOTNET_ROOT"))
                .map(|(_, value)| {
                    PathBuf::from(value).join(if cfg!(windows) {
                        "dotnet.exe"
                    } else {
                        "dotnet"
                    })
                })
        })
        .filter(|path| path.is_file())
        .or_else(|| find_on_path("dotnet"))
        .ok_or_else(|| "required .NET SDK resolver is unavailable".to_string())?;
    let mut command = Command::new(dotnet);
    command.arg("--version").current_dir(working_directory);
    if let Some(entry) = manifest_entry {
        if !entry.runtime_paths.is_empty() {
            let mut path_entries = entry.runtime_paths;
            if let Some(path) = env::var_os("PATH") {
                path_entries.extend(env::split_paths(&path));
            }
            if let Ok(path) = env::join_paths(path_entries) {
                command.env("PATH", path);
            }
        }
        for (key, value) in entry.environment {
            command.env(key, value);
        }
    }
    hide_console_window(&mut command);
    let output = command.output().map_err(|error| {
        format!(
            "cannot resolve the .NET SDK required by {}: {error}",
            working_directory.display()
        )
    })?;
    let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if output.status.success() && !version.is_empty() {
        return Ok(version);
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let detail = stderr
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("the repository's global.json cannot be satisfied by the installed SDKs");
    Err(format!(
        "required .NET SDK is unavailable for {}: {detail}",
        working_directory.display()
    ))
}

#[cfg(windows)]
fn hide_console_window(command: &mut Command) {
    use std::os::windows::process::CommandExt;

    // CREATE_NO_WINDOW: providers are background implementation details of the
    // desktop app and must never create a visible console for each invocation.
    command.creation_flags(0x08000000);
}

#[cfg(not(windows))]
fn hide_console_window(_command: &mut Command) {}

fn apply_provider_environment(command: &mut Command) {
    let offline = env::var("CODE_MEMORY_ALLOW_NETWORK").as_deref() != Ok("1");
    command.env("MSBUILDDISABLENODEREUSE", "1");
    command.env(
        "GRADLE_OPTS",
        provider_gradle_options(&env::var("GRADLE_OPTS").unwrap_or_default(), offline),
    );
    if !offline {
        return;
    }
    command.env("CODE_MEMORY_OFFLINE", "1");
    command.env("CARGO_NET_OFFLINE", "true");
    command.env("GOPROXY", "off");
    command.env("GOSUMDB", "off");
    command.env("GONOSUMDB", "*");
    command.env("GOTOOLCHAIN", "local");
    command.env("BUNDLE_ALLOW_OFFLINE_INSTALL", "1");
    command.env("BUNDLE_FROZEN", "1");
    command.env("DART_DISABLE_ANALYTICS", "true");
    command.env("npm_config_offline", "true");
    command.env("npm_config_audit", "false");
    command.env("npm_config_fund", "false");
    command.env("PNPM_CONFIG_OFFLINE", "true");
    command.env("YARN_ENABLE_NETWORK", "0");
    command.env("PIP_NO_INDEX", "1");
    command.env("UV_OFFLINE", "1");
    command.env("MAVEN_ARGS", "-o");
}

fn provider_gradle_options(existing: &str, offline: bool) -> String {
    let mut options = existing.trim().to_string();
    for option in [
        Some("-Dorg.gradle.daemon=false"),
        offline.then_some("-Dorg.gradle.offline=true"),
    ]
    .into_iter()
    .flatten()
    {
        if !options.is_empty() {
            options.push(' ');
        }
        options.push_str(option);
    }
    options
}

pub(crate) fn provider_ready(lang: &LanguageSpec, providers_root: Option<&Path>) -> bool {
    find_tool(lang.tool, providers_root).is_some()
        || (matches!(lang.id, "c" | "cpp") && find_tool("clangd", providers_root).is_some())
}

pub(crate) fn provider_provenance(
    lang: LanguageSpec,
    providers_root: Option<&Path>,
) -> ProviderProvenance {
    let configured = resolve_tool(lang.tool, providers_root);
    let (tool, resolution) = if configured.path.is_some() || !matches!(lang.id, "c" | "cpp") {
        (lang.tool, configured)
    } else {
        ("clangd", resolve_tool("clangd", providers_root))
    };
    ProviderProvenance {
        language: lang.id.to_string(),
        tool: tool.to_string(),
        origin: resolution.origin,
        status: if resolution.path.is_some() {
            "available"
        } else {
            "missing"
        },
        version: resolution.version,
    }
}

pub(crate) fn has_compile_context(root: &Path) -> bool {
    if compile_database_dir(root).is_some() {
        return true;
    }
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let mut candidate = Some(root.as_path());
    for _ in 0..4 {
        let Some(path) = candidate else { break };
        if clangd_has_semantic_context(&path.join(".clangd"))
            || path.join("compile_flags.txt").is_file()
        {
            return true;
        }
        candidate = path.parent();
    }
    false
}

fn clangd_has_semantic_context(path: &Path) -> bool {
    let Ok(source) = fs::read_to_string(path) else {
        return false;
    };
    // A .clangd containing only warning toggles is not enough to reconstruct
    // a project's include paths, defines, target, or language standard.
    [
        "-I",
        "-D",
        "-std=",
        "--target=",
        "--sysroot",
        "CompilationDatabase",
    ]
    .iter()
    .any(|marker| source.contains(marker))
}

pub(crate) fn has_compile_context_for_files(root: &Path, files: &[PathBuf]) -> bool {
    if let Some(database) = compile_database_dir_for_files(root, files) {
        let Some(entry_files) = compile_database_entry_files(&database) else {
            return false;
        };
        let translation_units: Vec<_> = files
            .iter()
            .filter(|file| !is_c_family_header(file))
            .collect();
        if translation_units.is_empty() {
            let project_root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
            return entry_files
                .iter()
                .any(|file| file.starts_with(&project_root));
        }
        return translation_units.iter().any(|file| {
            let expected = file.canonicalize().unwrap_or_else(|_| (*file).clone());
            entry_files.contains(&expected)
        });
    }
    has_compile_context(root)
}

/// Keep only translation units that have an actual compiler command. Headers
/// remain in the job because clangd resolves them through those translation
/// units. Files outside the active build are reported by coverage instead of
/// causing the entire module to fail.
pub(crate) fn active_c_family_files(root: &Path, files: &[PathBuf]) -> (Vec<PathBuf>, usize) {
    let Some(database) = compile_database_dir_for_files(root, files) else {
        return (files.to_vec(), 0);
    };
    let Some(entry_files) = compile_database_entry_files(&database) else {
        return (files.to_vec(), 0);
    };
    let has_active_translation_unit = files.iter().any(|file| {
        !is_c_family_header(file)
            && entry_files.contains(&file.canonicalize().unwrap_or_else(|_| file.clone()))
    });
    if !has_active_translation_unit {
        return (files.to_vec(), 0);
    }
    let mut active = Vec::with_capacity(files.len());
    let mut excluded = 0;
    for file in files {
        let is_active = is_c_family_header(file)
            || entry_files.contains(&file.canonicalize().unwrap_or_else(|_| file.clone()));
        if is_active {
            active.push(file.clone());
        } else {
            excluded += 1;
        }
    }
    (active, excluded)
}

pub(crate) fn compile_database_files_for_scope(
    root: &Path,
    files: &[PathBuf],
) -> Option<HashSet<PathBuf>> {
    let database = compile_database_dir_for_files(root, files)?;
    compile_database_entry_files(&database)
}

pub(crate) fn prepare_clangd_compile_database(
    root: &Path,
    files: &[PathBuf],
    scratch: &Path,
) -> Option<PathBuf> {
    let database = compile_database_dir_for_files(root, files)?;
    let source = fs::read_to_string(database.join("compile_commands.json")).ok()?;
    let all_entries = serde_json::from_str::<Value>(&source)
        .ok()?
        .as_array()?
        .clone();
    let requested: HashSet<PathBuf> = files
        .iter()
        .filter(|file| !is_c_family_header(file))
        .map(|file| file.canonicalize().unwrap_or_else(|_| (*file).clone()))
        .collect();
    let has_translation_units = !requested.is_empty();
    let mut seen_translation_units = HashSet::new();
    let mut entries: Vec<Value> = all_entries
        .iter()
        .filter(|entry| {
            let Some(value) = entry.get("file").and_then(Value::as_str) else {
                return false;
            };
            let path = resolve_compile_entry_path(&database, entry, value);
            if has_translation_units && !requested.contains(&path) {
                return false;
            }
            // A source file can have several configurations in a generated
            // database. The desktop needs one deterministic compiler context;
            // retaining every variant makes clangd parse the same TU again.
            seen_translation_units.insert(path)
        })
        .cloned()
        .collect();
    let changed = entries.len() != all_entries.len();
    let known: HashSet<PathBuf> = entries
        .iter()
        .filter_map(|entry| {
            let value = entry.get("file")?.as_str()?;
            Some(resolve_compile_entry_path(&database, entry, value))
        })
        .collect();
    let templates: Vec<(PathBuf, Value)> = if entries.is_empty() {
        all_entries
            .iter()
            .filter_map(|entry| {
                let value = entry.get("file")?.as_str()?;
                Some((
                    resolve_compile_entry_path(&database, entry, value),
                    entry.clone(),
                ))
            })
            .collect()
    } else {
        entries
            .iter()
            .filter_map(|entry| {
                let value = entry.get("file")?.as_str()?;
                Some((
                    resolve_compile_entry_path(&database, entry, value),
                    entry.clone(),
                ))
            })
            .collect()
    };
    let headers: Vec<_> = files
        .iter()
        .filter(|file| is_c_family_header(file))
        .filter(|file| !known.contains(&file.canonicalize().unwrap_or_else(|_| (*file).clone())))
        .collect();
    if headers.is_empty() && !changed {
        return Some(database);
    }
    let generated = scratch.join("clangd-context");
    fs::create_dir_all(&generated).ok()?;
    for header in &headers {
        let header = header.canonicalize().unwrap_or_else(|_| (**header).clone());
        let (_, template) = templates
            .iter()
            .min_by_key(|(path, _)| path_distance(path, &header))?;
        entries.push(rewrite_compile_entry(template, &header));
    }
    if entries.is_empty() && headers.is_empty() {
        return None;
    }
    fs::write(
        generated.join("compile_commands.json"),
        serde_json::to_vec(&entries).ok()?,
    )
    .ok()?;
    Some(generated)
}

fn rewrite_compile_entry(template: &Value, file: &Path) -> Value {
    let mut entry = template.clone();
    let old_file = template
        .get("file")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let new_file = file.to_string_lossy().replace('\\', "/");
    if let Some(value) = entry.get_mut("file") {
        *value = Value::String(new_file.clone());
    }
    if let Some(arguments) = entry.get_mut("arguments").and_then(Value::as_array_mut) {
        for argument in arguments {
            if argument.as_str() == Some(old_file) {
                *argument = Value::String(new_file.clone());
            }
        }
    }
    if let Some(command) = entry
        .get("command")
        .and_then(Value::as_str)
        .map(str::to_string)
    {
        let replaced = command
            .replace(old_file, &new_file)
            .replace(&old_file.replace('/', "\\"), &new_file);
        if let Some(value) = entry.get_mut("command") {
            *value = Value::String(replaced);
        }
    }
    entry
}

fn path_distance(left: &Path, right: &Path) -> usize {
    let left: Vec<_> = left.components().collect();
    let right: Vec<_> = right.components().collect();
    let common = left
        .iter()
        .zip(&right)
        .take_while(|(left, right)| left == right)
        .count();
    (left.len() - common) + (right.len() - common)
}

fn resolve_compile_entry_path(database: &Path, entry: &Value, value: &str) -> PathBuf {
    let path = PathBuf::from(value);
    let path = if path.is_absolute() {
        path
    } else {
        let directory = entry
            .get("directory")
            .and_then(Value::as_str)
            .map(PathBuf::from)
            .unwrap_or_else(|| database.to_path_buf());
        let directory = if directory.is_absolute() {
            directory
        } else {
            database.parent().unwrap_or(database).join(directory)
        };
        directory.join(path)
    };
    path.canonicalize().unwrap_or(path)
}

fn is_c_family_header(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| extension.to_ascii_lowercase())
            .as_deref(),
        Some("h" | "hh" | "hpp" | "hxx" | "inc" | "inl" | "ipp" | "tpp")
    )
}

/// Find the compile context without requiring the user's build directory to be
/// named `build`. clangd accepts a directory, so keep the search deliberately
/// shallow and deterministic instead of scanning the whole repository.
pub(crate) fn compile_database_dir(root: &Path) -> Option<PathBuf> {
    compile_database_dirs(root).into_iter().next()
}

pub(crate) fn compile_database_dirs(root: &Path) -> Vec<PathBuf> {
    compile_database_candidates(root)
        .into_iter()
        .filter(|candidate| candidate.join("compile_commands.json").is_file())
        .collect()
}

pub(crate) fn compile_database_dir_for_files(root: &Path, files: &[PathBuf]) -> Option<PathBuf> {
    let expected: HashSet<PathBuf> = files
        .iter()
        .filter(|file| !is_c_family_header(file))
        .map(|file| file.canonicalize().unwrap_or_else(|_| (*file).clone()))
        .collect();
    if expected.is_empty() {
        return compile_database_dir(root);
    }
    compile_database_candidates(root)
        .into_iter()
        .filter_map(|candidate| {
            let entry_files = compile_database_entry_files(&candidate)?;
            let matched = expected.intersection(&entry_files).count();
            Some((matched, entry_files.len(), candidate))
        })
        .max_by_key(|(matched, entries, candidate)| {
            (*matched, *entries, candidate.to_string_lossy().into_owned())
        })
        .filter(|(matched, _, _)| *matched > 0)
        .map(|(_, _, candidate)| candidate)
}

fn compile_database_candidates(root: &Path) -> Vec<PathBuf> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let mut candidates = vec![root.clone()];

    if let Ok(entries) = fs::read_dir(&root) {
        let mut children: Vec<PathBuf> = entries
            .flatten()
            .filter_map(|entry| {
                entry
                    .file_type()
                    .ok()
                    .filter(|file_type| file_type.is_dir())
                    .map(|_| entry.path())
            })
            .collect();
        children.sort();
        for child in children {
            let nested = is_build_directory(&child);
            candidates.push(child.clone());
            if nested {
                if let Ok(entries) = fs::read_dir(&child) {
                    let mut grandchildren: Vec<PathBuf> = entries
                        .flatten()
                        .filter_map(|entry| {
                            entry
                                .file_type()
                                .ok()
                                .filter(|file_type| file_type.is_dir())
                                .map(|_| entry.path())
                        })
                        .collect();
                    grandchildren.sort();
                    candidates.extend(grandchildren);
                }
            }
        }
    }

    let mut ancestor = Some(root.as_path());
    for _ in 0..4 {
        let Some(path) = ancestor else { break };
        if path != root {
            candidates.push(path.to_path_buf());
        }
        if let Ok(entries) = fs::read_dir(path) {
            let mut build_children: Vec<PathBuf> = entries
                .flatten()
                .filter_map(|entry| {
                    entry
                        .file_type()
                        .ok()
                        .filter(|file_type| file_type.is_dir())
                        .map(|_| entry.path())
                })
                .filter(|candidate| is_build_directory(candidate))
                .collect();
            build_children.sort();
            for build_child in build_children {
                candidates.push(build_child.clone());
                if let Ok(entries) = fs::read_dir(&build_child) {
                    let mut nested_build_children: Vec<PathBuf> = entries
                        .flatten()
                        .filter_map(|entry| {
                            entry
                                .file_type()
                                .ok()
                                .filter(|file_type| file_type.is_dir())
                                .map(|_| entry.path())
                        })
                        .collect();
                    nested_build_children.sort();
                    candidates.extend(nested_build_children);
                }
            }
        }
        ancestor = path.parent();
    }

    candidates
}

fn compile_database_entry_files(database: &Path) -> Option<HashSet<PathBuf>> {
    let cache = COMPILE_DATABASE_FILES_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(cache) = cache.lock() {
        if let Some(files) = cache.get(database) {
            return Some(files.clone());
        }
    }
    let source = fs::read_to_string(database.join("compile_commands.json")).ok()?;
    let entries = serde_json::from_str::<Value>(&source)
        .ok()?
        .as_array()?
        .clone();
    let mut files = HashSet::new();
    for entry in entries {
        let Some(value) = entry.get("file").and_then(Value::as_str) else {
            continue;
        };
        files.insert(resolve_compile_entry_path(database, &entry, value));
    }
    if let Ok(mut cache) = cache.lock() {
        cache.insert(database.to_path_buf(), files.clone());
    }
    Some(files)
}

fn is_build_directory(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    name == "build"
        || name == "out"
        || name.starts_with("cmake-build-")
        || name.starts_with("build-")
        || name.starts_with("out-")
}

pub(crate) fn missing_tool_message(lang: &LanguageSpec) -> String {
    match lang.provider {
        ProviderKind::Scip if matches!(lang.id, "c" | "cpp") => {
            format!("{} needs {} or clangd on PATH", lang.name, lang.tool)
        }
        ProviderKind::Scip => format!("{} needs {} on PATH", lang.name, lang.tool),
        ProviderKind::Lsp => format!("{} needs native LSP {} on PATH", lang.name, lang.tool),
    }
}

#[cfg(test)]
mod tests {
    use super::provider_gradle_options;

    #[test]
    fn provider_gradle_options_disable_persistent_daemons_in_every_mode() {
        assert_eq!(
            provider_gradle_options("-Xmx2g", false),
            "-Xmx2g -Dorg.gradle.daemon=false"
        );
        assert_eq!(
            provider_gradle_options("", true),
            "-Dorg.gradle.daemon=false -Dorg.gradle.offline=true"
        );
    }
}
