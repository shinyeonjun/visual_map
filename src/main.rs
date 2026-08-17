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
        eprintln!("       code-analysis-engine eval ab --catalog=<catalog.json> [--output=<ab-report.json>]");
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

fn run_eval_ab(arguments: &[String]) -> ExitCode {
    let Some(catalog_path) = option_value(arguments, "--catalog") else {
        eprintln!("eval ab에는 --catalog=<catalog.json>이 필요합니다.");
        return ExitCode::from(2);
    };
    let output_path = option_value(arguments, "--output");
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
