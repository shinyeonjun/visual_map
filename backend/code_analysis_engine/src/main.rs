use code_analysis_engine::{analyze, config::AnalysisConfig, AnalysisRequest};
use std::env;
use std::fs;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

fn main() -> ExitCode {
    let arguments: Vec<String> = env::args().skip(1).collect();
    if arguments
        .iter()
        .any(|argument| argument.starts_with("--codex-names-from-result"))
    {
        return run_codex_names_from_result(&arguments);
    }

    let Some(root_path) = arguments.first() else {
        eprintln!("사용법: code-analysis-engine <프로젝트-경로> [--compact] [--profile] [--no-output] [--output=<경로>] [--codex] [--config=<경로>] [--codex-executable=<경로>] [--codex-timeout-ms=<밀리초>] [--codex-max-input-bytes=<바이트>] [--codex-context-output=<경로>] [--codex-context-only]");
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
    let codex_enabled = flags.iter().any(|argument| argument == "--codex");
    // [DEV ONLY] 컨텍스트 덤프만 확인할 때 36MB 이상인 최종 분석 JSON 출력을
    // 생략하는 옵션이다. 제품 완성 단계에서는 별도 진단 명령으로 분리한다.
    let codex_context_only = flags
        .iter()
        .any(|argument| argument == "--codex-context-only");
    let config_path = flags
        .iter()
        .find_map(|argument| argument.strip_prefix("--config="));
    let codex_executable = flags
        .iter()
        .find_map(|argument| argument.strip_prefix("--codex-executable="))
        .map(ToOwned::to_owned);
    let codex_timeout_ms = flags
        .iter()
        .find_map(|argument| argument.strip_prefix("--codex-timeout-ms="))
        .and_then(|value| value.parse::<u64>().ok());
    let codex_max_input_bytes = flags
        .iter()
        .find_map(|argument| argument.strip_prefix("--codex-max-input-bytes="))
        .and_then(|value| value.parse::<usize>().ok());
    // [DEV ONLY] Codex 호출 직전 입력을 검사하기 위한 개발용 덤프 옵션이다.
    // 제품 완성 단계에서 제거하거나 별도 진단 명령으로 분리한다.
    let codex_context_output = flags
        .iter()
        .find_map(|argument| argument.strip_prefix("--codex-context-output="))
        .map(std::path::PathBuf::from);

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
    if codex_enabled {
        request.options.config.semantic.codex_enabled = true;
    }
    if let Some(executable) = codex_executable {
        request.options.config.semantic.codex_executable = executable;
    }
    if let Some(timeout_ms) = codex_timeout_ms {
        request.options.config.semantic.codex_timeout_ms = timeout_ms;
    }
    if let Some(max_input_bytes) = codex_max_input_bytes {
        request.options.config.semantic.codex_max_input_bytes = max_input_bytes;
    }
    request.options.codex_context_output = codex_context_output;
    let emit_codex_context_only =
        codex_context_only && request.options.codex_context_output.is_some();
    let max_output_bytes = request.options.config.limits.max_output_bytes;

    match analyze(request) {
        Ok(result) => {
            if emit_codex_context_only {
                eprintln!("Codex 컨텍스트 저장 완료: 최종 분석 JSON 출력 생략");
                return ExitCode::SUCCESS;
            }
            if no_output {
                eprintln!("분석 결과 JSON 출력 생략 (--no-output)");
            } else if let Some(path) = output_path {
                if let Err(error) = write_result_json(&path, &result, compact, max_output_bytes) {
                    eprintln!("결과 JSON 저장 실패: {error}");
                    return ExitCode::from(1);
                }
                eprintln!("분석 결과 저장 완료: {}", path.display());
            } else if let Err(error) = write_result_stdout(&result, compact, max_output_bytes) {
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

fn run_codex_names_from_result(arguments: &[String]) -> ExitCode {
    let Some(input_path) = option_value(arguments, "--codex-names-from-result") else {
        eprintln!("--codex-names-from-result=<결과 JSON 경로>가 필요합니다.");
        return ExitCode::from(2);
    };
    let result_source = match fs::read_to_string(&input_path) {
        Ok(source) => source,
        Err(error) => {
            eprintln!("분석 결과 JSON을 읽지 못했습니다: {error}");
            return ExitCode::from(1);
        }
    };
    let result: code_analysis_engine::AnalysisResult = match serde_json::from_str(&result_source) {
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
    let executable = option_value(arguments, "--codex-executable")
        .unwrap_or_else(|| config.semantic.codex_executable.clone());
    let timeout_ms = option_value(arguments, "--codex-timeout-ms")
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(config.semantic.codex_timeout_ms);
    let max_input_bytes = option_value(arguments, "--codex-max-input-bytes")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(config.semantic.codex_max_input_bytes);
    let max_context_bytes = max_input_bytes
        .saturating_sub(config.semantic.prompt_reserve_bytes)
        .max(config.semantic.minimum_context_bytes);
    let provider = code_analysis_engine::semantic::CodexProvider {
        executable,
        timeout_ms,
        max_input_bytes,
        command_prefix: Vec::new(),
    };
    let analyzer = code_analysis_engine::semantic::names::NameAnalyzer { provider };

    if let Some(context_path) = option_value(arguments, "--codex-names-context-output") {
        let artifact = match analyzer.context_from_result(&result, max_context_bytes) {
            Ok(artifact) => artifact,
            Err(error) => {
                eprintln!("이름 컨텍스트 생성 실패: {error}");
                return ExitCode::from(1);
            }
        };
        if let Err(error) = write_pretty_json(&context_path, &artifact) {
            eprintln!("이름 컨텍스트 저장 실패: {error}");
            return ExitCode::from(1);
        }
        eprintln!("Codex 이름 컨텍스트 저장 완료: {context_path}");
        if arguments
            .iter()
            .any(|argument| argument == "--codex-names-context-only")
        {
            return ExitCode::SUCCESS;
        }
    }

    let name_result = match analyzer.from_result(&result, max_context_bytes) {
        Ok(result) => result,
        Err(error) => {
            eprintln!("Codex 이름 분석 실패: {error}");
            return ExitCode::from(1);
        }
    };
    let json = match serde_json::to_string_pretty(&name_result) {
        Ok(json) => json,
        Err(error) => {
            eprintln!("이름 결과 JSON 생성 실패: {error}");
            return ExitCode::from(1);
        }
    };
    if let Some(output_path) = option_value(arguments, "--codex-names-output") {
        if let Err(error) = fs::write(&output_path, json) {
            eprintln!("이름 결과 저장 실패: {error}");
            return ExitCode::from(1);
        }
        eprintln!(
            "Codex 이름 분석 완료: status={} domains={} modules={} chunks={}",
            name_result.status,
            name_result.domains.len(),
            name_result.modules.len(),
            name_result.chunk_count
        );
    } else {
        println!("{json}");
    }
    ExitCode::SUCCESS
}

fn option_value(arguments: &[String], name: &str) -> Option<String> {
    arguments.iter().find_map(|argument| {
        argument
            .strip_prefix(&format!("{name}="))
            .map(ToOwned::to_owned)
    })
}

fn write_pretty_json<T: serde::Serialize>(path: &str, value: &T) -> std::io::Result<()> {
    let json = serde_json::to_string_pretty(value)
        .map_err(|error| std::io::Error::other(error.to_string()))?;
    fs::write(PathBuf::from(path), json)
}

fn write_result_stdout(
    result: &code_analysis_engine::AnalysisResult,
    compact: bool,
    max_output_bytes: usize,
) -> serde_json::Result<()> {
    let stdout = std::io::stdout();
    let writer = BufWriter::new(stdout.lock());
    let mut writer = LimitedWriter::new(writer, max_output_bytes);
    if compact {
        serde_json::to_writer(&mut writer, result)?;
    } else {
        serde_json::to_writer_pretty(&mut writer, result)?;
    }
    writer.write_all(b"\n").map_err(serde_json::Error::io)
}

fn write_result_json(
    path: &Path,
    result: &code_analysis_engine::AnalysisResult,
    compact: bool,
    max_output_bytes: usize,
) -> std::io::Result<()> {
    let file = fs::File::create(path)?;
    let writer = BufWriter::new(file);
    let mut writer = LimitedWriter::new(writer, max_output_bytes);
    if compact {
        serde_json::to_writer(&mut writer, result).map_err(std::io::Error::other)?;
    } else {
        serde_json::to_writer_pretty(&mut writer, result).map_err(std::io::Error::other)?;
    }
    writer.write_all(b"\n")
}

struct LimitedWriter<W> {
    inner: W,
    written: usize,
    limit: usize,
}

impl<W> LimitedWriter<W> {
    fn new(inner: W, limit: usize) -> Self {
        Self {
            inner,
            written: 0,
            limit,
        }
    }
}

impl<W: Write> Write for LimitedWriter<W> {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        if self.written >= self.limit {
            return Err(std::io::Error::other("OUTPUT_LIMIT_REACHED"));
        }
        let allowed = buffer.len().min(self.limit - self.written);
        let written = self.inner.write(&buffer[..allowed])?;
        self.written += written;
        if written < buffer.len() {
            return Err(std::io::Error::other("OUTPUT_LIMIT_REACHED"));
        }
        Ok(written)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}
