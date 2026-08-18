mod output;

use code_analysis_engine::{analyze, config::AnalysisConfig, AnalysisRequest};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

fn main() -> ExitCode {
    // 엔진 프로세스가 시작된 직후부터 종료 직전까지의 벽시계 시간을 잰다.
    // Rust 분석 단계의 시간(result.elapsed_ms)과 설정·직렬화·파일 쓰기 시간을
    // 포함한 실제 엔진 실행 시간을 구분하기 위한 측정값이다.
    let process_started = Instant::now();
    let arguments: Vec<String> = env::args().skip(1).collect();
    let profile = arguments.iter().any(|argument| argument == "--profile");
    let exit_code = run_command(arguments);
    if profile {
        eprintln!(
            "[profile] process_total_elapsed_ms={} (engine_start_to_exit)",
            process_started.elapsed().as_millis()
        );
    }
    exit_code
}

fn run_command(arguments: Vec<String>) -> ExitCode {
    if arguments.first().map(String::as_str) == Some("eval") {
        if arguments.get(1).map(String::as_str) == Some("ab") {
            return run_eval_ab(&arguments[2..]);
        }
        if arguments.get(1).map(String::as_str) == Some("diagnose-pairs") {
            return run_eval_diagnose_pairs(&arguments[2..]);
        }
        if arguments.get(1).map(String::as_str) == Some("diagnose-gold-pairs") {
            return run_eval_diagnose_gold_pairs(&arguments[2..]);
        }
        if arguments.get(1).map(String::as_str) == Some("diagnose-domain-seeds") {
            return run_eval_diagnose_domain_seeds(&arguments[2..]);
        }
        if arguments.get(1).map(String::as_str) == Some("domain-recovery") {
            return run_eval_domain_recovery(&arguments[2..]);
        }
        return run_eval(&arguments[1..]);
    }
    if arguments.first().map(String::as_str) == Some("postprocess")
        && arguments.get(1).map(String::as_str) == Some("ai-context")
    {
        return run_ai_context_postprocess(&arguments[2..]);
    }
    if arguments.first().map(String::as_str) == Some("semantic")
        && arguments.get(1).map(String::as_str) == Some("review")
    {
        return run_semantic_review(&arguments[2..]);
    }
    if arguments.first().map(String::as_str) == Some("clean")
        && arguments.get(1).map(String::as_str) == Some("bundle")
    {
        return run_clean_bundle(&arguments[2..]);
    }

    let Some(root_path) = arguments.first() else {
        eprintln!("사용법: code-analysis-engine <프로젝트-경로> [--compact] [--profile] [--no-cache] [--no-output] [--output=<경로>] [--clean-output=<디렉터리>] [--config=<경로>]");
        eprintln!("       code-analysis-engine eval --gold=<정답.json> (--clean=<clean-디렉터리> | --overview=<분석결과.json> | --project=<프로젝트-경로>) [--output=<리포트.json>] [--config=<경로>]");
        eprintln!("       code-analysis-engine eval --catalog=<정답-디렉터리-또는-catalog.json> --clean-root=<clean-루트> [--output=<리포트.json>]");
        eprintln!("       code-analysis-engine eval ab --catalog=<catalog.json> [--experiment] [--output=<ab-report.json>]");
        eprintln!("       code-analysis-engine eval diagnose-pairs --catalog=<catalog.json> [--ids=<id1,id2>] [--mode=legacy|structural|both] [--top=20] [--output=<pair-diagnose.json>]");
        eprintln!("       code-analysis-engine eval diagnose-gold-pairs --catalog=<catalog.json> [--ids=<id1,id2>] [--mode=legacy|structural|both] [--evidence] [--output=<gold-pair-signals.json>]");
        eprintln!("       code-analysis-engine eval diagnose-domain-seeds --catalog=<catalog.json> [--ids=<id1,id2>] [--output=<domain-seed-diagnostics.json>]");
        eprintln!("       code-analysis-engine eval domain-recovery --catalog=<catalog.json> [--output=<domain-recovery-eval.json>]");
        eprintln!("       code-analysis-engine postprocess ai-context --input=<분석결과.json> --output=<컨텍스트.json> [--config=<경로>] [--pretty] [--profile]");
        eprintln!("       code-analysis-engine semantic review --input=<ai-context.json> --output=<semantic-result.json> --project-root=<프로젝트-경로> [--config=<경로>] [--model=<모델>] [--profile]");
        eprintln!("       code-analysis-engine clean bundle --input=<분석결과.json> --output=<clean-디렉터리> [--config=<경로>] [--part-target-bytes=<바이트>] [--profile]");
        return ExitCode::from(2);
    };

    let flags = &arguments[1..];
    let compact = flags.iter().any(|argument| argument == "--compact");
    let profile = flags.iter().any(|argument| argument == "--profile");
    // 대형 프로젝트의 프로파일 실행에서 수백 MB 이상의 JSON을 stdout에
    // 만들지 않도록 한다. 분석 Facts는 그대로 만들고 직렬화만 생략한다.
    let no_output = flags.iter().any(|argument| argument == "--no-output");
    let output_path = flags
        .iter()
        .find_map(|argument| argument.strip_prefix("--output="))
        .map(PathBuf::from);
    let clean_output_path = flags
        .iter()
        .find_map(|argument| argument.strip_prefix("--clean-output="))
        .map(PathBuf::from);
    let config_path = flags
        .iter()
        .find_map(|argument| argument.strip_prefix("--config="));

    let mut request = AnalysisRequest::new(root_path);
    if let Some(path) = config_path {
        match AnalysisConfig::from_file(std::path::Path::new(path)) {
            Ok(config) => request.options.config = config,
            Err(error) => {
                eprintln!("설정 적용 실패: {error}");
                return ExitCode::from(2);
            }
        }
    }
    request.options.profile = profile;
    request.options.use_fact_cache = !flags.iter().any(|argument| argument == "--no-cache");
    let max_output_bytes = request.options.config.limits.max_output_bytes;
    let clean_policy = request.options.config.clean.clone();

    match analyze(request) {
        Ok(result) => {
            if no_output {
                eprintln!("분석 결과 JSON 출력 생략 (--no-output)");
            } else if let Some(path) = output_path {
                if let Err(error) =
                    output::write_result_json(&path, &result, compact, max_output_bytes)
                {
                    eprintln!("결과 JSON 저장 실패: {error}");
                    return ExitCode::from(1);
                }
                eprintln!("분석 결과 저장 완료: {}", path.display());
            } else if let Err(error) =
                output::write_result_stdout(&result, compact, max_output_bytes)
            {
                eprintln!("결과 JSON 생성 실패: {error}");
                return ExitCode::from(1);
            }
            if let Some(path) = clean_output_path {
                let started = Instant::now();
                match code_analysis_engine::clean::write_from_result(
                    &result,
                    &path,
                    &clean_policy,
                ) {
                    Ok(manifest) => eprintln!(
                        "Clean bundle 저장 완료: domains={} features={} flows={} datasets={} elapsed_ms={} output={}",
                        dataset_count(&manifest, "domains"),
                        dataset_count(&manifest, "features"),
                        dataset_count(&manifest, "flows"),
                        manifest.datasets.len(),
                        started.elapsed().as_millis(),
                        path.display()
                    ),
                    Err(error) => {
                        eprintln!("Clean bundle 저장 실패: {error}");
                        return ExitCode::from(1);
                    }
                }
            }
            let elapsed_ms = result.elapsed_ms;
            eprintln!(
                "분석 완료: {}ms ({}.{:03}초)",
                elapsed_ms,
                elapsed_ms / 1_000,
                elapsed_ms % 1_000
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("분석 시작 실패: {error}");
            ExitCode::from(1)
        }
    }
}

fn run_eval(arguments: &[String]) -> ExitCode {
    let catalog = option_value(arguments, "--catalog");
    let gold_path = option_value(arguments, "--gold");
    let output_path = option_value(arguments, "--output");

    if catalog.is_some() && gold_path.is_some() {
        eprintln!("eval은 --gold 와 --catalog 를 함께 쓰지 않습니다.");
        return ExitCode::from(2);
    }

    if let Some(catalog_path) = catalog {
        let Some(clean_root) = option_value(arguments, "--clean-root") else {
            eprintln!("--catalog 에는 --clean-root=<clean 루트>가 필요합니다.");
            return ExitCode::from(2);
        };
        match code_analysis_engine::eval::evaluate_catalog(
            Path::new(&catalog_path),
            Path::new(&clean_root),
        ) {
            Ok(report) => {
                print_eval_catalog(&report);
            return finish_eval_output(output_path.as_deref(), &report, report.failed == 0);
            }
            Err(error) => {
                eprintln!("평가 실패: {error}");
                return ExitCode::from(1);
            }
        }
    }

    let Some(gold_path) = gold_path else {
        eprintln!("eval에는 --gold=<정답.json> 또는 --catalog=<catalog.json>이 필요합니다.");
        return ExitCode::from(2);
    };
    let gold = match code_analysis_engine::eval::load_gold(Path::new(&gold_path)) {
        Ok(gold) => gold,
        Err(error) => {
            eprintln!("정답 JSON을 읽지 못했습니다: {error}");
            return ExitCode::from(2);
        }
    };

    let report = if let Some(clean_dir) = option_value(arguments, "--clean") {
        code_analysis_engine::eval::evaluate_clean(&gold, Path::new(&clean_dir))
    } else if let Some(overview_path) = option_value(arguments, "--overview") {
        code_analysis_engine::eval::evaluate_analysis_file(&gold, Path::new(&overview_path))
    } else if let Some(project_path) = option_value(arguments, "--project") {
        let mut request = AnalysisRequest::new(&project_path);
        if let Some(path) = option_value(arguments, "--config") {
            match AnalysisConfig::from_file(Path::new(&path)) {
                Ok(config) => request.options.config = config,
                Err(error) => {
                    eprintln!("설정 적용 실패: {error}");
                    return ExitCode::from(2);
                }
            }
        }
        match analyze(request) {
            Ok(result) => code_analysis_engine::eval::evaluate_result(&gold, &result),
            Err(error) => {
                eprintln!("분석 시작 실패: {error}");
                return ExitCode::from(1);
            }
        }
    } else {
        eprintln!("eval에는 --clean, --overview, --project 중 하나가 필요합니다.");
        return ExitCode::from(2);
    };

    match report {
        Ok(report) => {
            print_eval_report(&report);
            finish_eval_output(output_path.as_deref(), &report, report.passed)
        }
        Err(error) => {
            eprintln!("평가 실패: {error}");
            ExitCode::from(1)
        }
    }
}

fn print_eval_report(report: &code_analysis_engine::eval::EvalReport) {
    let status = match report.outcome {
        code_analysis_engine::eval::EvalOutcome::PassPositive => "PASS",
        code_analysis_engine::eval::EvalOutcome::PassNegativeOnly => "PASS(negative-only)",
        code_analysis_engine::eval::EvalOutcome::Fail => "FAIL",
    };
    eprintln!(
        "eval {id}: {status} domains={domain_hits}/{domain_expected} features={feature_hits}/{feature_expected} flows={flow_hits}/{flow_expected} findings={findings}",
        id = report.id,
        domain_hits = report.domain_hits,
        domain_expected = report.domain_expected,
        feature_hits = report.feature_hits,
        feature_expected = report.feature_expected,
        flow_hits = report.flow_hits,
        flow_expected = report.flow_expected,
        findings = report.findings.len()
    );
    for finding in &report.findings {
        eprintln!("  [{}/{}] {}", finding.layer, finding.kind, finding.message);
    }
}

fn run_eval_diagnose_pairs(arguments: &[String]) -> ExitCode {
    let Some(catalog_path) = option_value(arguments, "--catalog") else {
        eprintln!("eval diagnose-pairs에는 --catalog=<catalog.json>이 필요합니다.");
        return ExitCode::from(2);
    };
    let output_path = option_value(arguments, "--output");
    let project_ids =
        code_analysis_engine::eval::parse_project_ids(option_value(arguments, "--ids").as_deref());
    let mode_selection = code_analysis_engine::eval::parse_mode_selection(
        option_value(arguments, "--mode").as_deref(),
    );
    let top_k = option_value(arguments, "--top")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(20);

    match code_analysis_engine::eval::diagnose_pair_catalog(
        Path::new(&catalog_path),
        &project_ids,
        mode_selection,
        top_k,
    ) {
        Ok(report) => {
            print_pair_diagnose_summary(&report);
            finish_eval_output(output_path.as_deref(), &report, true)
        }
        Err(error) => {
            eprintln!("pair diagnostics 실패: {error}");
            ExitCode::from(1)
        }
    }
}

fn run_eval_diagnose_gold_pairs(arguments: &[String]) -> ExitCode {
    let Some(catalog_path) = option_value(arguments, "--catalog") else {
        eprintln!("eval diagnose-gold-pairs에는 --catalog=<catalog.json>이 필요합니다.");
        return ExitCode::from(2);
    };
    let output_path = option_value(arguments, "--output");
    let project_ids =
        code_analysis_engine::eval::parse_project_ids(option_value(arguments, "--ids").as_deref());
    let mode_selection = code_analysis_engine::eval::parse_mode_selection(
        option_value(arguments, "--mode").as_deref(),
    );
    let include_evidence = arguments.iter().any(|argument| argument == "--evidence");

    match code_analysis_engine::eval::diagnose_gold_pair_catalog(
        Path::new(&catalog_path),
        &project_ids,
        mode_selection,
        include_evidence,
    ) {
        Ok(report) => {
            print_gold_pair_signal_summary(&report);
            finish_eval_output(output_path.as_deref(), &report, true)
        }
        Err(error) => {
            eprintln!("gold pair signal diagnostics 실패: {error}");
            ExitCode::from(1)
        }
    }
}

fn print_gold_pair_signal_summary(report: &code_analysis_engine::eval::GoldPairSignalReport) {
    for project in &report.projects {
        if project.skipped {
            eprintln!(
                "  {}: SKIP ({})",
                project.id,
                project.skip_reason.as_deref().unwrap_or("unknown")
            );
            continue;
        }
        eprintln!(
            "  {}: goldPairs positive={} negative={}",
            project.id, project.positive_count, project.negative_count
        );
        for mode in &project.modes {
            eprintln!(
                "    [{}] matched +{}/{} -{}/{} | posAvg combined={:.3} path={:.3} lexical={:.3} resource={:.3} | negAvg combined={:.3} path={:.3} lexical={:.3} resource={:.3}",
                mode.clustering_mode,
                mode.matched_positive,
                mode.positive_count,
                mode.matched_negative,
                mode.negative_count,
                mode.positive_avg.combined,
                mode.positive_avg.path,
                mode.positive_avg.lexical,
                mode.positive_avg.resource,
                mode.negative_avg.combined,
                mode.negative_avg.path,
                mode.negative_avg.lexical,
                mode.negative_avg.resource,
            );
        }
    }
}

fn print_pair_diagnose_summary(report: &code_analysis_engine::eval::PairDiagnoseReport) {
    for project in &report.projects {
        if project.skipped {
            eprintln!(
                "  {}: SKIP ({})",
                project.id,
                project.skip_reason.as_deref().unwrap_or("unknown")
            );
            continue;
        }
        for mode in &project.modes {
            eprintln!(
                "  {} [{}]: capabilities={} crossKeyPairs={} rejected={:?} top={}",
                project.id,
                mode.clustering_mode,
                mode.capabilities,
                mode.cross_key_pairs,
                mode.rejected,
                mode.top_cross_key_candidates.len()
            );
            for candidate in mode.top_cross_key_candidates.iter().take(5) {
                eprintln!(
                    "    {} ↔ {} combined={:.3} rejection={} gate={:?} http={:.2} call={:.2} flow={:.2} resource={:.2} path={:.2} lexical={:.2}",
                    candidate.left_key,
                    candidate.right_key,
                    candidate.similarity.combined,
                    candidate.rejection,
                    candidate.merge_gate,
                    candidate.similarity.http_match,
                    candidate.similarity.call,
                    candidate.similarity.flow,
                    candidate.similarity.resource,
                    candidate.similarity.path,
                    candidate.similarity.lexical,
                );
            }
        }
    }
}

fn run_eval_domain_recovery(arguments: &[String]) -> ExitCode {
    let Some(catalog_path) = option_value(arguments, "--catalog") else {
        eprintln!("eval domain-recovery에는 --catalog=<catalog.json>이 필요합니다.");
        return ExitCode::from(2);
    };
    let output_path = option_value(arguments, "--output");
    match code_analysis_engine::eval::evaluate_domain_recovery_catalog(Path::new(&catalog_path)) {
        Ok(report) => {
            print_domain_recovery_summary(&report);
            finish_eval_output(output_path.as_deref(), &report, report.summary.failed == 0)
        }
        Err(error) => {
            eprintln!("domain recovery 평가 실패: {error}");
            ExitCode::from(1)
        }
    }
}

fn print_domain_recovery_summary(
    report: &code_analysis_engine::eval::DomainRecoveryCatalogReport,
) {
    let summary = &report.summary;
    let baseline = &report.historical_clustering_baseline;
    eprintln!(
        "eval domain-recovery: projects={} analyzed={} skipped={} passed={} failed={}",
        summary.projects, summary.analyzed, summary.skipped, summary.passed, summary.failed
    );
    eprintln!(
        "  domainRecovery: domains={}/{} miss={} features={}/{} miss={} flows={}/{} miss={}",
        summary.domain_hits,
        summary.domain_expected,
        summary.domain_miss,
        summary.feature_hits,
        summary.feature_expected,
        summary.feature_miss,
        summary.flow_hits,
        summary.flow_expected,
        summary.flow_miss,
    );
    eprintln!(
        "  goldMismatches: wrongDomain={} overSplit={} overMerge(underSplit)={} unassigned={} noCandidate={}",
        summary.wrong_domain,
        summary.over_split,
        summary.over_merge,
        summary.unassigned,
        summary.no_candidate,
    );
    eprintln!(
        "  historicalClusteringBaseline: legacy={}/{} passed={} structural={}/{} passed={} ({})",
        baseline.legacy_domain_hits,
        baseline.legacy_domain_expected,
        baseline.legacy_passed,
        baseline.structural_domain_hits,
        baseline.structural_domain_expected,
        baseline.structural_passed,
        baseline.note,
    );
    for project in &report.projects {
        if project.skipped {
            eprintln!(
                "  {} SKIP: {}",
                project.id,
                project.skip_reason.as_deref().unwrap_or("-")
            );
            continue;
        }
        let status = match project.outcome {
            code_analysis_engine::eval::EvalOutcome::PassPositive => "PASS",
            code_analysis_engine::eval::EvalOutcome::PassNegativeOnly => "PASS(negative-only)",
            code_analysis_engine::eval::EvalOutcome::Fail => "FAIL",
        };
        eprintln!(
            "  {} {status}: domains={}/{} features={}/{} flows={}/{} wrongDomain={} overSplit={} overMerge={} unassigned={} noCandidate={}",
            project.id,
            project.domain_hits,
            project.domain_expected,
            project.feature_hits,
            project.feature_expected,
            project.flow_hits,
            project.flow_expected,
            project.wrong_domain,
            project.over_split,
            project.over_merge,
            project.unassigned,
            project.no_candidate,
        );
    }
}

fn run_eval_diagnose_domain_seeds(arguments: &[String]) -> ExitCode {
    let Some(catalog_path) = option_value(arguments, "--catalog") else {
        eprintln!("eval diagnose-domain-seeds에는 --catalog=<catalog.json>이 필요합니다.");
        return ExitCode::from(2);
    };
    let output_path = option_value(arguments, "--output");
    let project_ids =
        code_analysis_engine::eval::parse_project_ids(option_value(arguments, "--ids").as_deref());

    match code_analysis_engine::eval::diagnose_domain_seed_catalog(
        Path::new(&catalog_path),
        &project_ids,
    ) {
        Ok(report) => {
            print_domain_seed_summary(&report);
            finish_eval_output(output_path.as_deref(), &report, true)
        }
        Err(error) => {
            eprintln!("domain seed diagnostics 실패: {error}");
            ExitCode::from(1)
        }
    }
}

fn print_domain_seed_summary(report: &code_analysis_engine::eval::DomainSeedCatalogReport) {
    eprintln!("eval diagnose-domain-seeds: projects={}", report.projects.len());
    for project in &report.projects {
        if project.skipped {
            eprintln!(
                "  {}: SKIP ({})",
                project.id,
                project.skip_reason.as_deref().unwrap_or("unknown")
            );
            continue;
        }
        let diagnostics = project.diagnostics.as_ref().expect("diagnostics");
        let affinity = &diagnostics.aggregation.anchor_capability_graph;
        let eligibility = &diagnostics.aggregation.domain_anchor_eligibility;
        let ablation = &affinity.retrieval_ablation;
        let assignments = &ablation.assignment_summary;
        let ambiguity = &affinity.assignment_ambiguity;
        eprintln!(
            "  {}: capabilities={} rankedSeeds={} rankedFamilies={} graphEdges={} sparseEdges={} sparseDensity={:.3} provenanceEdges={} provenanceDensity={:.3} affinityEdges={} diagnosticAnchors={} hypothesisNodes={} eligibleAnchors={} subconcept={} redundant={} confidentAssignments={} ambiguousAssignments={} weakAssignments={} unassignedAssignments={} noCandidateAssignments={} retrievalMedian={:.1} retrievalP95={} kgNodes={} kgEdges={} eligibleNodes={} explicitAnchors={} ambiguous={} excludedAction={}",
            project.id,
            diagnostics.capabilities,
            diagnostics.aggregation.ranked_candidates.len(),
            diagnostics.aggregation.ranked_concept_families.len(),
            diagnostics.aggregation.seed_candidate_graph.edges.len(),
            diagnostics.aggregation.sparse_seed_candidate_graph.edge_count,
            diagnostics.aggregation.sparse_seed_candidate_graph.graph_density,
            diagnostics.aggregation.provenance_seed_candidate_graph.provenance_independent_edge_count,
            diagnostics.aggregation.provenance_seed_candidate_graph.graph_density,
            affinity.retrieval_summary.total_affinity_edge_count,
            eligibility.diagnostic_anchor_count,
            eligibility.hypothesis_node_count,
            eligibility.eligible_domain_anchor_count,
            eligibility.subconcept_candidate_count,
            eligibility.redundant_anchor_candidate_count,
            assignments.confident_assignments,
            assignments.ambiguous_assignments,
            assignments.weak_assignments,
            assignments.unassigned_assignments,
            assignments.no_candidate_assignments,
            affinity.retrieval_summary.median_retrieved_candidate_count,
            affinity.retrieval_summary.p95_retrieved_candidate_count,
            diagnostics.knowledge_graph_ir.nodes.len(),
            diagnostics.knowledge_graph_ir.edges.len(),
            diagnostics.aggregation.sparse_seed_candidate_graph.eligible_node_count,
            diagnostics.aggregation.sparse_seed_candidate_graph.explicit_anchor_count,
            diagnostics.aggregation.sparse_seed_candidate_graph.ambiguous_node_count,
            diagnostics.aggregation.sparse_seed_candidate_graph.excluded_action_cross_cutting_count
        );
        eprintln!(
            "    conceptHierarchy: parentSubconceptEdges={} wouldDemote={} (not applied)",
            eligibility.concept_hierarchy.parent_subconcept_edge_count,
            eligibility.concept_hierarchy.would_demote_count,
        );
        for edge in eligibility
            .concept_hierarchy
            .parent_subconcept_edges
            .iter()
            .take(5)
        {
            eprintln!(
                "      parent={parent} child={child} confidence={confidence:.2} signals={signals}",
                parent = edge.parent_root_concept,
                child = edge.child_root_concept,
                confidence = edge.confidence,
                signals = edge.signals.join(","),
            );
        }
        eprintln!(
            "    retrievalAblation: baselineCandidateEdges={} suppressedOwnerOnlyEdges={} weakLexical={} genericModulePackage={} genericOwnerRole={} behaviorOnly={}",
            ablation.baseline_candidate_edge_count,
            ablation.suppressed_owner_only_edges,
            ablation.weak_evidence_contributions.weak_lexical_edges,
            ablation.weak_evidence_contributions.generic_module_package_edges,
            ablation.weak_evidence_contributions.generic_owner_role_edges,
            ablation.weak_evidence_contributions.behavior_only_edges,
        );
        for summary in &ablation.channel_summaries {
            eprintln!(
                "      channel {}: generated={} unique={} exclusive={} multiChannel={}",
                summary.channel,
                summary.generated_candidate_edges,
                summary.unique_candidate_edges,
                summary.exclusive_edges,
                summary.multi_channel_edges,
            );
        }
        for scenario in ablation
            .channel_only_ablations
            .iter()
            .chain(ablation.leave_one_out_ablations.iter())
        {
            eprintln!(
                "      ablation {} {}: edges={} median={:.1} p95={}",
                scenario.scenario_kind,
                scenario.channel,
                scenario.candidate_edge_count,
                scenario.capability_candidate_median,
                scenario.capability_candidate_p95,
            );
        }
        for record in ablation.top_hypothesis_fanout.iter().take(20) {
            eprintln!(
                "      fanout #{rank} {root} ({hypothesis}): capabilities={fanout} ratio={ratio:.3} channels={channels}",
                rank = ablation
                    .top_hypothesis_fanout
                    .iter()
                    .position(|item| item.hypothesis_id == record.hypothesis_id)
                    .map(|index| index + 1)
                    .unwrap_or(0),
                root = record.representative_root_concept,
                hypothesis = record.hypothesis_id,
                fanout = record.retrieval_fanout_capabilities,
                ratio = record.retrieval_fanout_ratio,
                channels = record.dominant_retrieval_channels.join(", "),
            );
        }
        for record in ablation.top_capability_candidate_load.iter().take(20) {
            let upstream = ambiguity
                .high_candidate_load_cases
                .iter()
                .find(|case| case.capability_key == record.capability_key)
                .map(|case| case.upstream_diagnosis.as_str())
                .unwrap_or("-");
            eprintln!(
                "      load #{rank} {capability}: candidates={count} channels={channels} upstream={upstream}",
                rank = ablation
                    .top_capability_candidate_load
                    .iter()
                    .position(|item| item.capability_key == record.capability_key)
                    .map(|index| index + 1)
                    .unwrap_or(0),
                capability = record.capability_key,
                count = record.retrieved_candidate_count,
                channels = record.dominant_retrieval_channels.join(", "),
                upstream = upstream,
            );
        }
        let owner_gate = &ambiguity.owner_suppression_verification;
        eprintln!(
            "    ownerSuppression: suppressed={} ownerExclusiveBeforeGate={} suppressedPure={} suppressedOwnerPlusWeak={} retainedOwnerReinforced={}",
            owner_gate.suppressed_owner_only_edges,
            owner_gate.owner_exclusive_pairs_before_gate,
            owner_gate.suppressed_owner_only_pure,
            owner_gate.suppressed_owner_with_weak_independent,
            owner_gate.retained_owner_reinforced_edges,
        );
        eprintln!(
            "    assignmentAmbiguity: ambiguous={} weak={}",
            ambiguity.ambiguous_assignment_count,
            ambiguity.weak_assignment_count,
        );
        for class_count in &ambiguity.ambiguity_class_counts {
            eprintln!(
                "      class {}: count={}",
                class_count.ambiguity_class, class_count.count
            );
        }
        for case in ambiguity.representative_cases.iter().take(8) {
            let top2 = case.top2.as_ref().map(|side| side.root_concept.as_str()).unwrap_or("-");
            eprintln!(
                "      ambiguous {capability} class={class} equiv={equiv} scope={scope} margin={margin:.3} top1={top1}({top1_score:.2}) top2={top2}",
                capability = case.capability_key,
                class = case.ambiguity_class,
                equiv = case.responsibility_equivalence_class,
                scope = case.responsibility_scope_ambiguity_class,
                margin = case.margin,
                top1 = case.top1.root_concept,
                top1_score = case.top1.symbolic_affinity_score,
                top2 = top2,
            );
        }
        let equivalence = &ambiguity.responsibility_equivalence;
        eprintln!(
            "    responsibilityEquivalence: anchorPairs={} ambiguousPairs={}",
            equivalence.analyzed_anchor_pair_count,
            equivalence.ambiguous_top_pair_count,
        );
        eprintln!("      anchorPairClasses:");
        for class_count in &equivalence.anchor_pair_class_counts {
            eprintln!(
                "        {}: count={}",
                class_count.equivalence_class, class_count.count
            );
        }
        eprintln!("      ambiguousPairClasses:");
        for class_count in &equivalence.ambiguous_pair_class_counts {
            eprintln!(
                "        {}: count={}",
                class_count.equivalence_class, class_count.count
            );
        }
        for record in equivalence.representative_ambiguous_pairs.iter().take(6) {
            eprintln!(
                "        ambiguous {capability} equiv={equiv} ambiguity={ambiguity} top1={top1} top2={top2} margin={margin:.3}",
                capability = record.capability_key,
                equiv = record.equivalence_class,
                ambiguity = record.ambiguity_class,
                top1 = record.top1_root_concept,
                top2 = record.top2_root_concept,
                margin = record.margin,
            );
        }
        let scope = &ambiguity.responsibility_scope;
        eprintln!(
            "    responsibilityScope: eligibleAnchors={}",
            scope.eligible_anchor_count,
        );
        eprintln!("      anchorScopeClasses:");
        for class_count in &scope.scope_class_counts {
            eprintln!(
                "        {}: count={}",
                class_count.scope_class, class_count.count
            );
        }
        for record in scope.top_fanout_anchors.iter().take(8) {
            eprintln!(
                "      fanout {root} scope={scope} fanout={fanout}/{total} ratio={ratio:.3} capDisp={cap_disp:.3} entityConc={entity:.3} flowConc={flow:.3} provDiv={prov:.3}",
                root = record.representative_root_concept,
                scope = record.scope_class,
                fanout = record.fanout_capabilities,
                total = diagnostics.capabilities,
                ratio = record.fanout_ratio,
                cap_disp = record.capability_dispersion,
                entity = record.entity_resource_concentration,
                flow = record.flow_neighborhood_concentration,
                prov = record.provenance_diversity,
            );
        }
        eprintln!("      ambiguousScopeClasses:");
        for class_count in &scope.responsibility_ambiguity_class_counts {
            eprintln!(
                "        {}: count={}",
                class_count.scope_class, class_count.count
            );
        }
        for record in scope.representative_ambiguous_scope_cases.iter().take(6) {
            eprintln!(
                "        ambiguous {capability} scope={scope} top1={top1}({top1_scope}) top2={top2}({top2_scope}) margin={margin:.3}",
                capability = record.capability_key,
                scope = record.responsibility_scope_ambiguity_class,
                top1 = record.top1_root_concept,
                top1_scope = record.top1_scope_class,
                top2 = record.top2_root_concept,
                top2_scope = record.top2_scope_class,
                margin = record.margin,
            );
        }
        let decomposition = &ambiguity.scope_classification_decomposition;
        let project = &decomposition.project_context;
        eprintln!(
            "    scopeClassificationDecomposition: scopeLike={} suspectedFalsePositive={}",
            decomposition.scope_like_anchor_count,
            decomposition.suspected_false_positive_count,
        );
        eprintln!(
            "      projectContext: capabilities={} tier={} eligibleAnchors={} partitions(entry/module/package/owner)={}/{}/{}/{} evidenceConfidence={}",
            project.capability_count,
            project.project_size_tier,
            project.eligible_anchor_count,
            project.independent_partition_counts.entrypoint_count,
            project.independent_partition_counts.module_path_count,
            project.independent_partition_counts.package_path_count,
            project.independent_partition_counts.owner_class_count,
            project.primitive_evidence_coverage.confidence,
        );
        eprintln!(
            "      primitiveEvidence: owners={} entities={} resources={} calls={} flows={} entityResourceSparse={} ownerSparse={}",
            project.primitive_evidence_coverage.owner_relations,
            project.primitive_evidence_coverage.entity_relations,
            project.primitive_evidence_coverage.resource_relations,
            project.primitive_evidence_coverage.call_relations,
            project.primitive_evidence_coverage.flow_relations,
            project.primitive_evidence_coverage.entity_resource_sparse,
            project.primitive_evidence_coverage.owner_sparse,
        );
        if !decomposition.failure_mode_counts.is_empty() {
            eprintln!("      failureModes:");
            for mode_count in &decomposition.failure_mode_counts {
                eprintln!(
                    "        {}: count={}",
                    mode_count.failure_mode, mode_count.count
                );
            }
        }
        for record in decomposition.scope_like_decompositions.iter().take(8) {
            let top_reason = record
                .classification_reasons
                .iter()
                .max_by(|left, right| {
                    left.weighted_contribution
                        .partial_cmp(&right.weighted_contribution)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .map(|reason| reason.reason.as_str())
                .unwrap_or("-");
            eprintln!(
                "      scopeLike {root} fanout={fanout}/{total} ratio={ratio:.3} absoluteTier={tier} suspectedFP={fp} modes={modes} topReason={top_reason}",
                root = record.representative_root_concept,
                fanout = record.retrieval_fanout_capabilities,
                total = project.capability_count,
                ratio = record.fanout_ratio,
                tier = record.absolute_support_tier,
                fp = record.suspected_false_positive,
                modes = record.failure_modes.join(","),
                top_reason = top_reason,
            );
        }
        for record in decomposition.comparison_highlights.iter() {
            eprintln!(
                "      compare {root}: fanout={fanout}/{total} ratio={ratio:.3} absoluteTier={tier} entrySpan={entry} moduleSpan={module} ownerSpan={owner} entityConf={entity_conf} entityUsable={entity_usable} scopeScore={scope:.3} respScore={resp:.3} modes={modes}",
                root = record.representative_root_concept,
                fanout = record.retrieval_fanout_capabilities,
                total = project.capability_count,
                ratio = record.fanout_ratio,
                tier = record.absolute_support_tier,
                entry = record.partition_spans.entrypoint_span,
                module = record.partition_spans.module_span,
                owner = record.partition_spans.owner_span,
                entity_conf = record.entity_resource_evidence.confidence,
                entity_usable = record.entity_resource_evidence.usable_for_responsibility,
                scope = record.scope_score,
                resp = record.responsibility_score,
                modes = record.failure_modes.join(","),
            );
            for reason in record.classification_reasons.iter().take(4) {
                eprintln!(
                    "        reason {reason}: value={value:.3} contrib={contrib:.3} {detail}",
                    reason = reason.reason,
                    value = reason.value,
                    contrib = reason.weighted_contribution,
                    detail = reason.detail,
                );
            }
        }
        let scope_role = &ambiguity.scope_role;
        eprintln!(
            "    scopeRole: broadAnchors={}",
            scope_role.broad_anchor_count,
        );
        if !scope_role.role_class_counts.is_empty() {
            eprintln!("      roleClasses:");
            for class_count in &scope_role.role_class_counts {
                eprintln!(
                    "        {}: count={}",
                    class_count.scope_role_class, class_count.count
                );
            }
        }
        for record in scope_role.anchor_role_records.iter().take(8) {
            eprintln!(
                "      broad {root} scope={scope} role={role} fanout={fanout}/{total} traversal={traversal:.3} cohesion={cohesion:.3} container={container:.3} structural={structural:.3} responsibility={responsibility:.3} suspectedFP={fp}",
                root = record.representative_root_concept,
                scope = record.scope_class,
                role = record.scope_role_class,
                fanout = record.retrieval_fanout_capabilities,
                total = project.capability_count,
                traversal = record.mediation_metrics.partition_traversal,
                cohesion = record.mediation_metrics.neighborhood_cohesion,
                container = record.mediation_metrics.common_container_overlap,
                structural = record.mediation_metrics.structural_role_score,
                responsibility = record.mediation_metrics.responsibility_role_score,
                fp = record.suspected_false_positive,
            );
        }
        for record in scope_role.comparison_highlights.iter() {
            eprintln!(
                "      compareRole {root}: role={role} fanout={fanout}/{total} traversal={traversal:.3} cohesion={cohesion:.3} independentEvidence={indep:.3} container={container:.3} ownershipBehavior={ownership}",
                root = record.representative_root_concept,
                role = record.scope_role_class,
                fanout = record.retrieval_fanout_capabilities,
                total = project.capability_count,
                traversal = record.mediation_metrics.partition_traversal,
                cohesion = record.mediation_metrics.neighborhood_cohesion,
                indep = record.mediation_metrics.independent_evidence_score,
                container = record.mediation_metrics.common_container_overlap,
                ownership = record.mediation_metrics.has_independent_ownership_state_behavior,
            );
            for reason in record.classification_reasons.iter().take(5) {
                eprintln!(
                    "        reason {reason}: value={value:.3} {detail}",
                    reason = reason.reason,
                    value = reason.value,
                    detail = reason.detail,
                );
            }
        }
        if eligibility.diagnostic_anchor_count > eligibility.hypothesis_node_count {
            eprintln!(
                "    duplicateDiagnosticFamilies: groups={}",
                eligibility.duplicate_diagnostic_family_groups.len()
            );
            for group in eligibility.duplicate_diagnostic_family_groups.iter().take(5) {
                for inclusion in group.inclusions.iter() {
                    eprintln!(
                        "      {} familyId={} rootConcept={} reason={}",
                        group.hypothesis_id,
                        inclusion.family_id,
                        inclusion.root_concept,
                        inclusion.inclusion_reason,
                    );
                }
            }
        }
        if !eligibility.evidence_coverage_gaps.is_empty() {
            eprintln!(
                "    evidenceCoverageGaps: {}",
                eligibility.evidence_coverage_gaps.join(", ")
            );
        }
        let rejection_summary = &diagnostics
            .aggregation
            .provenance_seed_candidate_graph
            .pair_rejection_summary;
        eprintln!(
            "    provenanceRejections: candidates={} rejected={} accepted={}",
            rejection_summary.total_candidate_pairs,
            rejection_summary.rejected_pair_count,
            rejection_summary.accepted_pair_count
        );
        for (reason, count) in &rejection_summary.rejection_reason_counts {
            eprintln!("      {reason}={count}");
        }
        let inventory = &diagnostics
            .aggregation
            .provenance_seed_candidate_graph
            .primitive_relation_inventory;
        eprintln!(
            "    primitiveRelations: owners={} entities={} resources={} calls={} flows={}",
            inventory.owner_relations,
            inventory.entity_relations,
            inventory.resource_relations,
            inventory.call_relations,
            inventory.flow_relations
        );
        for ranked in diagnostics.aggregation.ranked_concept_families.iter().take(8) {
            eprintln!(
                "    family #{rank} {root}: final={final_score:.3} role={role} alignment={alignment:.2} rawDisp={raw_disp:.2} effDisp={eff_disp:.2} anchorSymbolic={anchor:.2}",
                rank = ranked.rank,
                root = ranked.root_concept,
                final_score = ranked.final_seed_score,
                role = ranked.concept_role.role_class,
                alignment = ranked.concept_role.business_root_alignment,
                raw_disp = ranked.concept_role.context_dispersion,
                eff_disp = ranked.concept_role.effective_context_dispersion,
                anchor = ranked.anchor_score_components.symbolic_total,
            );
        }
        if diagnostics.aggregation.ranked_concept_families.len() > 8 {
            eprintln!(
                "    ... +{} more ranked families",
                diagnostics.aggregation.ranked_concept_families.len() - 8
            );
        }
        for ranked in diagnostics.aggregation.ranked_candidates.iter().take(5) {
            eprintln!(
                "    #{rank} {concept}: support={support} salience={salience:.3} extraction={extraction:.3} generic={generic:.2} transport={transport:.2} idf={idf:.2}",
                rank = ranked.rank,
                concept = ranked.concept,
                support = ranked.support_capabilities,
                salience = ranked.business_domain_salience,
                extraction = ranked.extraction_confidence,
                generic = ranked.genericness,
                transport = ranked.transportness,
                idf = ranked.idf_penalty,
            );
        }
        if diagnostics.aggregation.ranked_candidates.len() > 5 {
            eprintln!(
                "    ... +{} more ranked seeds",
                diagnostics.aggregation.ranked_candidates.len() - 5
            );
        }
    }
}

fn run_eval_ab(arguments: &[String]) -> ExitCode {
    let Some(catalog_path) = option_value(arguments, "--catalog") else {
        eprintln!("eval ab에는 --catalog=<catalog.json>이 필요합니다.");
        return ExitCode::from(2);
    };
    let output_path = option_value(arguments, "--output");
    let experiment = arguments.iter().any(|argument| argument == "--experiment");
    if experiment {
        match code_analysis_engine::eval::compare_clustering_modes_experiment(Path::new(
            &catalog_path,
        )) {
            Ok(report) => {
                print_clustering_ab_experiment_summary(&report);
                finish_eval_output(output_path.as_deref(), &report, true)
            }
            Err(error) => {
                eprintln!("clustering A/B 실험 평가 실패: {error}");
                ExitCode::from(1)
            }
        }
    } else {
        match code_analysis_engine::eval::compare_clustering_modes(Path::new(&catalog_path)) {
            Ok(report) => {
                print_clustering_ab_summary(&report);
                finish_eval_output(output_path.as_deref(), &report, true)
            }
            Err(error) => {
                eprintln!("clustering A/B 평가 실패: {error}");
                ExitCode::from(1)
            }
        }
    }
}

fn print_clustering_ab_summary(report: &code_analysis_engine::eval::ClusteringAbReport) {
    let summary = &report.summary;
    eprintln!(
        "eval ab: projects={} analyzed={} skipped={}",
        summary.projects, summary.analyzed, summary.skipped
    );
    eprintln!(
        "  legacy: passed={}/{} domainHits={}/{} featureHits={}/{} overSplit={} wrongDomain={} overMerge={}",
        summary.legacy_passed,
        summary.analyzed,
        summary.legacy_domain_hits,
        summary.legacy_domain_expected,
        summary.legacy_feature_hits,
        summary.legacy_feature_expected,
        summary.legacy_over_split,
        summary.legacy_wrong_domain,
        summary.legacy_over_merge
    );
    eprintln!(
        "  structural: passed={}/{} domainHits={}/{} featureHits={}/{} overSplit={} wrongDomain={} overMerge={}",
        summary.structural_passed,
        summary.analyzed,
        summary.structural_domain_hits,
        summary.structural_domain_expected,
        summary.structural_feature_hits,
        summary.structural_feature_expected,
        summary.structural_over_split,
        summary.structural_wrong_domain,
        summary.structural_over_merge
    );
    for project in &report.projects {
        if project.skipped {
            eprintln!(
                "  {}: SKIP ({})",
                project.id,
                project.skip_reason.as_deref().unwrap_or("unknown")
            );
            continue;
        }
        let legacy = project.legacy.as_ref().expect("legacy report");
        let structural = project.structural.as_ref().expect("structural report");
        eprintln!(
            "  {}: legacy domains={}/{} features={}/{} forbiddenRatio={:.0}% merges={} absorbed={} | structural domains={}/{} features={}/{} forbiddenRatio={:.0}% merges={} absorbed={}",
            project.id,
            legacy.domain_hits,
            legacy.domain_expected,
            legacy.feature_hits,
            legacy.feature_expected,
            legacy.formation_diagnostics.forbidden_ratio * 100.0,
            legacy.formation_diagnostics.clustering_merges,
            legacy.formation_diagnostics.absorbed_domains,
            structural.domain_hits,
            structural.domain_expected,
            structural.feature_hits,
            structural.feature_expected,
            structural.formation_diagnostics.forbidden_ratio * 100.0,
            structural.formation_diagnostics.clustering_merges,
            structural.formation_diagnostics.absorbed_domains,
        );
    }
}

fn print_clustering_ab_experiment_summary(
    report: &code_analysis_engine::eval::ClusteringAbExperimentReport,
) {
    print_clustering_ab_summary(&code_analysis_engine::eval::ClusteringAbReport {
        summary: report.summary.clone(),
        projects: report.projects.clone(),
    });
    let gold = &report.gold_pair_summary;
    eprintln!(
        "  gold pairs (project-equal): legacy posEligible={:.1}% negEligible={:.1}% | structural posEligible={:.1}% negEligible={:.1}% | v2 posEligible={:.1}% negEligible={:.1}%",
        gold.legacy.positive_eligible_rate_project_equal * 100.0,
        gold.legacy.negative_eligible_rate_project_equal * 100.0,
        gold.structural.positive_eligible_rate_project_equal * 100.0,
        gold.structural.negative_eligible_rate_project_equal * 100.0,
        gold.v2.positive_eligible_rate_project_equal * 100.0,
        gold.v2.negative_eligible_rate_project_equal * 100.0,
    );
    eprintln!(
        "  gold pairs (family-equal): legacy posEligible={:.1}% negEligible={:.1}% | structural posEligible={:.1}% negEligible={:.1}% | v2 posEligible={:.1}% negEligible={:.1}%",
        gold.legacy.positive_eligible_rate_family_equal * 100.0,
        gold.legacy.negative_eligible_rate_family_equal * 100.0,
        gold.structural.positive_eligible_rate_family_equal * 100.0,
        gold.structural.negative_eligible_rate_family_equal * 100.0,
        gold.v2.positive_eligible_rate_family_equal * 100.0,
        gold.v2.negative_eligible_rate_family_equal * 100.0,
    );
    eprintln!(
        "  gold pair signals (project-equal avg): v2 pos combined={:.3} path={:.3} lexical={:.3}",
        gold.v2.positive_avg_combined_project_equal,
        gold.v2.positive_avg_path_project_equal,
        gold.v2.positive_avg_lexical_project_equal,
    );
    let summary = &report.summary;
    eprintln!(
        "  v2: passed={}/{} domainHits={}/{} featureHits={}/{} overSplit={} wrongDomain={} overMerge={}",
        summary.v2_passed,
        summary.analyzed,
        summary.v2_domain_hits,
        summary.v2_domain_expected,
        summary.v2_feature_hits,
        summary.v2_feature_expected,
        summary.v2_over_split,
        summary.v2_wrong_domain,
        summary.v2_over_merge,
    );
    for project in &report.projects {
        if project.skipped {
            continue;
        }
        let v2 = project.v2.as_ref().expect("v2 report");
        eprintln!(
            "  {} v2: domains={}/{} features={}/{} merges={}",
            project.id,
            v2.domain_hits,
            v2.domain_expected,
            v2.feature_hits,
            v2.feature_expected,
            v2.formation_diagnostics.clustering_merges,
        );
    }
}

fn print_eval_catalog(report: &code_analysis_engine::eval::CatalogReport) {
    eprintln!(
        "eval catalog: passed={} failed={}",
        report.passed, report.failed
    );
    for item in &report.reports {
        print_eval_report(item);
    }
}

fn finish_eval_output<T: serde::Serialize>(
    output_path: Option<&str>,
    value: &T,
    passed: bool,
) -> ExitCode {
    if let Some(path) = output_path {
        if let Err(error) = output::write_pretty_json(Path::new(path), value) {
            eprintln!("평가 리포트 저장 실패: {error}");
            return ExitCode::from(1);
        }
        eprintln!("평가 리포트 저장 완료: {path}");
    } else {
        match serde_json::to_string_pretty(value) {
            Ok(json) => println!("{json}"),
            Err(error) => {
                eprintln!("평가 리포트를 직렬화하지 못했습니다: {error}");
                return ExitCode::from(1);
            }
        }
    }
    if passed {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

fn run_clean_bundle(arguments: &[String]) -> ExitCode {
    let Some(input_path) = option_value(arguments, "--input") else {
        eprintln!("clean bundle에는 --input=<분석 결과 JSON>이 필요합니다.");
        return ExitCode::from(2);
    };
    let Some(output_path) = option_value(arguments, "--output") else {
        eprintln!("clean bundle에는 --output=<clean 디렉터리>가 필요합니다.");
        return ExitCode::from(2);
    };
    let source = match fs::read_to_string(&input_path) {
        Ok(source) => source,
        Err(error) => {
            eprintln!("분석 결과 JSON을 읽지 못했습니다: {error}");
            return ExitCode::from(1);
        }
    };
    let result: code_analysis_engine::AnalysisResult = match serde_json::from_str(&source) {
        Ok(result) => result,
        Err(error) => {
            eprintln!("분석 결과 JSON을 해석하지 못했습니다: {error}");
            return ExitCode::from(1);
        }
    };
    let config = match option_value(arguments, "--config") {
        Some(path) => match AnalysisConfig::from_file(Path::new(&path)) {
            Ok(config) => config,
            Err(error) => {
                eprintln!("설정 적용 실패: {error}");
                return ExitCode::from(2);
            }
        },
        None => AnalysisConfig::default(),
    };
    let mut policy = config.clean;
    if let Some(value) = option_value(arguments, "--part-target-bytes") {
        match value.parse::<usize>() {
            Ok(bytes) => policy.part_target_bytes = bytes,
            Err(error) => {
                eprintln!("--part-target-bytes를 해석하지 못했습니다: {error}");
                return ExitCode::from(2);
            }
        }
    }
    let started = Instant::now();
    match code_analysis_engine::clean::write_from_result(&result, Path::new(&output_path), &policy)
    {
        Ok(manifest) => {
            eprintln!(
                "Clean bundle 생성 완료: bundleId={} datasets={} elapsed_ms={} output={}",
                manifest.bundle_id,
                manifest.datasets.len(),
                started.elapsed().as_millis(),
                output_path
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("Clean bundle 생성 실패: {error}");
            ExitCode::from(1)
        }
    }
}

fn dataset_count(manifest: &code_analysis_engine::clean::CleanBundleManifest, name: &str) -> usize {
    manifest
        .datasets
        .iter()
        .find(|dataset| dataset.name == name)
        .map(|dataset| dataset.count)
        .unwrap_or(0)
}

fn run_ai_context_postprocess(arguments: &[String]) -> ExitCode {
    let Some(input_path) = option_value(arguments, "--input") else {
        eprintln!("postprocess ai-context에는 --input=<분석 결과 JSON 또는 clean 디렉터리>이 필요합니다.");
        return ExitCode::from(2);
    };
    let Some(output_path) = option_value(arguments, "--output") else {
        eprintln!("postprocess ai-context에는 --output=<컨텍스트 JSON>이 필요합니다.");
        return ExitCode::from(2);
    };
    let config = match option_value(arguments, "--config") {
        Some(path) => match AnalysisConfig::from_file(Path::new(&path)) {
            Ok(config) => config,
            Err(error) => {
                eprintln!("설정 적용 실패: {error}");
                return ExitCode::from(2);
            }
        },
        None => AnalysisConfig::default(),
    };
    let pretty = arguments.iter().any(|argument| argument == "--pretty");
    let input = Path::new(&input_path);
    let bundle = if input.is_dir() {
        eprintln!("Clean bundle에서 AI 컨텍스트를 생성합니다: {input_path}");
        match code_analysis_engine::postprocess::build_ai_context_from_clean(input, &config) {
            Ok(context) => context,
            Err(error) => {
                eprintln!("AI 컨텍스트 후보정 실패: {error}");
                return ExitCode::from(1);
            }
        }
    } else {
        let source = match fs::read_to_string(&input_path) {
            Ok(source) => source,
            Err(error) => {
                eprintln!("분석 결과 JSON을 읽지 못했습니다: {error}");
                return ExitCode::from(1);
            }
        };
        let result: code_analysis_engine::AnalysisResult = match serde_json::from_str(&source) {
            Ok(result) => result,
            Err(error) => {
                eprintln!("분석 결과 JSON을 해석하지 못했습니다: {error}");
                return ExitCode::from(1);
            }
        };
        match code_analysis_engine::postprocess::build_ai_context(&result, &config) {
            Ok(context) => context,
            Err(error) => {
                eprintln!("AI 컨텍스트 후보정 실패: {error}");
                return ExitCode::from(1);
            }
        }
    };
    if bundle.chunks.len() == 1 {
        if let Err(error) = write_context_json(Path::new(&output_path), &bundle.chunks[0], pretty) {
            eprintln!("AI 컨텍스트 저장 실패: {error}");
            return ExitCode::from(1);
        }
        let context = &bundle.chunks[0];
        eprintln!(
            "AI 컨텍스트 후보정 완료: domains={} features={}/{} flows={}/{} bytes={} output={}",
            context.summary.included_domains,
            context.summary.included_features,
            context.summary.total_features,
            context.summary.included_flows,
            context.summary.total_flows,
            context.summary.used_bytes,
            output_path
        );
    } else {
        let output = Path::new(&output_path);
        let chunk_directory = output.with_file_name(format!(
            "{}.chunks",
            output
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or("ai-context")
        ));
        if let Err(error) = fs::create_dir_all(&chunk_directory) {
            eprintln!("AI 컨텍스트 chunk 디렉터리 생성 실패: {error}");
            return ExitCode::from(1);
        }
        for (descriptor, chunk) in bundle.manifest.chunks.iter().zip(&bundle.chunks) {
            let chunk_path = chunk_directory.join(&descriptor.file_name);
            if let Err(error) = write_context_json(&chunk_path, chunk, pretty) {
                eprintln!("AI 컨텍스트 chunk 저장 실패: {error}");
                return ExitCode::from(1);
            }
        }
        if let Err(error) = output::write_pretty_json(output, &bundle.manifest) {
            eprintln!("AI 컨텍스트 manifest 저장 실패: {error}");
            return ExitCode::from(1);
        }
        eprintln!(
            "AI 컨텍스트 후보정 완료: chunks={} output={} chunkDirectory={}",
            bundle.chunks.len(),
            output_path,
            chunk_directory.display()
        );
    }
    ExitCode::SUCCESS
}

fn run_semantic_review(arguments: &[String]) -> ExitCode {
    let Some(input_path) = option_value(arguments, "--input") else {
        eprintln!("semantic review에는 --input=<ai-context.json>이 필요합니다.");
        return ExitCode::from(2);
    };
    let Some(output_path) = option_value(arguments, "--output") else {
        eprintln!("semantic review에는 --output=<semantic-result.json>이 필요합니다.");
        return ExitCode::from(2);
    };
    let Some(project_root) = option_value(arguments, "--project-root") else {
        eprintln!("semantic review에는 --project-root=<프로젝트-경로>가 필요합니다.");
        return ExitCode::from(2);
    };
    let config = match option_value(arguments, "--config") {
        Some(path) => match AnalysisConfig::from_file(Path::new(&path)) {
            Ok(config) => config,
            Err(error) => {
                eprintln!("설정 적용 실패: {error}");
                return ExitCode::from(2);
            }
        },
        None => AnalysisConfig::default(),
    };
    let mut semantic_policy = config.semantic;
    if let Some(provider) = option_value(arguments, "--provider") {
        semantic_policy.provider = provider;
    }
    if let Some(model) = option_value(arguments, "--model") {
        match semantic_policy.provider.as_str() {
            "claude" => semantic_policy.claude_model = Some(model),
            _ => semantic_policy.codex_model = Some(model),
        }
    }
    let provider_label = match semantic_policy.provider.as_str() {
        "claude" => "Claude",
        _ => "Codex",
    };
    let started = Instant::now();
    match code_analysis_engine::semantic::review::run(
        Path::new(&input_path),
        Path::new(&output_path),
        Path::new(&project_root),
        &semantic_policy,
    ) {
        Ok(result) => {
            eprintln!(
                "{provider_label} 의미 분석 완료: status={} chunks={}/{} domainChunks={} retries={} domains={} features={} flows={} output={}",
                result.status,
                result.completed_chunks,
                result.chunk_count,
                result.domain_completed_chunks,
                result.retry_attempts,
                result.domains.len(),
                result.features.len(),
                result.flows.len(),
                output_path
            );
            for warning in result
                .warnings
                .iter()
                .filter(|warning| warning.code.ends_with("CHUNK_FAILED"))
            {
                eprintln!(
                    "[semantic] chunk_failed id={} message={}",
                    warning.item_id.as_deref().unwrap_or("unknown"),
                    warning.message
                );
            }
            if arguments.iter().any(|argument| argument == "--profile") {
                eprintln!(
                    "[profile] semantic_review_process_elapsed_ms={}",
                    started.elapsed().as_millis()
                );
            }
            if result.status == "failed" {
                ExitCode::from(1)
            } else {
                ExitCode::SUCCESS
            }
        }
        Err(error) => {
            eprintln!("Codex 의미 분석 실패: {error}");
            ExitCode::from(1)
        }
    }
}

fn write_context_json<T: serde::Serialize>(
    path: &Path,
    value: &T,
    pretty: bool,
) -> std::io::Result<()> {
    if pretty {
        output::write_pretty_json(path, value)
    } else {
        output::write_result_json(path, value, true, usize::MAX)
    }
}

fn option_value(arguments: &[String], name: &str) -> Option<String> {
    arguments.iter().find_map(|argument| {
        argument
            .strip_prefix(&format!("{name}="))
            .map(ToOwned::to_owned)
    })
}
