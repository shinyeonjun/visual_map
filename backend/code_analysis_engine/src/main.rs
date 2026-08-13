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
        && arguments.get(1).map(String::as_str) == Some("codex-context")
    {
        return run_codex_context_postprocess(&arguments[2..]);
    }
    if arguments.first().map(String::as_str) == Some("semantic")
        && arguments.get(1).map(String::as_str) == Some("review")
    {
        return run_semantic_review(&arguments[2..]);
    }

    let Some(root_path) = arguments.first() else {
        eprintln!("사용법: code-analysis-engine <프로젝트-경로> [--compact] [--profile] [--no-cache] [--no-output] [--output=<경로>] [--prepared-output=<경로>] [--config=<경로>]");
        eprintln!("       code-analysis-engine postprocess codex-context --input=<분석결과.json> --output=<컨텍스트.json> [--config=<경로>] [--pretty] [--profile]");
        eprintln!("       code-analysis-engine semantic review --input=<codex-context.json> --output=<semantic-result.json> --project-root=<프로젝트-경로> [--config=<경로>] [--profile]");
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
    let prepared_output_path = flags
        .iter()
        .find_map(|argument| argument.strip_prefix("--prepared-output="))
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

    match analyze(request) {
        Ok(result) => {
            if let Some(path) = prepared_output_path {
                let Some(prepared) = result.preprocessed_overview.as_ref() else {
                    eprintln!("전처리 Overview가 생성되지 않았습니다.");
                    return ExitCode::from(1);
                };
                if let Err(error) = output::write_pretty_json(&path, prepared) {
                    eprintln!("전처리 Overview 저장 실패: {error}");
                    return ExitCode::from(1);
                }
                eprintln!("전처리 Overview 저장 완료: {}", path.display());
            }
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

fn run_codex_context_postprocess(arguments: &[String]) -> ExitCode {
    let Some(input_path) = option_value(arguments, "--input") else {
        eprintln!("postprocess codex-context에는 --input=<분석 결과 JSON>이 필요합니다.");
        return ExitCode::from(2);
    };
    let Some(output_path) = option_value(arguments, "--output") else {
        eprintln!("postprocess codex-context에는 --output=<컨텍스트 JSON>이 필요합니다.");
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
    let pretty = arguments.iter().any(|argument| argument == "--pretty");
    let bundle =
        match code_analysis_engine::postprocess::build_codex_context_bundle(&result, &config) {
            Ok(context) => context,
            Err(error) => {
                eprintln!("Codex 컨텍스트 후보정 실패: {error}");
                return ExitCode::from(1);
            }
        };
    if bundle.chunks.len() == 1 {
        if let Err(error) = write_context_json(Path::new(&output_path), &bundle.chunks[0], pretty) {
            eprintln!("Codex 컨텍스트 저장 실패: {error}");
            return ExitCode::from(1);
        }
        let context = &bundle.chunks[0];
        eprintln!(
            "Codex 컨텍스트 후보정 완료: domains={} features={}/{} flows={}/{} bytes={} output={}",
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
                .unwrap_or("codex-context")
        ));
        if let Err(error) = fs::create_dir_all(&chunk_directory) {
            eprintln!("Codex 컨텍스트 chunk 디렉터리 생성 실패: {error}");
            return ExitCode::from(1);
        }
        for (descriptor, chunk) in bundle.manifest.chunks.iter().zip(&bundle.chunks) {
            let chunk_path = chunk_directory.join(&descriptor.file_name);
            if let Err(error) = write_context_json(&chunk_path, chunk, pretty) {
                eprintln!("Codex 컨텍스트 chunk 저장 실패: {error}");
                return ExitCode::from(1);
            }
        }
        if let Err(error) = output::write_pretty_json(output, &bundle.manifest) {
            eprintln!("Codex 컨텍스트 manifest 저장 실패: {error}");
            return ExitCode::from(1);
        }
        eprintln!(
            "Codex 컨텍스트 후보정 완료: chunks={} output={} chunkDirectory={}",
            bundle.chunks.len(),
            output_path,
            chunk_directory.display()
        );
    }
    ExitCode::SUCCESS
}

fn run_semantic_review(arguments: &[String]) -> ExitCode {
    let Some(input_path) = option_value(arguments, "--input") else {
        eprintln!("semantic review에는 --input=<codex-context.json>이 필요합니다.");
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
    let started = Instant::now();
    match code_analysis_engine::semantic::review::run(
        Path::new(&input_path),
        Path::new(&output_path),
        Path::new(&project_root),
        &config.semantic,
    ) {
        Ok(result) => {
            eprintln!(
                "Codex 의미 분석 완료: status={} chunks={}/{} domains={} features={} flows={} output={}",
                result.status,
                result.completed_chunks,
                result.chunk_count,
                result.domains.len(),
                result.features.len(),
                result.flows.len(),
                output_path
            );
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
