fn clone_github_repo(url: &str, target: &Path) -> Result<(), String> {
    if target.exists() {
        return Err("관리 프로젝트 폴더가 이미 있어 복제할 수 없습니다".to_string());
    }

    let parent = target
        .parent()
        .ok_or_else(|| "관리 프로젝트 경로를 만들 수 없습니다".to_string())?;
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;

    let target_string = target.display().to_string();
    let args = ["clone", "--depth", "1", url, target_string.as_str()];
    let result = run_git(&args, Duration::from_secs(180)).and_then(|run| {
        if run.ok {
            Ok(())
        } else {
            Err(git_failure("GitHub 프로젝트 복제 실패", &run))
        }
    });
    if result.is_err() {
        let _ = fs::remove_dir_all(target);
    }
    result
}

fn run_git(args: &[&str], timeout: Duration) -> Result<engine::EngineRunResult, String> {
    engine::run_command_with_env(
        Path::new("git"),
        args,
        timeout,
        &[("GIT_TERMINAL_PROMPT", "0")],
    )
    .map_err(|error| format!("git 실행 실패: {error}"))
}

fn git_failure(context: &str, run: &engine::EngineRunResult) -> String {
    let detail = if run.stderr.trim().is_empty() {
        run.stdout.trim()
    } else {
        run.stderr.trim()
    };
    format!("{context}: {detail}")
}

fn slugify(value: &str) -> String {
    let mut slug = String::new();

    for character in value.chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character.to_ascii_lowercase());
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }

    let slug = slug.trim_matches('-');
    if slug.is_empty() {
        "workspace".to_string()
    } else {
        slug.to_string()
    }
}

pub(crate) fn timestamp() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .to_string()
}

