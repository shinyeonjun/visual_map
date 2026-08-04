fn select_dotnet_solution(root: &Path, source_files: &[PathBuf]) -> Option<PathBuf> {
    let mut solutions = collect_files(root, &["sln", "slnx"]);
    solutions.sort_by(|left, right| {
        dotnet_solution_score(right, source_files)
            .cmp(&dotnet_solution_score(left, source_files))
            .then_with(|| left.cmp(right))
    });
    solutions.into_iter().next()
}

pub(crate) fn dotnet_project_roots_for_files(
    root: &Path,
    source_files: &[PathBuf],
) -> Vec<PathBuf> {
    let mut roots = select_dotnet_solution(root, source_files)
        .map(|solution| solution_project_roots(&solution))
        .unwrap_or_default();
    if roots.is_empty() {
        roots = collect_files(root, &["csproj"])
            .into_iter()
            .filter_map(|project| project.parent().map(Path::to_path_buf))
            .collect();
    }
    roots
        .into_iter()
        .map(|path| path.canonicalize().unwrap_or(path))
        .collect()
}

pub(crate) fn dotnet_requires_unavailable_legacy_sdk(
    root: &Path,
    source_files: &[PathBuf],
) -> bool {
    if source_files.is_empty() {
        return false;
    }
    let mut matched = 0;
    let mut legacy = 0;
    for project in collect_files(root, &["csproj"]) {
        let Some(project_root) = project.parent() else {
            continue;
        };
        let project_root = project_root
            .canonicalize()
            .unwrap_or_else(|_| project_root.to_path_buf());
        let project_files = source_files.iter().filter(|file| {
            file.canonicalize()
                .unwrap_or_else(|_| (*file).clone())
                .starts_with(&project_root)
        });
        let count = project_files.count();
        if count == 0 {
            continue;
        }
        matched += count;
        let Ok(source) = fs::read_to_string(project) else {
            continue;
        };
        let legacy_target = source.to_ascii_lowercase().contains("windowsphone")
            || source.to_ascii_lowercase().contains("silverlight");
        if legacy_target {
            legacy += count;
        }
    }
    matched == source_files.len() && legacy == matched
}

fn dotnet_restore_state_path(root: &Path) -> PathBuf {
    project_cache_root(root).join("dotnet-restore-state")
}

fn dotnet_solution_key(solution: &Path) -> String {
    solution
        .canonicalize()
        .unwrap_or_else(|_| solution.to_path_buf())
        .to_string_lossy()
        .replace('\\', "/")
}

fn dotnet_restore_is_current(root: &Path, solution: &Path, digest: u64) -> bool {
    let expected = format!("{digest}\n{}", dotnet_solution_key(solution));
    fs::read_to_string(dotnet_restore_state_path(root))
        .is_ok_and(|state| state.trim_end() == expected)
}

fn write_dotnet_restore_state(root: &Path, solution: &Path, digest: u64) -> Result<(), String> {
    let path = dotnet_restore_state_path(root);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create .NET restore cache: {error}"))?;
    }
    fs::write(path, format!("{digest}\n{}", dotnet_solution_key(solution)))
        .map_err(|error| format!("cannot write .NET restore cache: {error}"))
}

fn dotnet_solution_score(solution: &Path, source_files: &[PathBuf]) -> (usize, bool, bool, usize) {
    let project_roots = solution_project_roots(solution);
    let matched = if project_roots.is_empty() {
        let parent = solution.parent().unwrap_or_else(|| Path::new("."));
        source_files
            .iter()
            .filter(|file| file.starts_with(parent))
            .count()
    } else {
        source_files
            .iter()
            .filter(|file| project_roots.iter().any(|root| file.starts_with(root)))
            .count()
    };
    let lower = solution.to_string_lossy().to_ascii_lowercase();
    (
        matched,
        !lower.contains("\\build\\") && !lower.contains("/build/"),
        !lower.contains("\\test") && !lower.contains("/test"),
        usize::MAX.saturating_sub(solution.components().count()),
    )
}

fn solution_project_roots(solution: &Path) -> Vec<PathBuf> {
    let Ok(source) = fs::read_to_string(solution) else {
        return Vec::new();
    };
    let parent = solution.parent().unwrap_or_else(|| Path::new("."));
    source
        .split('"')
        .filter(|value| {
            let lower = value.to_ascii_lowercase();
            lower.ends_with(".csproj")
        })
        .map(|value| parent.join(value.replace('\\', "/")))
        .map(|path| path.parent().unwrap_or(&path).to_path_buf())
        .collect()
}

fn generated_dotnet_solution(root: &Path, out: &Path) -> Result<PathBuf, String> {
    let projects = collect_files(root, &["csproj"]);
    if projects.is_empty() {
        return Err("C# requires a .sln/.slnx or at least one .csproj file".to_string());
    }
    let directory = root;
    let suffix = out
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("solution");
    let solution = directory.join(format!(".code-memory-generated-{suffix}.sln"));
    let mut text = String::from(
        "Microsoft Visual Studio Solution File, Format Version 12.00\r\n# Visual Studio Version 17\r\nVisualStudioVersion = 17.0.31903.59\r\nMinimumVisualStudioVersion = 10.0.40219.1\r\n",
    );
    let mut project_guids = Vec::new();
    for project in projects {
        let relative = project.canonicalize().unwrap_or(project.clone());
        let relative = relative_path(directory, &relative)
            .to_string_lossy()
            .replace('/', "\\");
        let name = project
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("Project");
        let guid = stable_solution_guid(&relative);
        text.push_str(&format!(
            "Project(\"{{FAE04EC0-301F-11D3-BF4B-00C04F79EFBC}}\") = \"{name}\", \"{relative}\", \"{{{guid}}}\"\r\nEndProject\r\n"
        ));
        project_guids.push(guid);
    }
    text.push_str("Global\r\n\tGlobalSection(SolutionConfigurationPlatforms) = preSolution\r\n\t\tDebug|Any CPU = Debug|Any CPU\r\n\t\tRelease|Any CPU = Release|Any CPU\r\n\tEndGlobalSection\r\n\tGlobalSection(ProjectConfigurationPlatforms) = postSolution\r\n");
    for guid in project_guids {
        text.push_str(&format!(
            "\t\t{{{guid}}}.Debug|Any CPU.ActiveCfg = Debug|Any CPU\r\n\t\t{{{guid}}}.Debug|Any CPU.Build.0 = Debug|Any CPU\r\n\t\t{{{guid}}}.Release|Any CPU.ActiveCfg = Release|Any CPU\r\n\t\t{{{guid}}}.Release|Any CPU.Build.0 = Release|Any CPU\r\n"
        ));
    }
    text.push_str("\tEndGlobalSection\r\nEndGlobal\r\n");
    fs::write(&solution, text)
        .map_err(|error| format!("cannot write generated .NET solution: {error}"))?;
    Ok(solution)
}

fn relative_path(from: &Path, to: &Path) -> PathBuf {
    let from = from.canonicalize().unwrap_or_else(|_| from.to_path_buf());
    let to = to.canonicalize().unwrap_or_else(|_| to.to_path_buf());
    let from: Vec<_> = from.components().collect();
    let to: Vec<_> = to.components().collect();
    let common = from
        .iter()
        .zip(&to)
        .take_while(|(left, right)| left == right)
        .count();
    let mut relative = PathBuf::new();
    for _ in common..from.len() {
        relative.push("..");
    }
    for component in &to[common..] {
        relative.push(component.as_os_str());
    }
    relative
}

fn stable_solution_guid(value: &str) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in value.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!(
        "{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
        hash as u32,
        (hash >> 32) as u16,
        (hash >> 48) as u16,
        (hash.rotate_left(17) >> 48) as u16,
        hash.rotate_left(29) & 0x0000_ffff_ffff_ffff,
    )
}

