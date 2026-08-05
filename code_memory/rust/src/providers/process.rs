use std::collections::VecDeque;
use std::env;
use std::io::Read;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Debug)]
pub(crate) struct BoundedCommandOutput {
    pub(crate) status: ExitStatus,
    pub(crate) stdout: Vec<u8>,
    pub(crate) stderr: Vec<u8>,
    pub(crate) stdout_truncated: bool,
    pub(crate) stderr_truncated: bool,
}

pub(crate) struct ProviderProcessGuard {
    #[cfg(windows)]
    job: Option<std::os::windows::io::OwnedHandle>,
}

impl ProviderProcessGuard {
    pub(crate) fn attach(child: &Child) -> Self {
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

    pub(crate) fn terminate(&self, child: &mut Child) {
        #[cfg(windows)]
        if let Some(job) = &self.job {
            use std::os::windows::io::AsRawHandle;
            use windows_sys::Win32::System::JobObjects::TerminateJobObject;

            // SAFETY: the owned job handle remains valid for this call.
            if unsafe { TerminateJobObject(job.as_raw_handle() as _, 1) } != 0 {
                let _ = child.wait();
                return;
            }
        }
        terminate_process_tree(child);
    }
}

#[cfg(windows)]
fn attach_windows_job(child: &Child) -> Result<std::os::windows::io::OwnedHandle, String> {
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

/// Run a provider without allowing its output or lifetime to grow without a
/// bound. stdout keeps its prefix because it normally contains a structured
/// document; stderr keeps its tail because failures are usually reported last.
pub(crate) fn run_bounded_command(
    mut command: Command,
    label: &str,
    timeout: Duration,
    stdout_limit: usize,
    stderr_limit: usize,
) -> Result<BoundedCommandOutput, String> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|error| format!("cannot run {label}: {error}"))?;
    let process_guard = ProviderProcessGuard::attach(&child);
    let stdout = match child.stdout.take() {
        Some(stdout) => stdout,
        None => {
            process_guard.terminate(&mut child);
            return Err(format!("{label} stdout unavailable"));
        }
    };
    let stderr = match child.stderr.take() {
        Some(stderr) => stderr,
        None => {
            process_guard.terminate(&mut child);
            return Err(format!("{label} stderr unavailable"));
        }
    };
    let stdout_reader = thread::spawn(move || capture_head(stdout, stdout_limit));
    let stderr_reader = thread::spawn(move || capture_tail(stderr, stderr_limit));
    let deadline = Instant::now() + timeout;

    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                process_guard.terminate(&mut child);
                let (stdout, stdout_truncated) = stdout_reader
                    .join()
                    .map_err(|_| format!("{label} stdout reader failed"))?;
                let (stderr, stderr_truncated) = stderr_reader
                    .join()
                    .map_err(|_| format!("{label} stderr reader failed"))?;
                return Ok(BoundedCommandOutput {
                    status,
                    stdout,
                    stderr,
                    stdout_truncated,
                    stderr_truncated,
                });
            }
            Ok(None) if Instant::now() >= deadline => {
                process_guard.terminate(&mut child);
                let _ = stdout_reader.join();
                let _ = stderr_reader.join();
                return Err(format!(
                    "{label} timeout after {} seconds",
                    timeout.as_secs()
                ));
            }
            Ok(None) => thread::sleep(Duration::from_millis(25)),
            Err(error) => {
                process_guard.terminate(&mut child);
                let _ = stdout_reader.join();
                let _ = stderr_reader.join();
                return Err(format!("{label} wait failed: {error}"));
            }
        }
    }
}

fn capture_head(mut stream: impl Read, limit: usize) -> (Vec<u8>, bool) {
    let mut kept = Vec::with_capacity(limit.min(64 * 1024));
    let mut buffer = [0u8; 16 * 1024];
    let mut truncated = false;
    while let Ok(read) = stream.read(&mut buffer) {
        if read == 0 {
            break;
        }
        let remaining = limit.saturating_sub(kept.len());
        kept.extend_from_slice(&buffer[..read.min(remaining)]);
        truncated |= read > remaining;
    }
    (kept, truncated)
}

fn capture_tail(mut stream: impl Read, limit: usize) -> (Vec<u8>, bool) {
    let mut kept = VecDeque::with_capacity(limit.min(64 * 1024));
    let mut buffer = [0u8; 16 * 1024];
    let mut total = 0usize;
    while let Ok(read) = stream.read(&mut buffer) {
        if read == 0 {
            break;
        }
        total = total.saturating_add(read);
        for byte in &buffer[..read] {
            if kept.len() == limit {
                kept.pop_front();
            }
            if limit > 0 {
                kept.push_back(*byte);
            }
        }
    }
    (kept.into_iter().collect(), total > limit)
}

pub(crate) fn provider_timeout() -> Duration {
    env::var("CODE_MEMORY_PROVIDER_TIMEOUT_SECONDS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value == 0 || (5..=1_800).contains(value))
        .map(|value| {
            if value == 0 {
                // ponytail: practical no-timeout sentinel; use an optional
                // deadline only if a caller must distinguish this state.
                Duration::from_secs(60 * 60 * 24 * 365 * 10)
            } else {
                Duration::from_secs(value)
            }
        })
        .unwrap_or_else(|| Duration::from_secs(180))
}

pub(crate) fn terminate_process_tree(child: &mut Child) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;

        let mut command = Command::new("taskkill");
        let _ = command
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

#[cfg(test)]
mod tests {
    use super::{capture_head, capture_tail};

    #[test]
    fn bounded_streams_keep_the_useful_side() {
        let input = b"0123456789";
        assert_eq!(capture_head(&input[..], 4), (b"0123".to_vec(), true));
        assert_eq!(capture_tail(&input[..], 4), (b"6789".to_vec(), true));
    }

    #[cfg(windows)]
    #[test]
    fn windows_job_kills_descendant_helper() {
        use std::env;
        use std::process::Command;
        use std::time::Duration;

        let Ok(mode) = env::var("CODE_MEMORY_JOB_TEST_MODE") else {
            return;
        };
        let marker = env::var("CODE_MEMORY_JOB_TEST_MARKER").unwrap();
        if mode == "parent" {
            std::thread::sleep(Duration::from_millis(250));
            let descendant = Command::new(env::current_exe().unwrap())
                .args([
                    "--exact",
                    "providers::process::tests::windows_job_kills_descendant_helper",
                ])
                .env("CODE_MEMORY_JOB_TEST_MODE", "descendant")
                .env("CODE_MEMORY_JOB_TEST_MARKER", marker)
                .spawn()
                .unwrap();
            // The parent must exit before this child to reproduce the orphaned
            // provider process that the Job Object owns.
            std::mem::forget(descendant);
        } else {
            std::thread::sleep(Duration::from_secs(1));
            std::fs::write(marker, "survived").unwrap();
        }
    }

    #[cfg(windows)]
    #[test]
    fn windows_job_terminates_descendant_after_parent_exits() {
        use super::ProviderProcessGuard;
        use std::env;
        use std::process::{Command, Stdio};
        use std::time::Duration;

        let marker = env::temp_dir().join(format!(
            "code-memory-job-{}-{}.marker",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut parent = Command::new(env::current_exe().unwrap())
            .args([
                "--exact",
                "providers::process::tests::windows_job_kills_descendant_helper",
            ])
            .env("CODE_MEMORY_JOB_TEST_MODE", "parent")
            .env("CODE_MEMORY_JOB_TEST_MARKER", &marker)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let guard = ProviderProcessGuard::attach(&parent);
        assert!(parent.wait().unwrap().success());
        guard.terminate(&mut parent);
        std::thread::sleep(Duration::from_millis(1_250));
        assert!(!marker.exists(), "job descendant survived provider exit");
    }
}
