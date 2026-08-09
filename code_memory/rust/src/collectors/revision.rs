use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

use crate::{resolve_tool, run_bounded_command, tool_command};

use super::model::{
    properties, CollectedFact, CollectedRelation, CollectionDiagnostic, CollectionMode,
    CollectionStatus, CollectorResult, TruthClass,
};

const ID: &str = "git-revision";
const MAX_GIT_OUTPUT_BYTES: usize = 1024 * 1024;

pub(crate) fn collect(root: &Path, providers_root: Option<&Path>) -> CollectorResult {
    let mut result = CollectorResult::new(ID, "source-revision", CollectionMode::ToolAssisted);
    let resolution = resolve_tool("git", providers_root);
    result.summary.tool = Some("git".to_string());
    result.summary.tool_origin = Some(resolution.origin);
    result.summary.tool_version = resolution.version;
    if resolution.path.is_none() {
        result.summary.status = CollectionStatus::Unavailable;
        result.diagnostics.push(CollectionDiagnostic {
            collector: ID,
            level: "info",
            code: "tool-unavailable",
            message: "Git revision collection skipped because git is unavailable".to_string(),
            path: None,
        });
        return result;
    }

    let top_level = match git_text(root, providers_root, &["rev-parse", "--show-toplevel"]) {
        Ok(value) if !value.trim().is_empty() => value.trim().to_string(),
        Ok(_) => return result,
        Err(error) if error.contains("not a git repository") => return result,
        Err(error) => {
            result.summary.status = CollectionStatus::Failed;
            result.diagnostics.push(CollectionDiagnostic {
                collector: ID,
                level: "warning",
                code: "git-query-failed",
                message: error,
                path: None,
            });
            return result;
        }
    };
    result.summary.detected_by.push(
        Path::new(&top_level)
            .join(".git")
            .to_string_lossy()
            .into_owned(),
    );

    let repository_key = "git:repository".to_string();
    result.facts.push(CollectedFact {
        stable_key: repository_key.clone(),
        kind: "source-repository".to_string(),
        name: Path::new(&top_level)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("repository")
            .to_string(),
        path: Some(top_level.clone()),
        properties: BTreeMap::new(),
    });

    let head = git_text(root, providers_root, &["rev-parse", "--verify", "HEAD"])
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let branch = git_text(
        root,
        providers_root,
        &["symbolic-ref", "--quiet", "--short", "HEAD"],
    )
    .ok()
    .map(|value| value.trim().to_string())
    .filter(|value| !value.is_empty());

    let revision_key = head.as_ref().map(|head| format!("git:revision:{head}"));
    if let (Some(head), Some(revision_key)) = (&head, &revision_key) {
        result.facts.push(CollectedFact {
            stable_key: revision_key.clone(),
            kind: "source-revision".to_string(),
            name: head.clone(),
            path: None,
            properties: properties(&[("commit", Some(head)), ("branch", branch.as_deref())]),
        });
        result.relations.push(CollectedRelation {
            from: repository_key.clone(),
            to: revision_key.clone(),
            kind: "HAS_REVISION".to_string(),
            truth_class: TruthClass::Confirmed,
            evidence_type: "GIT_METADATA".to_string(),
            evidence: Vec::new(),
            properties: BTreeMap::new(),
        });
    }

    result.summary.status = if result.diagnostics.is_empty() {
        CollectionStatus::Collected
    } else {
        CollectionStatus::Partial
    };
    result
}

fn git_text(root: &Path, providers_root: Option<&Path>, args: &[&str]) -> Result<String, String> {
    let bytes = git_bytes(root, providers_root, args)?;
    String::from_utf8(bytes).map_err(|error| format!("git returned non-UTF-8 metadata: {error}"))
}

fn git_bytes(root: &Path, providers_root: Option<&Path>, args: &[&str]) -> Result<Vec<u8>, String> {
    let mut command = tool_command("git", providers_root)?;
    command.arg("-C").arg(root).args(args);
    let output = run_bounded_command(
        command,
        "git metadata query",
        Duration::from_secs(15),
        MAX_GIT_OUTPUT_BYTES,
        64 * 1024,
    )?;
    if output.stdout_truncated {
        return Err(format!(
            "git metadata output exceeded {} bytes",
            MAX_GIT_OUTPUT_BYTES
        ));
    }
    if output.status.success() {
        return Ok(output.stdout);
    }
    let detail = String::from_utf8_lossy(&output.stderr);
    Err(format!(
        "git command failed with {}: {}",
        output.status,
        detail.trim()
    ))
}
