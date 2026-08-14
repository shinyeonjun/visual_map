//! Claude CLI를 통해 의미 분석을 실행하는 provider.

mod process;

/// Claude CLI 의미 분석 provider의 기본 설정과 식별자다.
#[derive(Debug, Clone)]
pub struct ClaudeProvider {
    pub executable: String,
    pub model: Option<String>,
    pub timeout_ms: u64,
    pub max_input_bytes: usize,
}

#[derive(Debug)]
pub enum ClaudeError {
    Spawn(String),
    Io(String),
    Timeout,
    InputTooLarge {
        actual_bytes: usize,
        max_bytes: usize,
    },
    Process(String),
    InvalidResponse(String),
}

impl std::fmt::Display for ClaudeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Spawn(message) => write!(formatter, "Claude CLI 실행 실패: {message}"),
            Self::Io(message) => write!(formatter, "Claude CLI 입출력 실패: {message}"),
            Self::Timeout => write!(formatter, "Claude CLI 시간 제한 초과"),
            Self::InputTooLarge {
                actual_bytes,
                max_bytes,
            } => write!(
                formatter,
                "Claude CLI 입력이 너무 큽니다: {actual_bytes} bytes (최대 {max_bytes} bytes)"
            ),
            Self::Process(message) => write!(formatter, "Claude CLI가 실패했습니다: {message}"),
            Self::InvalidResponse(message) => {
                write!(formatter, "Claude 응답을 해석하지 못했습니다: {message}")
            }
        }
    }
}

impl std::error::Error for ClaudeError {}

impl crate::semantic::provider::SemanticProvider for ClaudeProvider {
    fn name(&self) -> &'static str {
        "claude"
    }
}
