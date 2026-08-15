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
