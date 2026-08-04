pub(crate) fn validate_workspace_id(workspace_id: &str) -> Result<(), String> {
    if workspace_id.is_empty() {
        return Err("프로젝트 ID가 필요합니다".to_string());
    }

    if workspace_id
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    {
        Ok(())
    } else {
        Err("프로젝트 ID에 허용되지 않는 문자가 있습니다".to_string())
    }
}

pub(crate) fn workspace_id(name: &str) -> String {
    let slug = slugify(name);
    let sequence = NEXT_WORKSPACE_ID.fetch_add(1, Ordering::Relaxed);
    format!(
        "{}-{}-{}-{}",
        slug,
        timestamp(),
        std::process::id(),
        sequence
    )
}

pub(crate) fn github_repo_name(value: &str) -> Option<String> {
    let trimmed = value.trim().trim_end_matches('/');
    let path = trimmed
        .strip_prefix("https://github.com/")
        .or_else(|| trimmed.strip_prefix("git@github.com:"))?;
    let parts = path.split('/').collect::<Vec<_>>();

    if parts.len() != 2 || !valid_github_segment(parts[0]) {
        return None;
    }

    let repo = parts[1].strip_suffix(".git").unwrap_or(parts[1]);
    if valid_github_segment(repo) {
        Some(repo.to_string())
    } else {
        None
    }
}

fn valid_github_segment(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn is_remote_url(value: &str) -> bool {
    let lower = value.trim().to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("git@")
}

fn canonical_local_repo_path(value: &str) -> Result<String, String> {
    let path = fs::canonicalize(value)
        .map_err(|error| format!("프로젝트 폴더를 찾을 수 없습니다: {error}"))?;
    if !path.is_dir() {
        return Err("프로젝트 경로는 폴더여야 합니다".to_string());
    }
    Ok(path_for_storage(&path))
}

#[cfg(windows)]
fn path_for_storage(path: &Path) -> String {
    let value = path.to_string_lossy();
    if let Some(rest) = value.strip_prefix(r"\\?\UNC\") {
        return format!(r"\\{rest}");
    }
    value.strip_prefix(r"\\?\").unwrap_or(&value).to_string()
}

#[cfg(not(windows))]
fn path_for_storage(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

