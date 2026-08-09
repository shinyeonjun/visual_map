mod support;

use codebase_semantic_compiler::{compile_base_prompt, parse_and_verify_base_response};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};
use support::fixture_draft;

/// Real-model prompt evaluation. It is ignored in normal CI because it needs
/// an installed, authenticated Codex CLI and consumes provider capacity.
#[test]
#[ignore = "requires an installed and authenticated Codex CLI"]
fn codex_groups_the_reviewed_commerce_fixture_without_repository_access() {
    let (draft, ids) = fixture_draft();
    let compiled = compile_base_prompt(draft).unwrap();
    let temp = unique_eval_dir();
    fs::create_dir(&temp).unwrap();
    let _cleanup = TempEvalDir(temp.clone());
    let schema_path = temp.join("base-semantic-output.schema.json");
    let output_path = temp.join("provider-output.json");
    fs::write(&schema_path, compiled.output_schema_pretty_json().unwrap()).unwrap();

    let mut child = Command::new("codex")
        .args([
            "exec",
            "--ephemeral",
            "--ignore-user-config",
            "--ignore-rules",
            "--skip-git-repo-check",
            "--sandbox",
            "read-only",
            "--output-schema",
        ])
        .arg(&schema_path)
        .arg("--output-last-message")
        .arg(&output_path)
        .args(["--model", &compiled.packet.provider.model, "-"])
        .current_dir(&temp)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start Codex CLI");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(compiled.rendered_prompt().as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "Codex failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let raw = fs::read_to_string(&output_path).unwrap();
    let approved = parse_and_verify_base_response(&compiled, &raw).unwrap();
    assert!(approved.unassigned_regions.is_empty());

    let order_area_ids = approved
        .assignments
        .iter()
        .filter(|assignment| assignment.region_id == ids.order_region)
        .map(|assignment| &assignment.area_id)
        .collect::<Vec<_>>();
    let auth_area_ids = approved
        .assignments
        .iter()
        .filter(|assignment| assignment.region_id == ids.auth_region)
        .map(|assignment| &assignment.area_id)
        .collect::<Vec<_>>();
    assert_eq!(order_area_ids.len(), 1);
    assert_eq!(auth_area_ids.len(), 1);

    let order_text = semantic_lineage_text(&approved, order_area_ids[0]);
    let auth_text = semantic_lineage_text(&approved, auth_area_ids[0]);
    println!("approved order meaning: {order_text}");
    println!("approved auth meaning: {auth_text}");
    assert!(
        order_text.contains("주문"),
        "order region did not receive an order meaning: {order_text}"
    );
    assert!(
        auth_text.contains("인증") || auth_text.contains("세션"),
        "auth region did not receive an auth/session meaning: {auth_text}"
    );
}

fn semantic_lineage_text(
    revision: &codebase_semantic_model::ApprovedSemanticRevision,
    leaf_id: &codebase_semantic_model::SemanticAreaId,
) -> String {
    let leaf = revision
        .areas
        .iter()
        .find(|area| &area.area_id == leaf_id)
        .unwrap();
    let mut parts = vec![leaf.label.clone(), leaf.summary.clone()];
    if let Some(parent_id) = &leaf.parent_area_id {
        let parent = revision
            .areas
            .iter()
            .find(|area| &area.area_id == parent_id)
            .unwrap();
        parts.push(parent.label.clone());
        parts.push(parent.summary.clone());
    }
    parts.join(" ")
}

fn unique_eval_dir() -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "codebase-semantic-codex-eval-{}-{nonce}",
        std::process::id()
    ))
}

struct TempEvalDir(PathBuf);

impl Drop for TempEvalDir {
    fn drop(&mut self) {
        let temp_root = std::env::temp_dir();
        if is_direct_child_with_prefix(&temp_root, &self.0, "codebase-semantic-codex-eval-") {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}

fn is_direct_child_with_prefix(root: &Path, candidate: &Path, prefix: &str) -> bool {
    candidate.parent() == Some(root)
        && candidate
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with(prefix))
}
