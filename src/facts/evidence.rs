use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// 원본 코드의 위치다.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SourceSpan {
    pub file_id: String,
    pub relative_path: String,
    pub start_line: u32,
    pub start_column: u32,
    pub end_line: u32,
    pub end_column: u32,
}

impl SourceSpan {
    pub fn new(
        file_id: impl Into<String>,
        relative_path: impl Into<String>,
        start_line: u32,
        start_column: u32,
        end_line: u32,
        end_column: u32,
    ) -> Self {
        Self {
            file_id: file_id.into(),
            relative_path: relative_path.into(),
            start_line,
            start_column,
            end_line,
            end_column,
        }
    }
}

/// 사실이나 도메인 판단을 원본 코드로 연결하는 근거다.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Evidence {
    /// Codex나 프론트엔드가 원본 근거를 안정적으로 참조하기 위한 ID다.
    pub id: String,
    pub kind: String,
    pub value: String,
    pub span: SourceSpan,
}

impl Evidence {
    pub fn new(kind: impl Into<String>, value: impl Into<String>, span: SourceSpan) -> Self {
        let kind = kind.into();
        let value = value.into();
        let id = stable_id(&kind, &value, &span);
        Self {
            id,
            kind,
            value,
            span,
        }
    }
}

fn stable_id(kind: &str, value: &str, span: &SourceSpan) -> String {
    let mut hasher = Sha256::new();
    hasher.update(kind.as_bytes());
    hasher.update([0]);
    hasher.update(value.as_bytes());
    hasher.update([0]);
    hasher.update(span.file_id.as_bytes());
    hasher.update([0]);
    hasher.update(span.relative_path.as_bytes());
    hasher.update([0]);
    hasher.update(span.start_line.to_le_bytes());
    hasher.update(span.start_column.to_le_bytes());
    hasher.update(span.end_line.to_le_bytes());
    hasher.update(span.end_column.to_le_bytes());
    let digest = hasher.finalize();
    let hex = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("evidence_{}", &hex[..24])
}
