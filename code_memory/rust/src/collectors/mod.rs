mod build_graph;
mod ci_evidence;
mod contracts;
mod database_assets;
mod deployment;
mod discovery;
mod frameworks;
mod messaging;
mod model;
mod revision;
mod telemetry;

use std::collections::HashSet;
use std::path::Path;

pub(crate) use model::CollectionReport;

pub(crate) fn collect_project(
    root: &Path,
    pack_root: &Path,
    providers_root: Option<&Path>,
) -> Result<CollectionReport, String> {
    let root = crate::source::canonical_project_root(root)?;
    let mut report = CollectionReport::new(root.to_string_lossy().into_owned());
    let snapshot = crate::source::load_source_snapshot(&root);
    report.push(build_graph::collect(&root, providers_root));
    report.push(ci_evidence::collect(&root));
    report.push(frameworks::collect(&root, pack_root, &snapshot));
    report.push(contracts::collect(&root));
    report.push(database_assets::collect(&root));
    report.push(deployment::collect(&root));
    report.push(messaging::collect(&snapshot));
    report.push(revision::collect(&root, providers_root));
    report.push(telemetry::collect(&root));
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
