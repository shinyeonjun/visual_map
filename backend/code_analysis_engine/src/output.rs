//! 분석 결과 JSON을 안전하게 직렬화하고 저장하는 출력 계층.

use serde::Serialize;
use std::fs;
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) fn write_result_stdout<T: Serialize>(
    value: &T,
    compact: bool,
    max_output_bytes: usize,
) -> serde_json::Result<()> {
    let stdout = io::stdout();
    let writer = BufWriter::new(stdout.lock());
    let mut writer = LimitedWriter::new(writer, max_output_bytes);
    write_json(&mut writer, value, compact)?;
    writer.write_all(b"\n").map_err(serde_json::Error::io)
}

pub(crate) fn write_result_json<T: Serialize>(
    path: &Path,
    value: &T,
    compact: bool,
    max_output_bytes: usize,
) -> io::Result<()> {
    write_atomically(path, |temporary_path| {
        let file = fs::File::create(temporary_path)?;
        let writer = BufWriter::new(file);
        let mut writer = LimitedWriter::new(writer, max_output_bytes);
        write_json(&mut writer, value, compact).map_err(io::Error::other)?;
        writer.write_all(b"\n")?;
        writer.flush()
    })
}

pub(crate) fn write_pretty_json<T: Serialize>(path: &Path, value: &T) -> io::Result<()> {
    write_atomically(path, |temporary_path| {
        let json = serde_json::to_vec_pretty(value).map_err(io::Error::other)?;
        fs::write(temporary_path, json)
    })
}

fn write_json<W: Write, T: Serialize>(
    writer: &mut W,
    value: &T,
    compact: bool,
) -> serde_json::Result<()> {
    if compact {
        serde_json::to_writer(writer, value)
    } else {
        serde_json::to_writer_pretty(writer, value)
    }
}

/// 임시 파일에 완전히 쓴 뒤 최종 경로로 교체한다.
///
/// 직렬화나 출력 제한에 실패하면 기존 결과를 유지하고 임시 파일만
/// 삭제한다. 따라서 잘린 JSON이 다음 실행의 입력으로 남지 않는다.
fn write_atomically<F>(path: &Path, write: F) -> io::Result<()>
where
    F: FnOnce(&Path) -> io::Result<()>,
{
    let temporary_path = temporary_path(path);
    let write_result = write(&temporary_path);
    if let Err(error) = write_result {
        let _ = fs::remove_file(&temporary_path);
        return Err(error);
    }

    if let Err(error) = replace_file(&temporary_path, path) {
        let _ = fs::remove_file(&temporary_path);
        return Err(error);
    }
    Ok(())
}

fn temporary_path(path: &Path) -> PathBuf {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("analysis-result.json");
    path.with_file_name(format!(
        ".{file_name}.tmp-{}-{timestamp}",
        std::process::id()
    ))
}

fn replace_file(temporary_path: &Path, destination: &Path) -> io::Result<()> {
    #[cfg(not(windows))]
    {
        fs::rename(temporary_path, destination)
    }

    #[cfg(windows)]
    {
        if !destination.exists() {
            return fs::rename(temporary_path, destination);
        }

        let backup_path = temporary_path.with_extension("backup");
        fs::rename(destination, &backup_path)?;
        match fs::rename(temporary_path, destination) {
            Ok(()) => {
                let _ = fs::remove_file(backup_path);
                Ok(())
            }
            Err(error) => {
                let _ = fs::rename(&backup_path, destination);
                Err(error)
            }
        }
    }
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
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        if self.written >= self.limit {
            return Err(io::Error::other("OUTPUT_LIMIT_REACHED"));
        }
        let allowed = buffer.len().min(self.limit - self.written);
        let written = self.inner.write(&buffer[..allowed])?;
        self.written += written;
        if written < buffer.len() {
            return Err(io::Error::other("OUTPUT_LIMIT_REACHED"));
        }
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::{write_pretty_json, write_result_json};
    use serde_json::json;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temporary_path(name: &str) -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("visual-map-output-{name}-{suffix}"));
        fs::create_dir_all(&path).expect("출력 테스트 디렉터리를 만들어야 한다");
        path
    }

    #[test]
    fn 출력_한도_초과시_기존_정상_json을_보존한다() {
        let root = temporary_path("limit");
        let output = root.join("result.json");
        fs::write(&output, br#"{"previous":true}"#).expect("기존 결과를 써야 한다");

        let error = write_result_json(
            &output,
            &json!({"large": "this output must exceed the limit"}),
            false,
            8,
        )
        .expect_err("출력 한도에 도달해야 한다");

        assert!(error.to_string().contains("OUTPUT_LIMIT_REACHED"));
        assert_eq!(fs::read_to_string(&output).unwrap(), r#"{"previous":true}"#);
        assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
        fs::remove_dir_all(root).expect("출력 테스트 디렉터리를 정리해야 한다");
    }

    #[test]
    fn 성공한_json만_최종_경로에_교체한다() {
        let root = temporary_path("success");
        let output = root.join("result.json");
        write_pretty_json(&output, &json!({"ready": true})).expect("결과를 저장해야 한다");

        let parsed: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&output).unwrap()).unwrap();
        assert_eq!(parsed["ready"], true);
        assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
        fs::remove_dir_all(root).expect("출력 테스트 디렉터리를 정리해야 한다");
    }
}
