use std::collections::HashSet;

use std::env;

use std::path::PathBuf;

use crate::{find_tool, LanguageJob, ProviderKind};

#[derive(Clone)]
pub(crate) struct ProviderJob {
    pub(crate) key: String,
    pub(crate) members: Vec<LanguageJob>,
}

pub(crate) fn merge_provider_jobs(jobs: Vec<LanguageJob>) -> Vec<ProviderJob> {
    let mut grouped: Vec<ProviderJob> = Vec::new();
    for job in jobs {
        let key = provider_job_key(&job);
        if let Some(existing) = grouped.iter_mut().find(|group| group.key == key) {
            existing.members.push(job);
        } else {
            grouped.push(ProviderJob {
                key,
                members: vec![job],
            });
        }
    }
    grouped
}

fn provider_job_key(job: &LanguageJob) -> String {
    let scope = job.module_id.replace([':', '\\', '/'], "_");
    if matches!(job.lang.provider, ProviderKind::Scip)
        && matches!(job.lang.tool, "scip-typescript" | "scip-clang")
        && find_tool(job.lang.tool, job.providers_root.as_deref()).is_some()
    {
        return format!("provider:{}:{}", job.lang.tool, scope);
    }
    if matches!(job.lang.id, "c" | "cpp")
        && find_tool("clangd", job.providers_root.as_deref()).is_some()
    {
        return format!("provider:clangd-c-cpp:{scope}");
    }
    format!("language:{}:{}", job.lang.id, scope)
}

pub(crate) fn combined_job_files(jobs: &[LanguageJob]) -> Vec<PathBuf> {
    let mut files = HashSet::new();
    for job in jobs {
        files.extend(job.files.iter().cloned());
    }
    let mut files: Vec<_> = files.into_iter().collect();
    files.sort();
    files
}

pub(crate) fn max_parallel_providers(job_count: usize) -> usize {
    let requested = env::var("CODE_MEMORY_MAX_PARALLEL")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| (1..=8).contains(value))
        .unwrap_or(3);
    requested.min(job_count.max(1))
}

pub(crate) fn max_provider_weight() -> usize {
    env::var("CODE_MEMORY_MAX_PROVIDER_WEIGHT")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| (2..=32).contains(value))
        .unwrap_or(4)
}

pub(crate) fn provider_job_weight(job: &ProviderJob) -> usize {
    if job.members.iter().any(|member| member.lang.id == "dart") {
        // Dart analysis_server keeps workspace state outside the LSP session;
        // concurrent sessions for one repository can contend on that state.
        return 4;
    }
    if job.members.iter().any(|member| member.lang.id == "rust") {
        // rust-analyzer can still be reloading the Cargo graph while another
        // provider saturates the same repository. Keep its workspace session
        // isolated; this avoids empty semantic results from shutdown races.
        return 4;
    }
    if job.members.iter().any(|member| {
        matches!(
            member.lang.id,
            "go" | "rust" | "java" | "dart" | "ruby" | "c" | "cpp"
        )
    }) {
        2
    } else {
        1
    }
}
