use super::{
    broker, emit_progress,
    recovery::{
        compile_recovery_prompt, continue_recovery_prompt, run_provider_with_repair,
        verify_provider_result,
    },
    SemanticProgress,
};
use crate::provider::ResolvedProvider;
use codebase_semantic_compiler::{
    compile_reconciliation_partition, compile_reconciliation_prompt, CompiledBasePrompt,
    CompiledSemanticPartition, SemanticCompileError, VerifiedSemanticPartition,
};

const MAX_REDUCE_FAN_IN: usize = 4;

pub(super) fn reconcile_hierarchically(
    runtime: &ResolvedProvider,
    base: &CompiledBasePrompt,
    mut inputs: Vec<VerifiedSemanticPartition>,
    operation_id: &str,
    progress: SemanticProgress<'_>,
    completed_map_jobs: u64,
    total_progress_steps: u64,
) -> Result<codebase_semantic_model::ApprovedSemanticRevision, String> {
    let execution = ReduceExecutionContext {
        runtime,
        operation_id,
        progress,
        completed_map_jobs,
        total_progress_steps,
    };
    let mut level = 1usize;
    loop {
        if inputs.len() <= MAX_REDUCE_FAN_IN {
            match compile_reconciliation_prompt(base, &inputs) {
                Ok(final_prompt) => {
                    emit_progress(
                        progress,
                        "분할 결과를 하나의 전체 지도로 최종 통합하는 중",
                        completed_map_jobs,
                        total_progress_steps,
                    );
                    let revision = run_provider_with_repair(
                        runtime,
                        &final_prompt,
                        operation_id,
                        "AI 의미 전역 통합 결과",
                    )?;
                    emit_progress(
                        progress,
                        "전체 의미 지도 검증 완료",
                        total_progress_steps,
                        total_progress_steps,
                    );
                    return Ok(revision);
                }
                Err(error) if error.path == "reconciliationPrompt" && inputs.len() > 1 => {}
                Err(error) => return Err(reconciliation_error("최종 통합", error)),
            }
        }

        let plans = plan_reduce_layer(base, &inputs)?;
        if plans.len() >= inputs.len() {
            return Err(format!(
                "AI 의미 전역 통합 입력을 줄이지 못했습니다: {}개 입력이 {}개로 남았습니다",
                inputs.len(),
                plans.len()
            ));
        }
        emit_progress(
            progress,
            &format!(
                "전역 의미 지도 {level}단계 병렬 통합 중 · {}개 → {}개",
                inputs.len(),
                plans.len()
            ),
            completed_map_jobs,
            total_progress_steps,
        );
        inputs = execute_reduce_layer(&execution, plans, level)?;
        level += 1;
    }
}

struct ReduceExecutionContext<'a> {
    runtime: &'a ResolvedProvider,
    operation_id: &'a str,
    progress: SemanticProgress<'a>,
    completed_map_jobs: u64,
    total_progress_steps: u64,
}

enum ReducePlan {
    Carry(Box<VerifiedSemanticPartition>),
    Execute(Box<CompiledSemanticPartition>),
}

fn plan_reduce_layer(
    base: &CompiledBasePrompt,
    inputs: &[VerifiedSemanticPartition],
) -> Result<Vec<ReducePlan>, String> {
    let mut plans = Vec::new();
    for chunk in inputs.chunks(MAX_REDUCE_FAN_IN) {
        plan_reduce_chunk(base, chunk, &mut plans)?;
    }
    Ok(plans)
}

fn plan_reduce_chunk(
    base: &CompiledBasePrompt,
    inputs: &[VerifiedSemanticPartition],
    plans: &mut Vec<ReducePlan>,
) -> Result<(), String> {
    if inputs.len() == 1 {
        plans.push(ReducePlan::Carry(Box::new(inputs[0].clone())));
        return Ok(());
    }
    match compile_reconciliation_partition(base, inputs) {
        Ok(compiled) => {
            plans.push(ReducePlan::Execute(Box::new(compiled)));
            Ok(())
        }
        Err(error) if error.path == "reconciliationPrompt" && inputs.len() > 2 => {
            let midpoint = inputs.len() / 2;
            plan_reduce_chunk(base, &inputs[..midpoint], plans)?;
            plan_reduce_chunk(base, &inputs[midpoint..], plans)
        }
        Err(error) => Err(reconciliation_error("중간 통합 계획", error)),
    }
}

fn execute_reduce_layer(
    context: &ReduceExecutionContext<'_>,
    plans: Vec<ReducePlan>,
    level: usize,
) -> Result<Vec<VerifiedSemanticPartition>, String> {
    let mut outputs = vec![None; plans.len()];
    let mut jobs = Vec::new();
    let mut output_indexes = Vec::new();
    for (output_index, plan) in plans.into_iter().enumerate() {
        match plan {
            ReducePlan::Carry(verified) => outputs[output_index] = Some(*verified),
            ReducePlan::Execute(compiled) => {
                output_indexes.push(output_index);
                jobs.push(compiled);
            }
        }
    }

    let prompts = jobs
        .iter()
        .map(|job| job.prompt.clone())
        .collect::<Vec<_>>();
    let mut recovery_requests = Vec::new();
    let mut errors = Vec::new();
    let mut completed = 0usize;
    broker::run_provider_reduce_batch(
        context.runtime,
        &prompts,
        context.operation_id,
        |job_index, raw| {
            completed += 1;
            match verify_provider_result(&prompts[job_index], raw) {
                Ok(revision) => {
                    outputs[output_indexes[job_index]] =
                        Some(verified_reduction(&jobs[job_index], revision));
                }
                Err(failure) => {
                    match compile_recovery_prompt(job_index, &prompts[job_index], failure) {
                        Ok(request) => recovery_requests.push(request),
                        Err(error) => errors.push(reduce_job_error(level, job_index, &error)),
                    }
                }
            }
            emit_progress(
                context.progress,
                &format!(
                    "전역 의미 지도 {level}단계 병렬 통합 {completed}/{}",
                    prompts.len()
                ),
                context.completed_map_jobs,
                context.total_progress_steps,
            );
        },
    );

    let mut recovery_round = 0usize;
    while !recovery_requests.is_empty() {
        recovery_round += 1;
        let recovery_prompts = recovery_requests
            .iter()
            .map(|request| request.prompt.clone())
            .collect::<Vec<_>>();
        let mut next_requests = Vec::new();
        broker::run_provider_repair_batch(
            context.runtime,
            &recovery_prompts,
            context.operation_id,
            |request_index, raw| {
                let request = &recovery_requests[request_index];
                let job_index = request.partition_index;
                match verify_provider_result(&recovery_prompts[request_index], raw) {
                    Ok(revision) => {
                        outputs[output_indexes[job_index]] =
                            Some(verified_reduction(&jobs[job_index], revision));
                    }
                    Err(failure) => {
                        match continue_recovery_prompt(&jobs[job_index].prompt, request, failure) {
                            Ok(next) => next_requests.push(next),
                            Err(error) => errors.push(reduce_job_error(level, job_index, &error)),
                        }
                    }
                }
                emit_progress(
                    context.progress,
                    &format!(
                        "전역 의미 지도 {level}단계 {recovery_round}차 결과 교정 {}/{}",
                        request_index + 1,
                        recovery_prompts.len()
                    ),
                    context.completed_map_jobs,
                    context.total_progress_steps,
                );
            },
        );
        recovery_requests = next_requests;
    }

    if !errors.is_empty() {
        return Err(errors.join(" | "));
    }
    outputs
        .into_iter()
        .enumerate()
        .map(|(index, output)| {
            output.ok_or_else(|| {
                format!("전역 의미 지도 {level}단계 출력 {index}가 생성되지 않았습니다")
            })
        })
        .collect()
}

fn verified_reduction(
    job: &CompiledSemanticPartition,
    revision: codebase_semantic_model::ApprovedSemanticRevision,
) -> VerifiedSemanticPartition {
    VerifiedSemanticPartition {
        partition_key: job.partition_key.clone(),
        region_ids: job.region_ids.clone(),
        packet_digest: job.prompt.packet.semantic_input_digest,
        revision,
    }
}

fn reconciliation_error(context: &str, error: SemanticCompileError) -> String {
    format!("AI 의미 {context} 입력을 만들지 못했습니다: {error}")
}

fn reduce_job_error(level: usize, job_index: usize, error: &str) -> String {
    format!("AI 의미 {level}단계 통합 {job_index} 실패: {error}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{fact_graph, workspace};
    use codebase_semantic_compiler::compile_semantic_plan;
    use std::{env, path::PathBuf};

    #[test]
    fn reduction_fan_in_is_bounded() {
        assert_eq!(MAX_REDUCE_FAN_IN, 4);
    }

    #[test]
    #[ignore = "requires a real app-data workspace with verified semantic partition cache"]
    fn real_cached_partitions_compile_into_bounded_reduce_groups() {
        let app_data = PathBuf::from(
            env::var("CODEBASE_REDUCE_APP_DATA").expect("CODEBASE_REDUCE_APP_DATA is required"),
        );
        let workspace_id = env::var("CODEBASE_REDUCE_WORKSPACE_ID")
            .expect("CODEBASE_REDUCE_WORKSPACE_ID is required");
        let workspace = workspace::open_workspace(&app_data, &workspace_id).unwrap();
        let snapshot = fact_graph::load_published_snapshot(&app_data, &workspace.id)
            .unwrap()
            .unwrap();
        let plan = compile_semantic_plan(
            super::super::read_model::build_base_draft(&workspace, &snapshot).unwrap(),
        )
        .unwrap();
        let verified = plan
            .partitions
            .iter()
            .map(|partition| {
                super::super::store::load_partition(&app_data, &workspace.id, partition)
                    .unwrap()
                    .expect("every real partition must already be cached")
            })
            .collect::<Vec<_>>();

        let compact_full_bytes = compile_reconciliation_prompt(&plan.base, &verified)
            .unwrap()
            .rendered_prompt()
            .len();
        let plans = plan_reduce_layer(&plan.base, &verified).unwrap();
        let prompt_sizes = plans
            .iter()
            .filter_map(|plan| match plan {
                ReducePlan::Carry(_) => None,
                ReducePlan::Execute(compiled) => Some(compiled.prompt.rendered_prompt().len()),
            })
            .collect::<Vec<_>>();
        println!(
            "real reduce plan: inputs={} compact_full_bytes={compact_full_bytes} outputs={} prompt_bytes={prompt_sizes:?}",
            verified.len(),
            plans.len()
        );
        assert!(compact_full_bytes < 512 * 1024);
        assert!(plans.len() < verified.len());
        assert!(prompt_sizes.iter().all(|bytes| *bytes < 512 * 1024));
    }
}
