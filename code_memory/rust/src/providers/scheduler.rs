use std::collections::HashSet;
use std::env;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use crate::{
    analyze_provider_job, find_tool, language_failure, LanguageAnalysis, LanguageJob, ProviderKind,
};

#[derive(Clone)]
pub(crate) struct ProviderJob {
    pub(crate) key: String,
    pub(crate) members: Vec<LanguageJob>,
}

pub(crate) struct ProviderUnitResult {
    pub(crate) language: String,
    pub(crate) id: String,
    pub(crate) provider: &'static str,
}

pub(crate) struct ProviderJobResult {
    pub(crate) units: Vec<ProviderUnitResult>,
    pub(crate) analyses: Vec<LanguageAnalysis>,
    pub(crate) elapsed_ms: u128,
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
    let detected = std::thread::available_parallelism()
        .map(|parallelism| parallelism.get())
        .unwrap_or(1);
    let requested = env::var("CODE_MEMORY_MAX_PARALLEL")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| (1..=8).contains(value))
        .unwrap_or_else(|| default_provider_parallelism(detected, job_count));
    requested.min(job_count.max(1))
}

pub(crate) fn max_provider_weight() -> usize {
    let detected = std::thread::available_parallelism()
        .map(|parallelism| parallelism.get())
        .unwrap_or(1);
    env::var("CODE_MEMORY_MAX_PROVIDER_WEIGHT")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| (2..=32).contains(value))
        .unwrap_or_else(|| detected.clamp(2, 4))
}

pub(crate) fn provider_job_weight(job: &ProviderJob, max_weight: usize) -> usize {
    let provider_weight = if job.members.iter().any(|member| member.lang.id == "dart") {
        // Dart analysis_server keeps workspace state outside the LSP session;
        // concurrent sessions for one repository can contend on that state.
        4
    } else if job.members.iter().any(|member| member.lang.id == "rust") {
        // rust-analyzer can still be reloading the Cargo graph while another
        // provider saturates the same repository. Keep its workspace session
        // isolated; this avoids empty semantic results from shutdown races.
        4
    } else if job.members.iter().any(|member| {
        matches!(
            member.lang.id,
            "go" | "rust" | "java" | "dart" | "ruby" | "c" | "cpp"
        )
    }) {
        2
    } else {
        1
    };
    let largest_unit = job
        .members
        .iter()
        .map(|member| member.files.len())
        .max()
        .unwrap_or(0);
    provider_weight.max(file_pressure_weight(largest_unit, max_weight))
}

pub(crate) fn run_provider_jobs(
    jobs: Vec<ProviderJob>,
    heartbeat: impl Fn(usize, usize),
) -> Result<Vec<ProviderJobResult>, String> {
    let max_parallel = max_parallel_providers(jobs.len());
    let max_weight = max_provider_weight();
    eprintln!(
        "scheduler providers jobs={} max_parallel={} max_weight={}",
        jobs.len(),
        max_parallel,
        max_weight
    );
    let (sender, receiver) = mpsc::channel();
    let mut next_job = 0usize;
    let mut active_jobs = 0usize;
    let mut active_weight = 0usize;
    let total = jobs.len();
    let mut completed = 0usize;
    let mut results = Vec::with_capacity(total);

    while next_job < jobs.len() || active_jobs > 0 {
        while next_job < jobs.len() && active_jobs < max_parallel {
            let job = jobs[next_job].clone();
            let weight = provider_job_weight(&job, max_weight);
            if !can_start(active_jobs, active_weight, weight, max_parallel, max_weight) {
                break;
            }
            let ordinal = next_job;
            next_job += 1;
            active_jobs += 1;
            active_weight += weight;
            let sender = sender.clone();
            std::thread::spawn(move || {
                let started = Instant::now();
                let units = job
                    .members
                    .iter()
                    .map(|member| ProviderUnitResult {
                        language: member.lang.id.to_string(),
                        id: member.module_id.clone(),
                        provider: provider_label(member),
                    })
                    .collect();
                let members = job.members.clone();
                let analyses = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    analyze_provider_job(job)
                }))
                .unwrap_or_else(|panic| {
                    let message = panic_message(panic);
                    members
                        .iter()
                        .map(|member| {
                            language_failure(
                                member.lang,
                                provider_label(member),
                                &member.files,
                                format!("provider worker panicked: {message}"),
                            )
                        })
                        .collect()
                });
                let _ = sender.send((
                    ordinal,
                    weight,
                    ProviderJobResult {
                        units,
                        analyses,
                        elapsed_ms: started.elapsed().as_millis(),
                    },
                ));
            });
        }

        let (ordinal, weight, result) = loop {
            match receiver.recv_timeout(Duration::from_secs(5)) {
                Ok(result) => break result,
                Err(mpsc::RecvTimeoutError::Timeout) => heartbeat(completed, total),
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err("language workers stopped unexpectedly".to_string())
                }
            }
        };
        active_jobs -= 1;
        active_weight = active_weight.saturating_sub(weight);
        completed += 1;
        results.push((ordinal, result));
        heartbeat(completed, total);
    }
    drop(sender);
    results.sort_unstable_by_key(|(ordinal, _)| *ordinal);
    Ok(results.into_iter().map(|(_, result)| result).collect())
}

fn default_provider_parallelism(detected: usize, job_count: usize) -> usize {
    detected.saturating_sub(1).clamp(1, 4).min(job_count.max(1))
}

fn file_pressure_weight(file_count: usize, max_weight: usize) -> usize {
    file_count.div_ceil(2_000).clamp(1, max_weight.max(1))
}

fn can_start(
    active_jobs: usize,
    active_weight: usize,
    next_weight: usize,
    max_parallel: usize,
    max_weight: usize,
) -> bool {
    active_jobs < max_parallel && (active_jobs == 0 || active_weight + next_weight <= max_weight)
}

fn provider_label(job: &LanguageJob) -> &'static str {
    if matches!(job.lang.id, "c" | "cpp")
        && find_tool(job.lang.tool, job.providers_root.as_deref()).is_none()
    {
        "native-lsp"
    } else {
        match job.lang.provider {
            ProviderKind::Scip => "scip",
            ProviderKind::Lsp => "native-lsp",
        }
    }
}

fn panic_message(panic: Box<dyn std::any::Any + Send>) -> String {
    panic
        .downcast_ref::<&str>()
        .map(|message| (*message).to_string())
        .or_else(|| panic.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "unknown panic".to_string())
}

#[cfg(test)]
mod tests {
    use super::{can_start, default_provider_parallelism, file_pressure_weight};

    #[test]
    fn weighted_scheduler_runs_one_oversized_job_but_never_overcommits_active_jobs() {
        assert!(can_start(0, 0, 8, 3, 4));
        assert!(can_start(1, 1, 2, 3, 4));
        assert!(!can_start(1, 3, 2, 3, 4));
        assert!(!can_start(3, 3, 1, 3, 4));
    }

    #[test]
    fn defaults_follow_cpu_and_unit_pressure_without_project_size_modes() {
        assert_eq!(default_provider_parallelism(1, 20), 1);
        assert_eq!(default_provider_parallelism(8, 20), 4);
        assert_eq!(default_provider_parallelism(8, 2), 2);
        assert_eq!(file_pressure_weight(100, 4), 1);
        assert_eq!(file_pressure_weight(2_001, 4), 2);
        assert_eq!(file_pressure_weight(20_000, 4), 4);
    }
}
