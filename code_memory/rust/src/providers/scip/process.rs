use scip::types::SymbolRole;
use serde_json::Value;
use protobuf::Message;
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::{
    capture_provider_stderr, collect_files, find_tool, javascript_workspace, project_cache_root,
    provider_timeout, range_contains, tool_command, typescript_config_files, Diagnostic,
    DiagnosticCode, DocumentOutput, LanguageSpec,
    OccurrenceOutput, ProviderProcessGuard, RelationOutput, SymbolOutput,
};

type SourceRange = (i32, i32, i32, i32);
type SymbolsByRange = HashMap<String, HashMap<SourceRange, String>>;
type CallRangesByPath = HashMap<String, HashSet<SourceRange>>;

pub(crate) fn ensure_default_scip_output(root: &Path, out: &Path) -> Result<(), String> {
    if out.is_file() {
        return Ok(());
    }
    let default = root.join("index.scip");
    if default.is_file() {
        fs::copy(&default, out).map_err(|e| {
            format!(
                "cannot copy {} to {}: {e}",
                default.display(),
                out.display()
            )
        })?;
        return Ok(());
    }
    Err(format!(
        "indexer completed but no SCIP output was found at {}",
        out.display()
    ))
}

pub(crate) fn run_command(
    mut command: Command,
    language: &str,
    provider: &str,
) -> Result<(), String> {
    let mut child = command
        .spawn()
        .map_err(|e| format!("{} indexer could not start: {e}", language))?;
    let process_guard = ProviderProcessGuard::attach(&child);
    let stderr_task = child
        .stderr
        .take()
        .map(|stderr| capture_provider_stderr(provider, stderr));
    let deadline = Instant::now() + provider_timeout();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                process_guard.terminate(&mut child);
                if status.success() {
                    let _ = stderr_task.map(|task| task.join());
                    return Ok(());
                }
                return Err(provider_failure(
                    language,
                    format!("{} indexer exited with {}", language, status),
                    stderr_task,
                ));
            }
            Ok(None) if Instant::now() >= deadline => {
                process_guard.terminate(&mut child);
                return Err(provider_failure(
                    language,
                    format!(
                        "{} indexer timeout after {} seconds",
                        language,
                        provider_timeout().as_secs()
                    ),
                    stderr_task,
                ));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(250)),
            Err(error) => {
                process_guard.terminate(&mut child);
                return Err(provider_failure(
                    language,
                    format!("{} indexer wait failed: {error}", language),
                    stderr_task,
                ));
            }
        }
    }
}

fn provider_failure(
    _language: &str,
    message: String,
    stderr_task: Option<std::thread::JoinHandle<String>>,
) -> String {
    let tail = stderr_task
        .and_then(|task| task.join().ok())
        .filter(|tail| !tail.is_empty());
    match tail {
        Some(tail) => format!("{message}; stderr_tail: {tail}"),
        None => message,
    }
}
