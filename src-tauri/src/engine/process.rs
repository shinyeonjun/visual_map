fn validate_sidecar_args(args: &[String]) -> Result<(), String> {
    for arg in args {
        let lower = arg.to_ascii_lowercase();
        let token = lower.trim_start_matches('-');
        if matches!(
            token,
            "install" | "installer" | "setup" | "register" | "register-mcp" | "mcp-register"
        ) || lower.ends_with(".ps1")
            || lower.ends_with(".bat")
            || lower.ends_with(".cmd")
            || lower.ends_with(".msi")
            || lower.contains("claude_desktop_config")
            || lower.contains("mcp_config")
        {
            return Err("허용되지 않는 sidecar 실행 인자입니다".to_string());
        }
    }

    Ok(())
}

pub(crate) fn run_command(
    executable: &Path,
    args: &[&str],
    timeout: Duration,
) -> Result<EngineRunResult, String> {
    run_command_with_env(executable, args, timeout, &[])
}

pub(crate) fn run_command_with_env(
    executable: &Path,
    args: &[&str],
    timeout: Duration,
    envs: &[(&str, &str)],
) -> Result<EngineRunResult, String> {
    run_command_with_env_observer(
        executable,
        args,
        EngineRunPolicy::fixed(timeout),
        envs,
        None,
    )
}

pub(crate) fn run_command_with_env_observer(
    executable: &Path,
    args: &[&str],
    policy: EngineRunPolicy,
    envs: &[(&str, &str)],
    observer: Option<EngineObserver>,
) -> Result<EngineRunResult, String> {
    let started_at = timestamp();
    let process_started = Instant::now();
    let deadline = process_started + policy.hard_timeout;
    let last_activity_ms = Arc::new(AtomicU64::new(0));
    let cancellation = envs
        .iter()
        .find(|(key, _)| *key == "BACKEND_VISUAL_MAP_OPERATION_ID")
        .and_then(|(_, operation_id)| cancellation_for_operation(operation_id));
    if cancellation
        .as_ref()
        .is_some_and(|value| value.load(Ordering::Acquire))
    {
        return Ok(cancelled_engine_run(started_at, Vec::new(), Vec::new()));
    }
    let mut command = Command::new(executable);
    command
        .args(args)
        .envs(envs.iter().copied())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    hide_console_window(&mut command);
    let mut child = command
        .spawn()
        .map_err(|error| format!("읽기 도구 실행 실패: {error}"))?;
    let process_guard = EngineProcessGuard::attach(&child);
    let stdout = match child.stdout.take() {
        Some(stdout) => stdout,
        None => {
            process_guard.terminate(&mut child);
            return Err("읽기 도구 stdout을 열지 못했습니다".to_string());
        }
    };
    let stderr = match child.stderr.take() {
        Some(stderr) => stderr,
        None => {
            process_guard.terminate(&mut child);
            return Err("읽기 도구 stderr를 열지 못했습니다".to_string());
        }
    };
    let output_limit_reached = Arc::new(AtomicBool::new(false));
    let stdout_limit = Arc::clone(&output_limit_reached);
    let stderr_limit = Arc::clone(&output_limit_reached);
    let stdout_activity = Arc::clone(&last_activity_ms);
    let stderr_activity = Arc::clone(&last_activity_ms);
    let stdout_observer = observer.clone();
    let stderr_observer = observer;
    let stdout_reader = thread::spawn(move || {
        read_process_stream_with_signal(
            stdout,
            stdout_limit,
            stdout_activity,
            process_started,
            stdout_observer,
            "stdout",
        )
    });
    let stderr_reader = thread::spawn(move || {
        read_process_stream_with_signal(
            stderr,
            stderr_limit,
            stderr_activity,
            process_started,
            stderr_observer,
            "stderr",
        )
    });

    loop {
        let status = match child.try_wait() {
            Ok(status) => status,
            Err(error) => {
                process_guard.terminate(&mut child);
                let _ = collect_process_streams(stdout_reader, stderr_reader);
                return Err(format!("읽기 도구 상태 확인 실패: {error}"));
            }
        };
        if let Some(status) = status {
            // A sidecar can exit while a descendant still owns the inherited
            // pipe. Ask the platform to terminate the tree before joining
            // readers; the bounded join below is the final no-hang guard.
            process_guard.terminate(&mut child);
            let (stdout, stderr) = collect_process_streams(stdout_reader, stderr_reader)?;

            return Ok(EngineRunResult {
                ok: status.success(),
                stdout: redact_secrets(&String::from_utf8_lossy(&stdout)),
                stderr: redact_secrets(&String::from_utf8_lossy(&stderr)),
                exit_code: status.code(),
                started_at,
                finished_at: timestamp(),
            });
        }

        if output_limit_reached.load(Ordering::Acquire) {
            process_guard.terminate(&mut child);
            let _ = collect_process_streams(stdout_reader, stderr_reader);
            return Err(format!(
                "읽기 도구 출력이 안전 한도({MAX_ENGINE_STREAM_BYTES} bytes)를 초과했습니다"
            ));
        }

        if cancellation
            .as_ref()
            .is_some_and(|value| value.load(Ordering::Acquire))
        {
            process_guard.terminate(&mut child);
            let (stdout, stderr) = collect_process_streams(stdout_reader, stderr_reader)?;
            return Ok(cancelled_engine_run(started_at, stdout, stderr));
        }

        if Instant::now() >= deadline {
            process_guard.terminate(&mut child);
            let (stdout, stderr) = collect_process_streams(stdout_reader, stderr_reader)?;
            let stderr = String::from_utf8_lossy(&stderr);
            let stderr = if stderr.trim().is_empty() {
                "읽기 도구 실행 시간이 초과되었습니다".to_string()
            } else {
                format!(
                    "{}\n읽기 도구 실행 시간이 초과되었습니다",
                    stderr.trim_end()
                )
            };

            return Ok(EngineRunResult {
                ok: false,
                stdout: redact_secrets(&String::from_utf8_lossy(&stdout)),
                stderr: redact_secrets(&stderr),
                exit_code: None,
                started_at,
                finished_at: timestamp(),
            });
        }

        let elapsed_ms = process_started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
        let idle_ms = policy.idle_timeout.as_millis().min(u128::from(u64::MAX)) as u64;
        if elapsed_ms.saturating_sub(last_activity_ms.load(Ordering::Acquire)) >= idle_ms {
            process_guard.terminate(&mut child);
            let (stdout, stderr) = collect_process_streams(stdout_reader, stderr_reader)?;
            let stderr = String::from_utf8_lossy(&stderr);
            let message = format!(
                "읽기 도구에서 {}초 동안 진행 신호가 없어 중단했습니다",
                policy.idle_timeout.as_secs()
            );
            let stderr = if stderr.trim().is_empty() {
                message
            } else {
                format!("{}\n{message}", stderr.trim_end())
            };
            return Ok(EngineRunResult {
                ok: false,
                stdout: redact_secrets(&String::from_utf8_lossy(&stdout)),
                stderr: redact_secrets(&stderr),
                exit_code: None,
                started_at,
                finished_at: timestamp(),
            });
        }

        thread::sleep(Duration::from_millis(10));
    }
}

struct EngineProcessGuard {
    #[cfg(windows)]
    job: Option<std::os::windows::io::OwnedHandle>,
}

impl EngineProcessGuard {
    fn attach(child: &std::process::Child) -> Self {
        #[cfg(windows)]
        {
            let job = attach_windows_job(child).map_err(|error| {
                eprintln!("Windows Job Object unavailable; using taskkill fallback: {error}")
            });
            Self { job: job.ok() }
        }
        #[cfg(not(windows))]
        {
            let _ = child;
            Self {}
        }
    }

    fn terminate(&self, child: &mut std::process::Child) {
        #[cfg(windows)]
        if let Some(job) = &self.job {
            use std::os::windows::io::AsRawHandle;
            use windows_sys::Win32::System::JobObjects::TerminateJobObject;

            // SAFETY: the owned handle remains valid for this call.
            if unsafe { TerminateJobObject(job.as_raw_handle() as _, 1) } != 0 {
                let _ = child.wait();
                return;
            }
        }
        terminate_engine_process_tree_fallback(child);
    }
}

#[cfg(windows)]
fn attach_windows_job(
    child: &std::process::Child,
) -> Result<std::os::windows::io::OwnedHandle, String> {
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };

    // SAFETY: null security/name pointers request an unnamed job with defaults.
    let raw_job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
    if raw_job.is_null() {
        return Err(std::io::Error::last_os_error().to_string());
    }
    // SAFETY: CreateJobObjectW returned an owned HANDLE.
    let job = unsafe { OwnedHandle::from_raw_handle(raw_job as _) };
    let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    // SAFETY: pointers reference initialized values for the documented call duration.
    if unsafe {
        SetInformationJobObject(
            job.as_raw_handle() as _,
            JobObjectExtendedLimitInformation,
            std::ptr::addr_of!(limits).cast(),
            std::mem::size_of_val(&limits) as u32,
        )
    } == 0
    {
        return Err(std::io::Error::last_os_error().to_string());
    }
    // SAFETY: both handles are valid and remain owned by their Rust values.
    if unsafe { AssignProcessToJobObject(job.as_raw_handle() as _, child.as_raw_handle() as _) }
        == 0
    {
        return Err(std::io::Error::last_os_error().to_string());
    }
    Ok(job)
}

fn terminate_engine_process_tree_fallback(child: &mut std::process::Child) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;

        let _ = Command::new("taskkill")
            .creation_flags(0x08000000)
            .args(["/PID", &child.id().to_string(), "/T", "/F"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(windows)]
fn hide_console_window(command: &mut Command) {
    use std::os::windows::process::CommandExt;

    // CREATE_NO_WINDOW keeps bundled console sidecars invisible in the GUI app.
    command.creation_flags(0x08000000);
}

#[cfg(not(windows))]
fn hide_console_window(_command: &mut Command) {}

fn cancelled_engine_run(started_at: String, stdout: Vec<u8>, stderr: Vec<u8>) -> EngineRunResult {
    let stderr = String::from_utf8_lossy(&stderr);
    let stderr = if stderr.trim().is_empty() {
        "읽기 도구 실행이 취소되었습니다".to_string()
    } else {
        format!("{}\n읽기 도구 실행이 취소되었습니다", stderr.trim_end())
    };
    EngineRunResult {
        ok: false,
        stdout: redact_secrets(&String::from_utf8_lossy(&stdout)),
        stderr: redact_secrets(&stderr),
        exit_code: None,
        started_at,
        finished_at: timestamp(),
    }
}

struct CapturedProcessStream {
    bytes: Vec<u8>,
    exceeded_limit: bool,
}

fn read_process_stream_with_signal(
    stream: impl Read,
    overflow: Arc<AtomicBool>,
    activity: Arc<AtomicU64>,
    started: Instant,
    observer: Option<EngineObserver>,
    stream_name: &'static str,
) -> Result<CapturedProcessStream, String> {
    read_process_stream_with_limit_and_signal(
        stream,
        MAX_ENGINE_STREAM_BYTES,
        Some(overflow),
        Some((activity, started)),
        observer,
        stream_name,
    )
}

#[cfg(test)]
fn read_process_stream_with_limit(
    stream: impl Read,
    limit: usize,
) -> Result<CapturedProcessStream, String> {
    read_process_stream_with_limit_and_signal(stream, limit, None, None, None, "test")
}

fn read_process_stream_with_limit_and_signal(
    mut stream: impl Read,
    limit: usize,
    overflow: Option<Arc<AtomicBool>>,
    activity: Option<(Arc<AtomicU64>, Instant)>,
    observer: Option<EngineObserver>,
    stream_name: &'static str,
) -> Result<CapturedProcessStream, String> {
    let mut output = Vec::with_capacity(limit.min(64 * 1024));
    let mut buffer = [0u8; 64 * 1024];
    let mut exceeded_limit = false;
    let mut pending_line = Vec::new();

    loop {
        let read = stream
            .read(&mut buffer)
            .map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        if let Some((activity, started)) = &activity {
            activity.store(
                started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
                Ordering::Release,
            );
        }
        if observer.is_some() {
            pending_line.extend_from_slice(&buffer[..read]);
            notify_progress_lines(&mut pending_line, observer.as_ref(), stream_name, false);
        }
        let remaining = limit.saturating_sub(output.len());
        let stored = remaining.min(read);
        output.extend_from_slice(&buffer[..stored]);
        if stored < read {
            exceeded_limit = true;
            if let Some(overflow) = &overflow {
                overflow.store(true, Ordering::Release);
            }
            break;
        }
    }

    notify_progress_lines(&mut pending_line, observer.as_ref(), stream_name, true);

    Ok(CapturedProcessStream {
        bytes: output,
        exceeded_limit,
    })
}

fn notify_progress_lines(
    pending: &mut Vec<u8>,
    observer: Option<&EngineObserver>,
    stream: &'static str,
    flush: bool,
) {
    let Some(observer) = observer else {
        return;
    };
    loop {
        let boundary = pending.iter().position(|byte| *byte == b'\n');
        let Some(boundary) = boundary.or_else(|| flush.then_some(pending.len())) else {
            if pending.len() > 16 * 1024 {
                pending.clear();
            }
            return;
        };
        let line = String::from_utf8_lossy(&pending[..boundary])
            .trim_end_matches('\r')
            .to_string();
        let consumed = (boundary + usize::from(boundary < pending.len())).min(pending.len());
        pending.drain(..consumed);
        if line.starts_with("@visual-map-progress ") || line.starts_with("timing stage=") {
            observer(EngineProcessEvent {
                stream,
                line: redact_secrets(&line),
            });
        }
        if pending.is_empty() {
            return;
        }
    }
}

fn collect_process_streams(
    stdout: thread::JoinHandle<Result<CapturedProcessStream, String>>,
    stderr: thread::JoinHandle<Result<CapturedProcessStream, String>>,
) -> Result<(Vec<u8>, Vec<u8>), String> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let result = (|| {
            let stdout = stdout
                .join()
                .map_err(|_| "읽기 도구 stdout 수집 작업이 중단됐습니다".to_string())??;
            let stderr = stderr
                .join()
                .map_err(|_| "읽기 도구 stderr 수집 작업이 중단됐습니다".to_string())??;
            Ok::<_, String>((stdout, stderr))
        })();
        let _ = sender.send(result);
    });
    let (stdout, stderr) = receiver
        .recv_timeout(Duration::from_secs(2))
        .map_err(|_| "읽기 도구 출력 수집이 시간 제한을 초과했습니다".to_string())??;
    if stdout.exceeded_limit || stderr.exceeded_limit {
        return Err(format!(
            "읽기 도구 출력이 안전 한도({MAX_ENGINE_STREAM_BYTES} bytes)를 초과했습니다"
        ));
    }
    Ok((stdout.bytes, stderr.bytes))
}

pub(crate) fn redact_secrets(input: &str) -> String {
    if let Ok(mut value) = serde_json::from_str::<serde_json::Value>(input) {
        redact_json_value(&mut value);
        if let Ok(redacted) = serde_json::to_string(&value) {
            return redacted;
        }
    }

    redact_unstructured_secrets(input)
}
