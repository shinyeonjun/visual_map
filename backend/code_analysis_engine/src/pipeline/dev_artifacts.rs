//! 개발 단계에서만 사용하는 Codex 입력 산출물 저장기.
//!
//! [DEV ONLY] 이 모듈은 운영 파이프라인의 프론트엔드 계약에 포함되지 않는다.
//! 제품 릴리스에서 중간 컨텍스트 덤프를 제거할 때 이 모듈과 CLI 옵션을 함께
//! 제거하거나 별도 진단 명령으로 이동한다.

use crate::semantic::context::SemanticContextArtifact;
use crate::EngineError;
use std::fs;
use std::path::Path;

pub(crate) fn serialize_artifact(
    artifact: &SemanticContextArtifact,
) -> Result<String, EngineError> {
    serde_json::to_string_pretty(artifact).map_err(EngineError::Serialization)
}

pub(crate) fn write_artifact(output_path: &Path, json: &str) -> Result<(), EngineError> {
    if let Some(parent) = output_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|source| EngineError::OutputWrite {
            path: parent.to_path_buf(),
            source,
        })?;
    }

    fs::write(output_path, json).map_err(|source| EngineError::OutputWrite {
        path: output_path.to_path_buf(),
        source,
    })
}
