use std::collections::HashSet;
use std::env;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use crate::{
    analyze_provider_job, assign_provider_batch_scope, find_tool, language_failure, LanguageJob,
    ProviderKind, ProviderUnitBatch,
};

#[derive(Clone)]
pub(crate) struct ProviderJob {
    pub(crate) key: String,
    pub(crate) members: Vec<LanguageJob>,
}

pub(crate) struct ProviderJobResult {
    pub(crate) batches: Vec<ProviderUnitBatch>,
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
    let scope = job.execution_scope_id.replace([':', '\\', '/'], "_");
    if env::var("CODE_MEMORY_EXPERIMENTAL_TS_MULTI_CONFIG_BATCH").as_deref() == Ok("1")
        && job.lang.tool == "scip-typescript"
        && job.provider_config.is_some()
        && find_tool(job.lang.tool, job.providers_root.as_deref()).is_some()
    {
        // Experimental only: scip-typescript accepts several tsconfig/jsconfig
        // inputs in one process.  Do not enable this by default until the
        // canonical shadow gate is exact.  On ESLint it was 51% faster, but a
        // cross-root batch omitted CustomParserServices.program plus two
        // relations.  The established per-scope path remains the correctness
        // baseline.
        return format!(
            "provider:scip-typescript:configured:{}",
            stable_path_hash(&job.project_root)
        );
    }
    if matches!(job.lang.provider, ProviderKind::Scip)
        && matches!(job.lang.tool, "scip-typescript" | "scip-clang")
        && find_tool(job.lang.tool, job.providers_root.as_deref()).is_some()
    {
        return format!("provider:{}:{}", job.lang.tool, scope);
    }
    if matches!(job.lang.id, "c" | "cpp")
        && find_tool("clangd", job.providers_root.as_deref()).is_some()
    {
        return format!("provider:clangd:{}:{scope}", job.lang.id);
    }
    format!("language:{}:{}", job.lang.id, scope)
}

fn stable_path_hash(path: &std::path::Path) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in path.to_string_lossy().replace('\\', "/").bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
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

fn max_provider_weight(memory_budget_mb: Option<usize>) -> usize {
    let detected = std::thread::available_parallelism()
        .map(|parallelism| parallelism.get())
        .unwrap_or(1);
    let cpu_weight = env::var("CODE_MEMORY_MAX_PROVIDER_WEIGHT")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| (2..=32).contains(value))
        .unwrap_or_else(|| detected.clamp(2, 4));
    provider_weight_budget(cpu_weight, memory_budget_mb)
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
            "go" | "rust" | "java" | "dart" | "c" | "cpp"
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
    let memory_budget_mb = provider_memory_budget_mb();
    let max_weight = max_provider_weight(memory_budget_mb);
    eprintln!(
        "scheduler providers jobs={} max_parallel={} max_weight={} memory_budget_mb={}",
        jobs.len(),
        max_parallel,
        max_weight,
        memory_budget_mb
            .map(|value| value.to_string())
            .unwrap_or_else(|| "unknown".to_string())
    );
    let (sender, receiver) = mpsc::channel();
    let total = jobs.len();
    // Keep the original ordinal beside each pending job. Results are sorted by
    // this ordinal below, so choosing a later light job never changes the
    // deterministic output order. It only avoids leaving CPU and memory idle
    // while a heavy head-of-line job waits for enough weight budget.
    let mut pending = jobs
        .into_iter()
        .enumerate()
        .map(|(ordinal, job)| {
            let weight = provider_job_weight(&job, max_weight);
            Some((ordinal, job, weight))
        })
        .collect::<Vec<_>>();
    let mut pending_jobs = pending.len();
    let mut active_jobs = 0usize;
    let mut active_weight = 0usize;
    let mut completed = 0usize;
    let mut results = Vec::with_capacity(total);

    while pending_jobs > 0 || active_jobs > 0 {
        while active_jobs < max_parallel {
            let Some(pending_index) = next_startable_job_index(
                &pending,
                active_jobs,
                active_weight,
                max_parallel,
                max_weight,
            ) else {
                break;
            };
            let (ordinal, job, weight) = pending[pending_index]
                .take()
                .expect("startable provider job remained pending");
            pending_jobs -= 1;
            active_jobs += 1;
            active_weight += weight;
            let sender = sender.clone();
            std::thread::spawn(move || {
                let started = Instant::now();
                let job_key = job.key.clone();
                let file_count = combined_job_files(&job.members).len();
                let unit_count = job.members.len();
                let members = job.members.clone();
                let mut batches = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
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
                if batches.len() != members.len() {
                    let returned = batches.len();
                    batches = members
                        .iter()
                        .map(|member| {
                            language_failure(
                                member.lang,
                                provider_label(member),
                                &member.files,
                                format!(
                                    "provider worker returned {returned} unit batches for {} planned units",
                                    members.len()
                                ),
                            )
                        })
                        .collect();
                }
                for (member, batch) in members.iter().zip(&mut batches) {
                    assign_provider_batch_scope(batch, &member.project_root, &member.files);
                }
                let elapsed_ms = started.elapsed().as_millis();
                eprintln!(
                    "@codebase-workspace-provider-performance {}",
                    serde_json::json!({
                        "jobKey": job_key,
                        "weight": weight,
                        "units": unit_count,
                        "files": file_count,
                        "elapsedMs": elapsed_ms,
                    })
                );
                let _ = sender.send((ordinal, weight, ProviderJobResult { batches }));
            });
        }

        if active_jobs == 0 {
            return Err("provider scheduler could not admit a pending job".to_string());
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

const PROVIDER_MEMORY_TOKEN_MB: usize = 512;
const SYSTEM_MEMORY_RESERVE_MB: usize = 1_024;
const UNKNOWN_MEMORY_WEIGHT: usize = 2;

pub(crate) fn provider_memory_budget_mb() -> Option<usize> {
    env::var("CODE_MEMORY_MEMORY_BUDGET_MB")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| (PROVIDER_MEMORY_TOKEN_MB..=1_048_576).contains(value))
        .or_else(|| {
            available_memory_mb().map(|available| {
                available
                    .saturating_sub(SYSTEM_MEMORY_RESERVE_MB)
                    .max(PROVIDER_MEMORY_TOKEN_MB)
            })
        })
}

fn provider_weight_budget(cpu_weight: usize, memory_budget_mb: Option<usize>) -> usize {
    let memory_weight = memory_budget_mb
        .map(|memory| (memory / PROVIDER_MEMORY_TOKEN_MB).max(1))
        .unwrap_or(UNKNOWN_MEMORY_WEIGHT);
    cpu_weight.min(memory_weight).max(1)
}

#[cfg(windows)]
fn available_memory_mb() -> Option<usize> {
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};

    let mut status = MEMORYSTATUSEX {
        dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
        ..Default::default()
    };
    // SAFETY: status points to an initialized MEMORYSTATUSEX with the required size.
    (unsafe { GlobalMemoryStatusEx(&mut status) } != 0)
        .then_some((status.ullAvailPhys / 1_048_576) as usize)
}

#[cfg(target_os = "linux")]
fn available_memory_mb() -> Option<usize> {
    let meminfo = std::fs::read_to_string("/proc/meminfo").ok()?;
    meminfo.lines().find_map(|line| {
        let value = line.strip_prefix("MemAvailable:")?;
        value
            .split_whitespace()
            .next()?
            .parse::<usize>()
            .ok()
            .map(|kib| kib / 1_024)
    })
}

#[cfg(not(any(windows, target_os = "linux")))]
fn available_memory_mb() -> Option<usize> {
    None
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

fn next_startable_job_index(
    pending: &[Option<(usize, ProviderJob, usize)>],
    active_jobs: usize,
    active_weight: usize,
    max_parallel: usize,
    max_weight: usize,
) -> Option<usize> {
    pending.iter().position(|entry| {
        entry.as_ref().is_some_and(|(_, _, weight)| {
            can_start(
                active_jobs,
                active_weight,
                *weight,
                max_parallel,
                max_weight,
            )
        })
    })
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
    use super::{
        can_start, default_provider_parallelism, file_pressure_weight, next_startable_job_index,
        provider_weight_budget, ProviderJob,
    };

    #[test]
    fn weighted_scheduler_runs_one_oversized_job_but_never_overcommits_active_jobs() {
        assert!(can_start(0, 0, 8, 3, 4));
        assert!(can_start(1, 1, 2, 3, 4));
        assert!(!can_start(1, 3, 2, 3, 4));
        assert!(!can_start(3, 3, 1, 3, 4));
    }

    #[test]
    fn weighted_scheduler_skips_a_blocked_heavy_job_to_start_lighter_work() {
        let pending = vec![
            Some((
                0,
                ProviderJob {
                    key: "heavy".to_string(),
                    members: Vec::new(),
                },
                4,
            )),
            Some((
                1,
                ProviderJob {
                    key: "light".to_string(),
                    members: Vec::new(),
                },
                1,
            )),
        ];

        assert_eq!(next_startable_job_index(&pending, 1, 1, 4, 4), Some(1));
        assert_eq!(next_startable_job_index(&pending, 0, 0, 4, 4), Some(0));
    }

    #[test]
    fn defaults_follow_cpu_and_unit_pressure_without_project_size_modes() {
        assert_eq!(default_provider_parallelism(1, 20), 1);
        assert_eq!(default_provider_parallelism(8, 20), 4);
        assert_eq!(default_provider_parallelism(8, 2), 2);
        assert_eq!(file_pressure_weight(100, 4), 1);
        assert_eq!(file_pressure_weight(2_001, 4), 2);
        assert_eq!(file_pressure_weight(20_000, 4), 4);
        assert_eq!(provider_weight_budget(4, Some(512)), 1);
        assert_eq!(provider_weight_budget(4, Some(1_024)), 2);
        assert_eq!(provider_weight_budget(4, Some(8_192)), 4);
        assert_eq!(provider_weight_budget(4, None), 2);
    }

    #[cfg(windows)]
    #[test]
    fn windows_memory_admission_reads_available_physical_memory() {
        assert!(super::available_memory_mb().is_some_and(|memory| memory > 0));
    }
}
