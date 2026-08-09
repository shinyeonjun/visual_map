use serde::Serialize;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

const _: &str = codebase_fact_model::ContractSchema::LanguageIrV2.as_str();

mod cache;
mod frameworks;
mod model;
mod project_model;
mod provider_batch;
mod provider_compare;
mod providers;
mod source;
mod static_pipeline;
pub(crate) use cache::*;
pub(crate) use model::*;
pub(crate) use provider_batch::*;
pub(crate) use providers::*;
#[cfg(test)]
pub(crate) use source::load_source_snapshot;
#[cfg(test)]
pub(crate) use source::load_source_snapshot_from_files;
#[cfg(test)]
pub(crate) use source::load_source_snapshot_metadata_from_files;
pub(crate) use source::{collect_files, is_excluded_source_dir, load_source_contents};

#[cfg(test)]
mod tests;

fn emit_progress(stage: &str, completed: usize, total: usize, label: &str) {
    eprintln!(
        "@codebase-workspace-progress {}",
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
        Some("compare-scip") => {
            let rest: Vec<String> = args.collect();
            provider_compare::compare_scip(&rest)
        }
        Some("index") => {
            let rest: Vec<String> = args.collect();
            let root = required_path(&rest, "--root")?;
            let pack_root = optional_path(&rest, "--packs-root")
                .unwrap_or(env::current_dir().map_err(|e| e.to_string())?);
            let providers_root = optional_path(&rest, "--providers-root");
            if optional_path(&rest, "--out").is_some()
                || optional_path(&rest, "--architecture-out").is_some()
            {
                return Err(
                    "--out and --architecture-out were removed; index publishes one immutable canonical Fact bundle"
                        .to_string(),
                );
            }
            index_project(&root, &pack_root, providers_root.as_deref())
        }
        Some(command) => Err(format!(
            "unknown command '{command}'. Use list, doctor, compare-scip, or index."
        )),
        None => Err("missing command. Use list, doctor, compare-scip, or index.".to_string()),
    }
}

include!("cli.rs");
include!("provider_planning.rs");
include!("index.rs");
include!("publication.rs");
include!("analysis.rs");
