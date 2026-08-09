mod build_graph;
mod contracts;
mod database_assets;
mod deployment;
mod discovery;
mod messaging;
mod model;
mod revision;

use std::collections::HashSet;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

pub(crate) use model::CollectionReport;
use model::{CollectionDiagnostic, CollectionMode, CollectionStatus, CollectorResult};

#[derive(Clone, Copy)]
enum CollectorTask {
    BuildGraph,
    Contracts,
    DatabaseAssets,
    Deployment,
    Messaging,
    Revision,
}

impl CollectorTask {
    const ALL: [Self; 6] = [
        Self::BuildGraph,
        Self::Contracts,
        Self::DatabaseAssets,
        Self::Deployment,
        Self::Messaging,
        Self::Revision,
    ];

    fn collect(
        self,
        root: &Path,
        providers_root: Option<&Path>,
        snapshot: &crate::SourceSnapshot,
    ) -> CollectorResult {
        match self {
            Self::BuildGraph => build_graph::collect(root, providers_root),
            Self::Contracts => contracts::collect(root),
            Self::DatabaseAssets => database_assets::collect(root),
            Self::Deployment => deployment::collect(root),
            Self::Messaging => messaging::collect(snapshot),
            Self::Revision => revision::collect(root, providers_root),
        }
    }

    fn failed(self, message: String) -> CollectorResult {
        let (id, capability, mode) = match self {
            Self::BuildGraph => ("build-graph", "build-graph", CollectionMode::Passive),
            Self::Contracts => ("contracts", "api-contracts", CollectionMode::Passive),
            Self::DatabaseAssets => (
                "database-assets",
                "orm-and-migrations",
                CollectionMode::Passive,
            ),
            Self::Deployment => ("deployment", "deployment-topology", CollectionMode::Passive),
            Self::Messaging => ("messaging", "message-flow", CollectionMode::Passive),
            Self::Revision => (
                "git-revision",
                "source-revision",
                CollectionMode::ToolAssisted,
            ),
        };
        let mut result = CollectorResult::new(id, capability, mode);
        result.summary.status = CollectionStatus::Failed;
        result.diagnostics.push(CollectionDiagnostic {
            collector: id,
            level: "error",
            code: "collector-panicked",
            message,
            path: None,
        });
        result
    }
}

pub(crate) fn collect_project(
    root: &Path,
    providers_root: Option<&Path>,
) -> Result<CollectionReport, String> {
    let root = crate::source::canonical_project_root(root)?;
    let mut report = CollectionReport::new(root.to_string_lossy().into_owned());
    let snapshot = crate::source::load_source_snapshot(&root);
    let workers = std::thread::available_parallelism()
        .map(|parallelism| parallelism.get())
        .unwrap_or(1)
        .min(4);
    for result in bounded_map(CollectorTask::ALL.len(), workers, |ordinal| {
        let task = CollectorTask::ALL[ordinal];
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            task.collect(&root, providers_root, &snapshot)
        }))
        .unwrap_or_else(|payload| task.failed(panic_message(payload)))
    }) {
        report.push(result);
    }
    report.canonicalize();
    let keys: HashSet<&str> = report
        .facts
        .iter()
        .map(|fact| fact.stable_key.as_str())
        .collect();
    if let Some(relation) = report.relations.iter().find(|relation| {
        !keys.contains(relation.from.as_str()) || !keys.contains(relation.to.as_str())
    }) {
        return Err(format!(
            "collector invariant failed: dangling {} relation {} -> {}",
            relation.kind, relation.from, relation.to
        ));
    }
    Ok(report)
}

fn bounded_map<T: Send, F: Fn(usize) -> T + Sync>(
    task_count: usize,
    max_workers: usize,
    run: F,
) -> Vec<T> {
    if task_count == 0 {
        return Vec::new();
    }
    let next = AtomicUsize::new(0);
    let results = Mutex::new(Vec::with_capacity(task_count));
    std::thread::scope(|scope| {
        for _ in 0..max_workers.max(1).min(task_count) {
            scope.spawn(|| loop {
                let ordinal = next.fetch_add(1, Ordering::Relaxed);
                if ordinal >= task_count {
                    break;
                }
                let result = run(ordinal);
                results.lock().unwrap().push((ordinal, result));
            });
        }
    });
    let mut results = results.into_inner().unwrap();
    results.sort_unstable_by_key(|(ordinal, _)| *ordinal);
    results.into_iter().map(|(_, result)| result).collect()
}

fn panic_message(payload: Box<dyn std::any::Any + Send>) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_string()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "collector panicked without a message".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::bounded_map;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    #[test]
    fn bounded_map_limits_workers_and_preserves_task_order() {
        let active = AtomicUsize::new(0);
        let peak = AtomicUsize::new(0);
        let results = bounded_map(6, 2, |ordinal| {
            let current = active.fetch_add(1, Ordering::SeqCst) + 1;
            peak.fetch_max(current, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(5));
            active.fetch_sub(1, Ordering::SeqCst);
            ordinal
        });

        assert_eq!(results, (0..6).collect::<Vec<_>>());
        assert_eq!(peak.load(Ordering::SeqCst), 2);
    }
}
