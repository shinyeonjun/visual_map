//! AI semantic compilation built strictly on the published canonical snapshot.

mod broker;
mod map_view;
mod read_model;
mod reconcile;
mod recovery;
mod store;

use crate::{
    analysis::AnalysisCachePolicy,
    fact_graph,
    provider::{AiProviderKind as RuntimeProviderKind, ProviderRegistry, ResolvedProvider},
    workspace::Workspace,
};
use codebase_semantic_compiler::{
    compile_semantic_plan, CompiledSemanticPlan, VerifiedSemanticPartition,
};
use recovery::{
    compile_recovery_prompt, continue_recovery_prompt, run_provider_with_repair,
    verify_provider_result,
};
use std::path::Path;

type SemanticProgress<'a> = Option<&'a dyn Fn(&str, u64, u64)>;

pub(crate) fn analyze_and_publish(
    app_data_dir: &Path,
    workspace: &Workspace,
    providers: &ProviderRegistry,
    operation_id: &str,
    cache_policy: AnalysisCachePolicy,
    progress: SemanticProgress<'_>,
) -> Result<String, String> {
    let fact_reader = fact_graph::open_published_read_model(app_data_dir, &workspace.id)?
        .ok_or_else(|| "게시된 canonical Fact snapshot이 없습니다".to_string())?;
    let plan = compile_semantic_plan(read_model::build_base_draft(workspace, &fact_reader)?)
        .map_err(|error| format!("AI 의미 입력을 만들지 못했습니다: {error}"))?;
    eprintln!(
        "@codebase-workspace-ai-plan {}",
        serde_json::json!({
            "regions": plan.base.packet.input.regions.len(),
            "inputBytes": plan.base.rendered_prompt().len(),
            "partitions": plan.partitions.len(),
            "mode": if plan.is_direct() { "direct" } else { "partitioned-compact-global-reconciliation" },
        })
    );
    if cache_policy.reuses_results() {
        if let Some(current) = store::load_current(app_data_dir, &workspace.id)? {
            if current.packet == plan.base.packet {
                return Ok(current.revision.revision_id.to_string());
            }
        }
    }
    let runtime_kind = match plan.base.packet.provider.kind {
        codebase_semantic_model::AiProviderKind::Codex => RuntimeProviderKind::Codex,
        codebase_semantic_model::AiProviderKind::Claude => RuntimeProviderKind::Claude,
    };
    // Resolve once at the analysis boundary. Every local partition, repair,
    // execution retry, and reconciliation uses this exact CLI snapshot.
    let runtime = providers.resolve(runtime_kind)?;
    let approved = if plan.is_direct() {
        emit_progress(progress, "AI 의미 지도를 분석하는 중", 0, 1);
        let result = run_provider_with_repair(&runtime, &plan.base, operation_id, "AI 의미 결과")?;
        emit_progress(progress, "AI 의미 지도 검증 완료", 1, 1);
        result
    } else {
        analyze_partitioned(
            app_data_dir,
            workspace,
            &runtime,
            &plan,
            operation_id,
            cache_policy,
            progress,
        )?
    };
    let revision_id = approved.revision_id.to_string();
    store::publish(app_data_dir, &workspace.id, &plan.base.packet, &approved)?;
    Ok(revision_id)
}

fn analyze_partitioned(
    app_data_dir: &Path,
    workspace: &Workspace,
    runtime: &ResolvedProvider,
    plan: &CompiledSemanticPlan,
    operation_id: &str,
    cache_policy: AnalysisCachePolicy,
    progress: SemanticProgress<'_>,
) -> Result<codebase_semantic_model::ApprovedSemanticRevision, String> {
    let total = plan.partitions.len() as u64 + 1;
    emit_progress(
        progress,
        &format!(
            "코드 영역을 {}개 독립 작업으로 나눠 의미 분석 중",
            plan.partitions.len()
        ),
        0,
        total,
    );
    let mut verified = vec![None; plan.partitions.len()];
    let mut pending_indexes = Vec::new();
    let mut pending_prompts = Vec::new();
    for (index, partition) in plan.partitions.iter().enumerate() {
        let cached = if cache_policy.reuses_results() {
            store::load_partition(app_data_dir, &workspace.id, partition)?
        } else {
            None
        };
        if let Some(cached) = cached {
            verified[index] = Some(cached);
            emit_progress(
                progress,
                "검증된 의미 분석 결과를 재사용하는 중",
                verified.iter().filter(|result| result.is_some()).count() as u64,
                total,
            );
        } else {
            pending_indexes.push(index);
            pending_prompts.push(partition.prompt.clone());
        }
    }

    let mut recovery_requests = Vec::new();
    let mut fatal_errors = Vec::new();
    let mut initial_completed = 0usize;
    broker::run_provider_batch(
        runtime,
        &pending_prompts,
        operation_id,
        |batch_index, raw| {
            initial_completed += 1;
            let index = pending_indexes[batch_index];
            let prompt = &pending_prompts[batch_index];
            match verify_provider_result(prompt, raw) {
                Ok(revision) => {
                    if let Err(error) = accept_verified_partition(
                        app_data_dir,
                        workspace,
                        plan,
                        index,
                        revision,
                        cache_policy,
                        &mut verified,
                    ) {
                        fatal_errors.push(error);
                    }
                }
                Err(failure) => match compile_recovery_prompt(index, prompt, failure) {
                    Ok(request) => recovery_requests.push(request),
                    Err(error) => fatal_errors.push(partition_error(plan, index, &error)),
                },
            }
            let approved_count = verified.iter().flatten().count();
            emit_progress(
                progress,
                &format!(
                    "분할 의미 분석 {initial_completed}/{} 처리 · 검증 {approved_count}/{}",
                    pending_prompts.len(),
                    plan.partitions.len()
                ),
                approved_count as u64,
                total,
            );
        },
    );

    // Invalid JSON results are repaired with the original output plus exact
    // verifier feedback. Only execution failures without a result rerun the
    // original analysis prompt. First-pass successes stay verified and cached.
    let mut recovery_round = 0usize;
    while !recovery_requests.is_empty() {
        recovery_round += 1;
        let recovery_prompts = recovery_requests
            .iter()
            .map(|request| request.prompt.clone())
            .collect::<Vec<_>>();
        let mut next_recovery_requests = Vec::new();
        let mut recovery_completed = 0usize;
        broker::run_provider_repair_batch(
            runtime,
            &recovery_prompts,
            operation_id,
            |recovery_index, raw| {
                recovery_completed += 1;
                let request = &recovery_requests[recovery_index];
                match verify_provider_result(&recovery_prompts[recovery_index], raw) {
                    Ok(revision) => {
                        if let Err(error) = accept_verified_partition(
                            app_data_dir,
                            workspace,
                            plan,
                            request.partition_index,
                            revision,
                            cache_policy,
                            &mut verified,
                        ) {
                            fatal_errors.push(error);
                        }
                    }
                    Err(recovery_error) => match continue_recovery_prompt(
                        &plan.partitions[request.partition_index].prompt,
                        request,
                        recovery_error,
                    ) {
                        Ok(next) => next_recovery_requests.push(next),
                        Err(error) => fatal_errors.push(partition_error(
                            plan,
                            request.partition_index,
                            &error,
                        )),
                    },
                }
                let approved_count = verified.iter().flatten().count();
                emit_progress(
                    progress,
                    &format!(
                        "AI 결과 {recovery_round}차 교정 및 실행 복구 {recovery_completed}/{} · 검증 {approved_count}/{}",
                        recovery_prompts.len(),
                        plan.partitions.len()
                    ),
                    approved_count as u64,
                    total,
                );
            },
        );
        recovery_requests = next_recovery_requests;
    }

    if !fatal_errors.is_empty() {
        return Err(summarize_partition_errors(&fatal_errors));
    }

    let verified = verified
        .into_iter()
        .enumerate()
        .map(|(index, result)| {
            result.ok_or_else(|| partition_error(plan, index, "검증된 결과가 생성되지 않았습니다"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    reconcile::reconcile_globally(
        runtime,
        &plan.base,
        &verified,
        operation_id,
        progress,
        plan.partitions.len() as u64,
        total,
    )
}

fn accept_verified_partition(
    app_data_dir: &Path,
    workspace: &Workspace,
    plan: &CompiledSemanticPlan,
    index: usize,
    revision: codebase_semantic_model::ApprovedSemanticRevision,
    cache_policy: AnalysisCachePolicy,
    verified: &mut [Option<VerifiedSemanticPartition>],
) -> Result<(), String> {
    let partition = &plan.partitions[index];
    let result = VerifiedSemanticPartition {
        partition_key: partition.partition_key.clone(),
        region_ids: partition.region_ids.clone(),
        packet_digest: partition.prompt.packet.semantic_input_digest,
        revision,
    };
    // Keep the in-memory verified result even if the cache write fails. The
    // storage error is still reported, but no valid provider work is confused
    // with an invalid AI answer.
    verified[index] = Some(result.clone());
    store::cache_partition(
        app_data_dir,
        &workspace.id,
        partition,
        &result,
        cache_policy,
    )
    .map_err(|error| partition_error(plan, index, &format!("검증 결과 저장 실패: {error}")))
}

fn summarize_partition_errors(errors: &[String]) -> String {
    const MAX_DETAILS: usize = 3;
    let mut summary = errors
        .iter()
        .take(MAX_DETAILS)
        .cloned()
        .collect::<Vec<_>>()
        .join(" | ");
    if errors.len() > MAX_DETAILS {
        summary.push_str(&format!(" | 그 외 {}개 실패", errors.len() - MAX_DETAILS));
    }
    format!(
        "AI 의미 분할 {}개를 검증하지 못했습니다. 성공한 분할은 저장되어 다음 분석에서 재사용됩니다: {summary}",
        errors.len()
    )
}

fn emit_progress(progress: SemanticProgress<'_>, label: &str, completed: u64, total: u64) {
    if let Some(progress) = progress {
        progress(label, completed, total.max(1));
    }
}

fn partition_error(plan: &CompiledSemanticPlan, index: usize, detail: &str) -> String {
    let partition = &plan.partitions[index];
    let labels = partition
        .region_ids
        .iter()
        .filter_map(|region_id| {
            plan.base
                .packet
                .input
                .regions
                .iter()
                .find(|region| &region.region_id == region_id)
                .map(|region| region.structural_label.as_str())
        })
        .take(3)
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "AI 의미 분할 {}/{} ({labels}) 실패: {detail}",
        index + 1,
        plan.partitions.len()
    )
}

pub(crate) use map_view::{get_map_selection, get_map_view, MapSelection, MapView};

#[cfg(test)]
mod external_tests {
    use super::*;
    use crate::{
        fact_graph::CanonicalFactBundleArtifact,
        workspace::{WorkspaceProvider, WorkspaceProviderKind, WorkspaceReasoningEffort},
    };
    use codebase_fact_model::fact_graph::FactBundleManifest;
    use std::{env, fs, path::PathBuf};

    /// Opt-in vertical-slice gate. The normal suite stays offline, while a
    /// release/dev gate can prove canonical SQLite -> prompt -> provider ->
    /// verified semantic store -> UI read model with one real repository.
    #[test]
    #[ignore = "requires a canonical fixture plus an authenticated AI provider"]
    fn real_canonical_snapshot_reaches_the_map_read_model() {
        let repo = PathBuf::from(env::var("CODEBASE_WORKSPACE_E2E_REPO").unwrap());
        let manifest_path = PathBuf::from(env::var("CODEBASE_WORKSPACE_E2E_MANIFEST").unwrap());
        let manifest: FactBundleManifest =
            serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
        let bundle_path = manifest_path
            .parent()
            .unwrap()
            .join(format!("canonical-{}.sqlite", manifest.bundle_digest));
        let app_data = std::env::temp_dir().join(format!(
            "codebase-workspace-vertical-slice-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&app_data);
        fs::create_dir_all(
            crate::workspace::workspace_data_dir(&app_data, manifest.workspace_id.as_str())
                .unwrap(),
        )
        .unwrap();
        let workspace = Workspace {
            schema_version: 2,
            id: manifest.workspace_id.to_string(),
            name: "vertical slice fixture".to_string(),
            repo_path: repo.canonicalize().unwrap().display().to_string(),
            provider: Some(WorkspaceProvider {
                kind: WorkspaceProviderKind::Codex,
                model: env::var("CODEBASE_WORKSPACE_E2E_MODEL")
                    .unwrap_or_else(|_| "gpt-5.6-sol".to_string()),
                effort: WorkspaceReasoningEffort::High,
            }),
            created_at: 1,
            updated_at: 1,
        };
        let artifact = CanonicalFactBundleArtifact {
            schema: "codebase-workspace.canonical-fact-bundle-artifact.v1".to_string(),
            snapshot_id: manifest.snapshot_id.clone(),
            semantic_digest: manifest.semantic_digest,
            bundle_digest: manifest.bundle_digest,
            bundle_path,
            manifest_path,
        };

        crate::fact_graph::import_and_publish(&app_data, &workspace.id, &artifact).unwrap();
        let providers = ProviderRegistry::discover();
        let revision_id = analyze_and_publish(
            &app_data,
            &workspace,
            &providers,
            "semantic-cache-test",
            AnalysisCachePolicy::Reuse,
            None,
        )
        .unwrap();
        // The same canonical Fact packet must reuse the verified semantic
        // revision instead of paying for and varying another AI call.
        let cached_revision_id = analyze_and_publish(
            &app_data,
            &workspace,
            &providers,
            "semantic-cache-test",
            AnalysisCachePolicy::Reuse,
            None,
        )
        .unwrap();
        let map = get_map_view(&app_data, &workspace).unwrap().unwrap();

        assert!(!revision_id.is_empty());
        assert_eq!(cached_revision_id, revision_id);
        assert!(!map.areas.is_empty());
        let selected_id = map.areas[0].id.clone();
        assert!(get_map_selection(&app_data, &workspace, &selected_id)
            .unwrap()
            .is_some());
        fs::remove_dir_all(app_data).unwrap();
    }
}
