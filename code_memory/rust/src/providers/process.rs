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
    let stdout = match child.stdout.take() {
        Some(stdout) => stdout,
        None => {
            terminate_process_tree(&mut child);
            return Err(format!("{label} stdout unavailable"));
        }
    };
    let stderr = match child.stderr.take() {
        Some(stderr) => stderr,
        None => {
            terminate_process_tree(&mut child);
            return Err(format!("{label} stderr unavailable"));
        }
    };
    let stdout_reader = thread::spawn(move || capture_head(stdout, stdout_limit));
    let stderr_reader = thread::spawn(move || capture_tail(stderr, stderr_limit));
    let deadline = Instant::now() + timeout;

    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
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
                terminate_process_tree(&mut child);
                let _ = stdout_reader.join();
                let _ = stderr_reader.join();
                return Err(format!(
                    "{label} timeout after {} seconds",
                    timeout.as_secs()
                ));
            }
            Ok(None) => thread::sleep(Duration::from_millis(25)),
            Err(error) => {
                terminate_process_tree(&mut child);
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
}
