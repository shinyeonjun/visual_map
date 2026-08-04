use serde::Serialize;
use serde_json::Value;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

mod architecture;
mod cache;
mod collectors;
mod frameworks;
mod model;
mod module_plan;
mod project_model;
mod providers;
mod source;
mod tool_api;
mod verification;
pub(crate) use cache::*;
pub(crate) use model::*;
pub(crate) use module_plan::*;
pub(crate) use providers::*;
#[cfg(test)]
pub(crate) use source::load_source_snapshot;
#[cfg(test)]
pub(crate) use source::load_source_snapshot_from_files;
pub(crate) use source::{
    collect_files, is_excluded_source_dir, load_source_contents,
    load_source_snapshot_metadata_from_files,
};

#[cfg(test)]
mod tests;

fn emit_progress(stage: &str, completed: usize, total: usize, label: &str) {
    eprintln!(
        "@visual-map-progress {}",
        serde_json::json!({
            "stage": stage,
            "completed": completed,
            "total": total.max(1),
            "label": label,
        })
    );
}

fn main() {
    if let Err(error) = run() {
        eprintln!("code-memory-language: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        Some("list") => list_languages(),
        Some("framework-packs") => {
            let rest: Vec<String> = args.collect();
            let root = optional_path(&rest, "--root")
                .unwrap_or(env::current_dir().map_err(|e| e.to_string())?);
            if rest.iter().any(|arg| arg == "--self-test") {
                frameworks::self_test(&root).map(|_| ())
            } else {
                validate_framework_packs(&root)
            }
        }
        Some("doctor") => {
            let rest: Vec<String> = args.collect();
            doctor(optional_path(&rest, "--providers-root").as_deref())
        }
        Some("index") => {
            let rest: Vec<String> = args.collect();
            let root = required_path(&rest, "--root")?;
            let pack_root = optional_path(&rest, "--packs-root")
                .unwrap_or(env::current_dir().map_err(|e| e.to_string())?);
            let providers_root = optional_path(&rest, "--providers-root");
            let out = optional_path(&rest, "--out")
                .unwrap_or_else(|| root.join(r".code_memory\language-index.json"));
            let out = if out.is_absolute() {
                out
            } else {
                env::current_dir()
                    .map_err(|e| format!("cannot resolve output path: {e}"))?
                    .join(out)
            };
            let architecture_out = optional_path(&rest, "--architecture-out")
                .map(resolve_output_path)
                .transpose()?
                .unwrap_or_else(|| default_architecture_output(&out));
            index_project(
                &root,
                &out,
                &architecture_out,
                &pack_root,
                providers_root.as_deref(),
            )
        }
        Some("collect") => {
            let rest: Vec<String> = args.collect();
            let root = required_path(&rest, "--root")?;
            let pack_root = optional_path(&rest, "--packs-root")
                .unwrap_or(env::current_dir().map_err(|e| e.to_string())?);
            let providers_root = optional_path(&rest, "--providers-root");
            let out = optional_path(&rest, "--out")
                .unwrap_or_else(|| root.join(r".code_memory\collection-report.json"));
            let out = resolve_output_path(out)?;
            if let Some(parent) = out.parent() {
                fs::create_dir_all(parent)
                    .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
            }
            let report = collectors::collect_project(&root, &pack_root, providers_root.as_deref())?;
            let file = fs::File::create(&out)
                .map_err(|error| format!("cannot write {}: {error}", out.display()))?;
            let mut writer = BufWriter::new(file);
            write_json(&mut writer, &report)
                .map_err(|error| format!("cannot serialize collection report: {error}"))?;
            writer
                .flush()
                .map_err(|error| format!("cannot flush {}: {error}", out.display()))?;
            println!("wrote {}", out.display());
            Ok(())
        }
        Some("verify") => {
            let rest: Vec<String> = args.collect();
            let root = required_path(&rest, "--root")?;
            let providers_root = optional_path(&rest, "--providers-root");
            let tool = required_value(&rest, "--tool")?;
            let label = optional_value(&rest, "--label").unwrap_or_else(|| tool.clone());
            let arguments = repeated_values(&rest, "--arg")?;
            let timeout = optional_value(&rest, "--timeout-seconds")
                .map(|value| {
                    value
                        .parse::<u64>()
                        .map_err(|_| "--timeout-seconds must be an integer".to_string())
                })
                .transpose()?
                .unwrap_or(600);
            if !(5..=3_600).contains(&timeout) {
                return Err("--timeout-seconds must be between 5 and 3600".to_string());
            }
            let out = optional_path(&rest, "--out")
                .unwrap_or_else(|| root.join(r".code_memory\evidence\verification-run.json"));
            let out = resolve_output_path(out)?;
            let execution = verification::run(
                &root,
                providers_root.as_deref(),
                &tool,
                &arguments,
                &label,
                Duration::from_secs(timeout),
            )?;
            if let Some(parent) = out.parent() {
                fs::create_dir_all(parent)
                    .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
            }
            let file = fs::File::create(&out)
                .map_err(|error| format!("cannot write {}: {error}", out.display()))?;
            let mut writer = BufWriter::new(file);
            write_json(&mut writer, &execution.report)
                .map_err(|error| format!("cannot serialize verification report: {error}"))?;
            writer
                .flush()
                .map_err(|error| format!("cannot flush {}: {error}", out.display()))?;
            io::stdout()
                .write_all(&execution.stdout)
                .map_err(|error| format!("cannot write verification stdout: {error}"))?;
            io::stderr()
                .write_all(&execution.stderr)
                .map_err(|error| format!("cannot write verification stderr: {error}"))?;
            println!("wrote {}", out.display());
            if execution.report.status == "passed" {
                Ok(())
            } else {
                Err(format!(
                    "verification {} {}",
                    execution.report.label, execution.report.status
                ))
            }
        }
        Some("cli") => tool_api::run_cli(&args.collect::<Vec<_>>()),
        Some(command) => Err(format!(
            "unknown command '{command}'. Use list, doctor, index, collect, or verify."
        )),
        None => Err("missing command. Use list, doctor, index, collect, or verify.".to_string()),
    }
}

include!("cli.rs");
include!("index.rs");
include!("analysis.rs");
include!("output.rs");
