use scip::types::Index;
use codebase_fact_model::analysis::{
    ProviderConfigUse, ProviderExecutionMode, ProviderProtocol,
};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::AtomicI64;
use std::time::{Duration, Instant};

use crate::{
    executed_provider_context, find_tool, generated_context_digest_from_files,
    inventory_call_sites, prepare_clangd_compile_database, project_cache_root, provider_timeout,
    range_parts, range_span, tool_command, workspace_context_files, CallSiteForm, Diagnostic,
    DiagnosticCode, ExecutedProviderContextInput, LanguageSpec, ProviderProcessGuard,
    ProviderRoots, ProviderRunOutcome, SyntaxCallSite,
};
use crate::static_pipeline::language_ir::type_relations::{
    inventory_type_syntax, SyntaxTypeInventory,
};
use crate::static_pipeline::context_dimensions::go_execution_environment;

fn bundled_java_home(jdtls_path: &Path) -> Option<PathBuf> {
    let parent = jdtls_path.parent()?;
    let candidates = [parent.join("runtime"), parent.parent()?.join("runtime")];
    candidates
        .into_iter()
        .find(|candidate| candidate.join("bin").is_dir())
}

pub(crate) fn run_native_lsp(
    lang: &LanguageSpec,
    roots: ProviderRoots<'_>,
    out: &Path,
    providers_root: Option<&Path>,
    files: &[PathBuf],
) -> Result<ProviderRunOutcome, String> {
    let server = lang.tool;
    run_native_lsp_with_server(lang, server, roots, out, providers_root, files)
}

pub(crate) fn run_native_lsp_source_only(
    lang: &LanguageSpec,
    roots: ProviderRoots<'_>,
    out: &Path,
    providers_root: Option<&Path>,
    files: &[PathBuf],
) -> Result<ProviderRunOutcome, String> {
    run_native_lsp_with_server_mode(
        lang,
        "jdtls",
        roots,
        out,
        providers_root,
        files,
        true,
    )
}

pub(crate) fn run_native_lsp_with_server(
    lang: &LanguageSpec,
    server: &str,
    roots: ProviderRoots<'_>,
    out: &Path,
    providers_root: Option<&Path>,
    files: &[PathBuf],
) -> Result<ProviderRunOutcome, String> {
    run_native_lsp_with_server_mode(
        lang,
        server,
        roots,
        out,
        providers_root,
        files,
        false,
    )
}
