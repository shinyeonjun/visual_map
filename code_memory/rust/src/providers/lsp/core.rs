use scip::types::Index;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::AtomicI64;
use std::time::Duration;

use crate::{
    find_tool, prepare_clangd_compile_database, project_cache_root, provider_timeout, range_parts,
    range_span, tool_command, Diagnostic, DiagnosticCode, LanguageSpec, ProviderProcessGuard,
};

fn bundled_java_home(jdtls_path: &Path) -> Option<PathBuf> {
    let parent = jdtls_path.parent()?;
    let candidates = [parent.join("runtime"), parent.parent()?.join("runtime")];
    candidates
        .into_iter()
        .find(|candidate| candidate.join("bin").is_dir())
}

pub(crate) fn run_native_lsp(
    lang: &LanguageSpec,
    root: &Path,
    out: &Path,
    providers_root: Option<&Path>,
    files: &[PathBuf],
) -> Result<Vec<Diagnostic>, String> {
    let server = lang.tool;
    run_native_lsp_with_server(lang, server, root, out, providers_root, files)
}

pub(crate) fn run_native_lsp_source_only(
    lang: &LanguageSpec,
    root: &Path,
    out: &Path,
    providers_root: Option<&Path>,
    files: &[PathBuf],
) -> Result<Vec<Diagnostic>, String> {
    run_native_lsp_with_server_mode(lang, "jdtls", root, out, providers_root, files, true)
}

pub(crate) fn run_native_lsp_with_server(
    lang: &LanguageSpec,
    server: &str,
    root: &Path,
    out: &Path,
    providers_root: Option<&Path>,
    files: &[PathBuf],
) -> Result<Vec<Diagnostic>, String> {
    run_native_lsp_with_server_mode(lang, server, root, out, providers_root, files, false)
}
