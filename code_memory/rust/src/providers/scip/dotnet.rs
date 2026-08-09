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

/// Returns the exact C# source set admitted by statically declared MSBuild
/// `<Compile Remove="...">` items. These files are repository artifacts (for
/// example generated-code baselines), but they are not compiler inputs and
/// must not be reported as semantic-provider failures.
///
/// Only literal, unconditional remove globs are consumed. Property/item
/// expressions and conditioned items are deliberately left unresolved rather
/// than guessed; the semantic provider's ordinary partial-coverage contract
/// remains responsible for those cases.
pub(crate) fn active_csharp_files(
    root: &Path,
    source_files: &[PathBuf],
) -> (Vec<PathBuf>, usize) {
    let mut excluded = HashSet::<PathBuf>::new();
    for project in collect_files(root, &["csproj"]) {
        let Ok(source) = fs::read_to_string(&project) else {
            continue;
        };
        let matchers = unconditional_compile_remove_matchers(&source);
        if matchers.is_empty() {
            continue;
        }
        let Some(project_root) = project.parent() else {
            continue;
        };
        let project_root = project_root
            .canonicalize()
            .unwrap_or_else(|_| project_root.to_path_buf());
        for file in source_files {
            let canonical = file.canonicalize().unwrap_or_else(|_| file.clone());
            let Ok(relative) = canonical.strip_prefix(&project_root) else {
                continue;
            };
            let relative = relative.to_string_lossy().replace('\\', "/");
            if matchers.iter().any(|matcher| matcher.is_match(&relative)) {
                excluded.insert(canonical);
            }
        }
    }
    let active = source_files
        .iter()
        .filter(|file| {
            let canonical = file.canonicalize().unwrap_or_else(|_| (*file).clone());
            !excluded.contains(&canonical)
        })
        .cloned()
        .collect();
    (active, excluded.len())
}

fn unconditional_compile_remove_matchers(source: &str) -> Vec<globset::GlobMatcher> {
    let mut matchers = Vec::new();
    let mut cursor = 0usize;
    while let Some(relative_start) = source[cursor..].find("<Compile") {
        let start = cursor + relative_start;
        let after_name = start + "<Compile".len();
        if source
            .as_bytes()
            .get(after_name)
            .is_some_and(|byte| !byte.is_ascii_whitespace() && *byte != b'/' && *byte != b'>')
        {
            cursor = after_name;
            continue;
        }
        let Some(relative_end) = source[after_name..].find('>') else {
            break;
        };
        let end = after_name + relative_end + 1;
        let tag = &source[start..end];
        cursor = end;
        if xml_attribute(tag, "Condition").is_some()
            || enclosing_item_group_is_conditional(source, start)
        {
            continue;
        }
        let Some(remove) = xml_attribute(tag, "Remove") else {
            continue;
        };
        for pattern in remove.split(';').map(str::trim).filter(|item| !item.is_empty()) {
            if pattern.contains("$(") || pattern.contains("@(") || pattern.contains("%(") {
                continue;
            }
            let pattern = pattern
                .replace('\\', "/")
                .trim_start_matches("./")
                .to_string();
            let Ok(glob) = globset::GlobBuilder::new(&pattern)
                .literal_separator(true)
                .backslash_escape(false)
                .build()
            else {
                continue;
            };
            matchers.push(glob.compile_matcher());
        }
    }
    matchers
}

fn enclosing_item_group_is_conditional(source: &str, position: usize) -> bool {
    let prefix = &source[..position];
    let Some(open) = prefix.rfind("<ItemGroup") else {
        return false;
    };
    if prefix.rfind("</ItemGroup>").is_some_and(|close| close > open) {
        return false;
    }
    let Some(relative_end) = source[open..].find('>') else {
        return false;
    };
    xml_attribute(&source[open..open + relative_end + 1], "Condition").is_some()
}

fn xml_attribute(tag: &str, expected: &str) -> Option<String> {
    let bytes = tag.as_bytes();
    let mut cursor = 1usize;
    while cursor < bytes.len() {
        while cursor < bytes.len()
            && (bytes[cursor].is_ascii_whitespace()
                || matches!(bytes[cursor], b'<' | b'/' | b'>'))
        {
            cursor += 1;
        }
        let name_start = cursor;
        while cursor < bytes.len()
            && (bytes[cursor].is_ascii_alphanumeric()
                || matches!(bytes[cursor], b'_' | b':' | b'-' | b'.'))
        {
            cursor += 1;
        }
        if cursor == name_start {
            cursor += 1;
            continue;
        }
        let name = &tag[name_start..cursor];
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        if bytes.get(cursor) != Some(&b'=') {
            continue;
        }
        cursor += 1;
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        let quote = *bytes.get(cursor)?;
        if !matches!(quote, b'\'' | b'"') {
            continue;
        }
        cursor += 1;
        let value_start = cursor;
        while cursor < bytes.len() && bytes[cursor] != quote {
            cursor += 1;
        }
        let value = tag.get(value_start..cursor)?;
        cursor += usize::from(cursor < bytes.len());
        if name.eq_ignore_ascii_case(expected) {
            return Some(value.to_string());
        }
    }
    None
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
    solution_project_references(&source)
        .into_iter()
        .filter(|value| value.to_ascii_lowercase().ends_with(".csproj"))
        .map(|value| parent.join(value.replace('\\', "/")))
        .map(|path| path.parent().unwrap_or(&path).to_path_buf())
        .collect()
}

fn solution_has_non_csharp_projects(solution: &Path) -> bool {
    fs::read_to_string(solution).is_ok_and(|source| {
        solution_project_references(&source)
            .into_iter()
            .any(|value| !value.to_ascii_lowercase().ends_with(".csproj"))
    })
}

fn solution_project_references(source: &str) -> Vec<&str> {
    source
        .split('"')
        .filter(|value| {
            let lower = value.to_ascii_lowercase();
            [
                ".csproj",
                ".fsproj",
                ".vbproj",
                ".vcxproj",
                ".shproj",
                ".proj",
            ]
            .iter()
            .any(|extension| lower.ends_with(extension))
        })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solution_reference_inventory_handles_sln_and_slnx_forms() {
        let source = r#"
          Project("{type}") = "App", "src\App\App.csproj", "{id}"
          <Project Path="src/Worker/Worker.fsproj" />
          <Project Path="src/Legacy/Legacy.vbproj" />
        "#;
        assert_eq!(
            solution_project_references(source),
            vec![
                "src\\App\\App.csproj",
                "src/Worker/Worker.fsproj",
                "src/Legacy/Legacy.vbproj"
            ]
        );
    }

    #[test]
    fn literal_unconditional_compile_remove_is_exactly_matchable() {
        let source = r#"
          <Project>
            <ItemGroup>
              <Compile Remove="Scaffolding\Baselines\**\*" />
              <Compile Remove="Internal\**" Condition="'$(Mode)' == 'test'" />
            </ItemGroup>
            <ItemGroup Condition="'$(Mode)' == 'legacy'">
              <Compile Remove="Legacy\**" />
            </ItemGroup>
          </Project>
        "#;
        let matchers = unconditional_compile_remove_matchers(source);
        assert_eq!(matchers.len(), 1);
        assert!(matchers[0].is_match("Scaffolding/Baselines/BigModel/DbContextModel.cs"));
        assert!(!matchers[0].is_match("Scaffolding/ScaffoldingTest.cs"));
        assert!(!matchers[0].is_match("Internal/StateManager.cs"));
    }
}
