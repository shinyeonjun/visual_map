fn code_source_revision(workspace: &Workspace) -> Option<(String, String)> {
    let root = Path::new(&workspace.repo_path);
    git_source_revision(root).or_else(|| folder_source_revision(root))
}

fn git_source_revision(root: &Path) -> Option<(String, String)> {
    let head = git_output(root, &["rev-parse", "HEAD"])?;
    let head = String::from_utf8(head).ok()?.trim().to_string();
    if head.len() < 7 {
        return None;
    }
    let prefix = git_output(root, &["rev-parse", "--show-prefix"])?;
    let prefix = PathBuf::from(
        String::from_utf8(prefix)
            .ok()?
            .trim_end_matches(['/', '\\']),
    );
    let tracked = git_output(root, &["ls-files", "-s", "-z", "--", "."])?;
    let status = git_output(
        root,
        &[
            "status",
            "--porcelain=v1",
            "-z",
            "--untracked-files=all",
            "--",
            ".",
        ],
    )?;
    let paths = git_changed_paths(&status)?;
    let mut hasher = Sha256::new();
    hasher.update(b"git-scope\0");
    hasher.update(&tracked);
    hasher.update(b"\0status\0");
    hasher.update(&status);
    for relative in &paths {
        let scoped = if prefix.as_os_str().is_empty() {
            relative.as_path()
        } else {
            relative.strip_prefix(&prefix).ok()?
        };
        hasher.update(b"\0path\0");
        hasher.update(scoped.to_string_lossy().as_bytes());
        hash_path_state(&mut hasher, &root.join(scoped))?;
    }
    let revision = format!("{:X}", hasher.finalize());
    let state = if paths.is_empty() {
        "clean".to_string()
    } else {
        format!("변경 {}개", paths.len())
    };
    let branch = git_output(root, &["symbolic-ref", "--quiet", "--short", "HEAD"])
        .and_then(|value| String::from_utf8(value).ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "detached".to_string());
    Some((revision, format!("{branch}@{} · {state}", &head[..7])))
}

fn git_output(root: &Path, args: &[&str]) -> Option<Vec<u8>> {
    let mut command = Command::new("git");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let output = command.arg("-C").arg(root).args(args).output().ok()?;
    output.status.success().then_some(output.stdout)
}

fn git_changed_paths(status: &[u8]) -> Option<BTreeSet<PathBuf>> {
    let records = status
        .split(|byte| *byte == 0)
        .filter(|record| !record.is_empty())
        .collect::<Vec<_>>();
    let mut paths = BTreeSet::new();
    let mut index = 0;
    while index < records.len() {
        let record = records[index];
        if record.len() < 4 || record[2] != b' ' {
            return None;
        }
        let relative = PathBuf::from(String::from_utf8_lossy(&record[3..]).into_owned());
        if relative.is_absolute()
            || relative.components().any(|component| {
                matches!(
                    component,
                    std::path::Component::ParentDir
                        | std::path::Component::RootDir
                        | std::path::Component::Prefix(_)
                )
            })
        {
            return None;
        }
        paths.insert(relative);
        if matches!(record[0], b'R' | b'C') || matches!(record[1], b'R' | b'C') {
            index += 1;
        }
        index += 1;
    }
    Some(paths)
}

fn folder_source_revision(root: &Path) -> Option<(String, String)> {
    let root = fs::canonicalize(root).ok()?;
    let mut files = Vec::new();
    // ponytail: non-Git folders are scanned in full; add a persisted manifest cache only if
    // measured startup time becomes material on very large source trees.
    collect_source_files(&root, &mut files)?;
    files.sort();
    let mut hasher = Sha256::new();
    hasher.update(b"folder\0");
    for path in &files {
        let relative = path.strip_prefix(&root).ok()?;
        hasher.update(relative.to_string_lossy().as_bytes());
        hash_path_state(&mut hasher, path)?;
    }
    let revision = format!("{:X}", hasher.finalize());
    Some((
        revision.clone(),
        format!("파일 {}개 · {}", files.len(), short_revision(&revision)),
    ))
}

fn collect_source_files(directory: &Path, files: &mut Vec<PathBuf>) -> Option<()> {
    collect_files(directory, files, ignored_source_directory)
}

fn collect_files(
    directory: &Path,
    files: &mut Vec<PathBuf>,
    skip_directory: fn(&str) -> bool,
) -> Option<()> {
    let mut entries = fs::read_dir(directory)
        .ok()?
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let file_type = entry.file_type().ok()?;
        if file_type.is_symlink() {
            continue;
        }
        let path = entry.path();
        if file_type.is_dir() {
            if !skip_directory(&entry.file_name().to_string_lossy()) {
                collect_files(&path, files, skip_directory)?;
            }
        } else if file_type.is_file() {
            files.push(path);
        }
    }
    Some(())
}

fn ignored_source_directory(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        ".git"
            | ".codex"
            | ".idea"
            | ".next"
            | ".openai"
            | ".venv"
            | ".vscode"
            | "__pycache__"
            | "build"
            | "coverage"
            | "dist"
            | "node_modules"
            | "out"
            | "target"
            | "venv"
    )
}

fn db_source_revision(profile: &DbProfile) -> Option<(String, String)> {
    let path = Path::new(profile.path.as_deref()?);
    match &profile.source {
        DbSource::DdlSqlite => ddl_source_revision(path),
        DbSource::Sqlite => {
            let metadata = path.metadata().ok()?;
            let modified = metadata
                .modified()
                .ok()?
                .duration_since(UNIX_EPOCH)
                .ok()?
                .as_nanos();
            let mut hasher = Sha256::new();
            hasher.update(metadata.len().to_le_bytes());
            hasher.update(modified.to_le_bytes());
            let revision = format!("{:X}", hasher.finalize());
            Some((
                revision.clone(),
                format!("SQLite {}", short_revision(&revision)),
            ))
        }
        _ => None,
    }
}

fn ddl_source_revision(path: &Path) -> Option<(String, String)> {
    if path.is_file() {
        let revision = hash_file(path)?;
        return Some((
            revision.clone(),
            format!("DDL {}", short_revision(&revision)),
        ));
    }
    if !path.is_dir() {
        return None;
    }

    let root = fs::canonicalize(path).ok()?;
    let mut files = Vec::new();
    collect_files(&root, &mut files, |_| false)?;
    files.sort();
    let mut hasher = Sha256::new();
    hasher.update(b"ddl-directory\0");
    for file in &files {
        let relative = file.strip_prefix(&root).ok()?;
        hasher.update(relative.to_string_lossy().as_bytes());
        hash_path_state(&mut hasher, file)?;
    }
    let revision = format!("{:X}", hasher.finalize());
    Some((
        revision.clone(),
        format!("DDL {} · 파일 {}개", short_revision(&revision), files.len()),
    ))
}

fn hash_file(path: &Path) -> Option<String> {
    if !path.is_file() {
        return None;
    }
    let mut hasher = Sha256::new();
    hash_path_state(&mut hasher, path)?;
    Some(format!("{:X}", hasher.finalize()))
}

fn hash_path_state(hasher: &mut Sha256, path: &Path) -> Option<()> {
    if !path.exists() {
        hasher.update(b"\0missing");
        return Some(());
    }
    if path.is_dir() {
        hasher.update(b"\0directory");
        return Some(());
    }
    let mut file = fs::File::open(path).ok()?;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).ok()?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Some(())
}

fn short_revision(revision: &str) -> &str {
    revision.get(..8).unwrap_or(revision)
}

fn code_source_type(workspace: &Workspace) -> String {
    let normalized = workspace.repo_path.replace('\\', "/");
    if normalized.ends_with(&format!("/workspaces/{}/repo", workspace.id)) {
        "github-clone".to_string()
    } else {
        "local-folder".to_string()
    }
}

fn db_source_key(source: &impl Serialize) -> String {
    serde_json::to_value(source)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_else(|| "unknown".to_string())
}

fn localized_db_capability_warning(value: &str) -> String {
    if let Some((capability, source)) = db_capability_parts(value, " is partially tracked by the ")
    {
        return format!("{source} 어댑터는 {capability}를 일부만 추적합니다.");
    }
    if let Some((capability, source)) = db_capability_parts(value, " is not tracked by the ") {
        return format!("{source} 어댑터는 {capability}를 추적하지 않습니다.");
    }
    if let Some((capability, source)) = db_capability_parts(value, " support is unknown for the ") {
        return format!("{source} 어댑터의 {capability} 지원 여부를 확인할 수 없습니다.");
    }

    match value {
        "SQLite CHECK and UNIQUE constraints are not emitted as constraint nodes." => {
            "SQLite CHECK·UNIQUE 제약은 제약 노드로 수집하지 않습니다.".to_string()
        }
        "SQLite partial-index predicates and expression-index expressions are not extracted." => {
            "SQLite 부분 인덱스 조건식과 표현식 인덱스 식은 수집하지 않습니다.".to_string()
        }
        "SQLite generated columns are identified, but generation expressions are not extracted." => {
            "SQLite 생성 열 여부는 식별하지만 생성식은 수집하지 않습니다.".to_string()
        }
        "SQLite view dependencies are resolved from prepare-time read authorization; trigger-body dependencies are not emitted." => {
            "SQLite 뷰 의존성은 준비 단계 읽기 권한으로 확인하며, 트리거 본문의 의존성은 수집하지 않습니다.".to_string()
        }
        _ => value.to_string(),
    }
}

fn db_capability_parts<'a>(value: &'a str, separator: &str) -> Option<(&'static str, &'a str)> {
    let (capability, source) = value.split_once(separator)?;
    let source = source.strip_suffix(" adapter.")?;
    Some((
        match capability {
            "view dependency metadata" => "뷰 의존성 메타데이터",
            "trigger dependency metadata" => "트리거 의존성 메타데이터",
            "routine dependency metadata" => "프로시저·함수 의존성 메타데이터",
            "cross-object dependency metadata" => "객체 간 의존성 메타데이터",
            _ => return None,
        },
        match source {
            "ddl-sqlite" => "SQLite DDL",
            "sqlite" => "SQLite",
            "postgres" => "PostgreSQL",
            "mysql" => "MySQL/MariaDB",
            "sqlserver" => "SQL Server",
            "oracle" => "Oracle",
            _ => source,
        },
    ))
}
