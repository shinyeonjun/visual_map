use crate::{analysis::AnalysisCachePolicy, workspace};
use codebase_fact_model::identity::Sha256Digest;
use codebase_semantic_compiler::{CompiledSemanticPartition, VerifiedSemanticPartition};
use codebase_semantic_model::{ApprovedSemanticRevision, BaseSemanticPacket, RegionId};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

const RECORD_SCHEMA_VERSION: u32 = 1;
const PARTITION_CACHE_SCHEMA_VERSION: u32 = 1;
const POINTER_FILE: &str = "published-semantic-v1.json";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct StoredSemanticRevision {
    pub schema_version: u32,
    pub packet: BaseSemanticPacket,
    pub revision: ApprovedSemanticRevision,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SemanticPointer {
    schema_version: u32,
    snapshot_id: String,
    revision_id: String,
    record_file: String,
    record_digest: Sha256Digest,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredSemanticPartition {
    schema_version: u32,
    partition_key: String,
    region_ids: Vec<RegionId>,
    packet_digest: Sha256Digest,
    revision: ApprovedSemanticRevision,
}

pub(super) fn publish(
    app_data_dir: &Path,
    workspace_id: &str,
    packet: &BaseSemanticPacket,
    revision: &ApprovedSemanticRevision,
) -> Result<(), String> {
    if packet.snapshot_id != revision.snapshot_id
        || packet.semantic_input_digest != revision.semantic_input_digest
    {
        return Err("AI packet과 승인 revision identity가 다릅니다".to_string());
    }
    let workspace_dir = workspace::workspace_data_dir(app_data_dir, workspace_id)?;
    let revision_dir = workspace_dir.join("semantic-revisions");
    fs::create_dir_all(&revision_dir)
        .map_err(|error| format!("semantic revision 폴더 생성 실패: {error}"))?;
    let record = StoredSemanticRevision {
        schema_version: RECORD_SCHEMA_VERSION,
        packet: packet.clone(),
        revision: revision.clone(),
    };
    let record_bytes = serde_json::to_vec_pretty(&record)
        .map_err(|error| format!("semantic revision 직렬화 실패: {error}"))?;
    let record_digest = Sha256Digest::of_bytes(&record_bytes);
    // Revision identity intentionally excludes non-semantic diagnostics such
    // as warning wording. A fresh provider run may therefore produce a new
    // byte payload for the same semantic revision. Address the immutable
    // stored record by both identities so those honest variants never collide.
    let record_file = format!("{}-{}.json", revision.revision_id, record_digest);
    let record_path = revision_dir.join(&record_file);
    write_immutable_bytes(&record_path, &record_bytes)?;
    let pointer = SemanticPointer {
        schema_version: RECORD_SCHEMA_VERSION,
        snapshot_id: revision.snapshot_id.to_string(),
        revision_id: revision.revision_id.to_string(),
        record_file,
        record_digest,
    };
    replace_json_atomically(&workspace_dir.join(POINTER_FILE), &pointer)
}

pub(crate) fn load_current(
    app_data_dir: &Path,
    workspace_id: &str,
) -> Result<Option<StoredSemanticRevision>, String> {
    let workspace_dir = workspace::workspace_data_dir(app_data_dir, workspace_id)?;
    let pointer_path = workspace_dir.join(POINTER_FILE);
    if !pointer_path.is_file() {
        return Ok(None);
    }
    let pointer: SemanticPointer = serde_json::from_slice(
        &fs::read(&pointer_path).map_err(|error| format!("semantic pointer 읽기 실패: {error}"))?,
    )
    .map_err(|error| format!("semantic pointer 형식 오류: {error}"))?;
    if pointer.schema_version != RECORD_SCHEMA_VERSION {
        return Err("semantic pointer schema migration이 필요합니다".to_string());
    }
    validate_leaf_name(&pointer.record_file)?;
    let root = workspace_dir.join("semantic-revisions");
    let path = root.join(&pointer.record_file);
    let canonical_root = root
        .canonicalize()
        .map_err(|error| format!("semantic revision root 확인 실패: {error}"))?;
    let canonical_path = path
        .canonicalize()
        .map_err(|error| format!("semantic revision 경로 확인 실패: {error}"))?;
    if canonical_path.parent() != Some(canonical_root.as_path()) {
        return Err("semantic revision 경로가 workspace를 벗어났습니다".to_string());
    }
    let record_bytes = read_digest_verified(&canonical_path, pointer.record_digest)?;
    let record: StoredSemanticRevision = serde_json::from_slice(&record_bytes)
        .map_err(|error| format!("semantic revision 형식 오류: {error}"))?;
    if record.schema_version != RECORD_SCHEMA_VERSION
        || record.revision.snapshot_id.to_string() != pointer.snapshot_id
        || record.revision.revision_id.to_string() != pointer.revision_id
        || record.packet.snapshot_id != record.revision.snapshot_id
        || record.packet.semantic_input_digest != record.revision.semantic_input_digest
    {
        return Err("semantic pointer와 revision identity가 일치하지 않습니다".to_string());
    }
    Ok(Some(record))
}

pub(super) fn load_partition(
    app_data_dir: &Path,
    workspace_id: &str,
    partition: &CompiledSemanticPartition,
) -> Result<Option<VerifiedSemanticPartition>, String> {
    let path = partition_cache_path(app_data_dir, workspace_id, partition)?;
    if !path.is_file() {
        return Ok(None);
    }
    let bytes = fs::read(&path).map_err(|error| format!("의미 분할 cache 읽기 실패: {error}"))?;
    let record: StoredSemanticPartition = serde_json::from_slice(&bytes)
        .map_err(|error| format!("의미 분할 cache 형식 오류: {error}"))?;
    if record.schema_version != PARTITION_CACHE_SCHEMA_VERSION
        || record.partition_key != partition.partition_key
        || record.region_ids != partition.region_ids
        || record.packet_digest != partition.prompt.packet.semantic_input_digest
        || record.revision.snapshot_id != partition.prompt.packet.snapshot_id
        || record.revision.semantic_input_digest != partition.prompt.packet.semantic_input_digest
        || record.revision.provider.kind != partition.prompt.packet.provider.kind
        || record.revision.provider.model != partition.prompt.packet.provider.model
        || record.revision.provider.effort != partition.prompt.packet.provider.effort
        || record.revision.prompt_policy_version != partition.prompt.packet.prompt_policy_version
    {
        return Err("의미 분할 cache identity가 현재 분석 계약과 일치하지 않습니다".to_string());
    }
    Ok(Some(VerifiedSemanticPartition {
        partition_key: record.partition_key,
        region_ids: record.region_ids,
        packet_digest: record.packet_digest,
        revision: record.revision,
    }))
}

pub(super) fn cache_partition(
    app_data_dir: &Path,
    workspace_id: &str,
    partition: &CompiledSemanticPartition,
    verified: &VerifiedSemanticPartition,
    cache_policy: AnalysisCachePolicy,
) -> Result<(), String> {
    if verified.partition_key != partition.partition_key
        || verified.region_ids != partition.region_ids
        || verified.packet_digest != partition.prompt.packet.semantic_input_digest
        || verified.revision.snapshot_id != partition.prompt.packet.snapshot_id
        || verified.revision.semantic_input_digest != partition.prompt.packet.semantic_input_digest
    {
        return Err("검증된 의미 분할과 cache 대상 packet identity가 다릅니다".to_string());
    }
    let path = partition_cache_path(app_data_dir, workspace_id, partition)?;
    let parent = path
        .parent()
        .ok_or_else(|| "의미 분할 cache 상위 경로가 없습니다".to_string())?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("의미 분할 cache 폴더 생성 실패: {error}"))?;
    let record = StoredSemanticPartition {
        schema_version: PARTITION_CACHE_SCHEMA_VERSION,
        partition_key: partition.partition_key.clone(),
        region_ids: partition.region_ids.clone(),
        packet_digest: partition.prompt.packet.semantic_input_digest,
        revision: verified.revision.clone(),
    };
    if cache_policy == AnalysisCachePolicy::Fresh {
        replace_json_atomically(&path, &record)
    } else {
        let bytes = serde_json::to_vec_pretty(&record)
            .map_err(|error| format!("의미 분할 cache 직렬화 실패: {error}"))?;
        write_immutable_bytes(&path, &bytes)
    }
}

fn partition_cache_path(
    app_data_dir: &Path,
    workspace_id: &str,
    partition: &CompiledSemanticPartition,
) -> Result<PathBuf, String> {
    let workspace_dir = workspace::workspace_data_dir(app_data_dir, workspace_id)?;
    let digest = partition.prompt.packet.semantic_input_digest.to_hex();
    Ok(workspace_dir
        .join("semantic-partition-cache-v1")
        .join(format!("{digest}.json")))
}

fn write_immutable_bytes(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if path.is_file() {
        let existing =
            fs::read(path).map_err(|error| format!("기존 semantic revision 읽기 실패: {error}"))?;
        return (existing == bytes)
            .then_some(())
            .ok_or_else(|| "동일 revision ID의 payload가 다릅니다".to_string());
    }
    let temporary = path.with_extension(format!("json.next-{}", std::process::id()));
    write_synced(&temporary, bytes)?;
    fs::rename(&temporary, path).map_err(|error| format!("semantic revision 게시 실패: {error}"))
}

fn read_digest_verified(path: &Path, expected: Sha256Digest) -> Result<Vec<u8>, String> {
    let bytes = fs::read(path).map_err(|error| format!("semantic revision 읽기 실패: {error}"))?;
    if Sha256Digest::of_bytes(&bytes) != expected {
        return Err("semantic revision SHA-256이 pointer와 일치하지 않습니다".to_string());
    }
    Ok(bytes)
}

fn replace_json_atomically(path: &Path, value: &impl Serialize) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| format!("semantic JSON 직렬화 실패: {error}"))?;
    let temporary = path.with_extension("json.next");
    let previous = path.with_extension("json.previous");
    write_synced(&temporary, &bytes)?;
    if path.is_file() {
        if previous.is_file() {
            fs::remove_file(&previous)
                .map_err(|error| format!("이전 semantic JSON 정리 실패: {error}"))?;
        }
        fs::rename(path, &previous)
            .map_err(|error| format!("semantic JSON backup 실패: {error}"))?;
    }
    if let Err(error) = fs::rename(&temporary, path) {
        if previous.is_file() {
            let _ = fs::rename(&previous, path);
        }
        return Err(format!("semantic JSON 게시 실패: {error}"));
    }
    if previous.is_file() {
        let _ = fs::remove_file(&previous);
    }
    Ok(())
}

fn write_synced(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(path)
        .map_err(|error| format!("semantic 파일 staging 실패: {error}"))?;
    file.write_all(bytes)
        .map_err(|error| format!("semantic 파일 쓰기 실패: {error}"))?;
    file.sync_all()
        .map_err(|error| format!("semantic 파일 fsync 실패: {error}"))
}

fn validate_leaf_name(value: &str) -> Result<(), String> {
    let path = PathBuf::from(value);
    if value.is_empty()
        || value.len() > 192
        || value.chars().any(char::is_control)
        || path.file_name().and_then(|name| name.to_str()) != Some(value)
    {
        return Err("semantic revision 파일 이름이 올바르지 않습니다".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod replacement_tests {
    use super::*;

    #[test]
    fn atomic_json_replacement_refreshes_a_derivative_cache() {
        let root = std::env::temp_dir().join(format!(
            "codebase-workspace-semantic-cache-replace-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let path = root.join("partition.json");

        replace_json_atomically(&path, &serde_json::json!({ "value": "old" })).unwrap();
        replace_json_atomically(&path, &serde_json::json!({ "value": "fresh" })).unwrap();

        let value: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(value["value"], "fresh");
        assert!(!path.with_extension("json.previous").exists());
        fs::remove_dir_all(root).unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn immutable_revision_bytes_are_repeatable_and_tamper_evident() {
        let root = std::env::temp_dir().join(format!(
            "codebase-workspace-semantic-store-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let path = root.join("revision.json");
        let bytes = br#"{"revision":"same-input-same-output"}"#;
        let digest = Sha256Digest::of_bytes(bytes);

        write_immutable_bytes(&path, bytes).unwrap();
        write_immutable_bytes(&path, bytes).unwrap();
        assert_eq!(read_digest_verified(&path, digest).unwrap(), bytes);
        assert!(write_immutable_bytes(&path, b"different").is_err());

        fs::write(&path, b"tampered").unwrap();
        assert!(read_digest_verified(&path, digest)
            .unwrap_err()
            .contains("SHA-256"));
        fs::remove_dir_all(root).unwrap();
    }
}
