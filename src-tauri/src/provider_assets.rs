//! Verified, one-time activation of the signed language-provider archives.
//!
//! Installers carry providers as signed ZIP packs to avoid shipping tens of
//! thousands of loose files.  The desktop verifies the catalog signature,
//! every archive digest, safe archive paths, declared unpacked byte counts,
//! and all provider entry points before atomically publishing one immutable
//! provider directory under app data.

use base64::{engine::general_purpose::STANDARD, Engine as _};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    sync::{Mutex, OnceLock},
    time::{SystemTime, UNIX_EPOCH},
};
use zip::ZipArchive;

const CATALOG_FILE: &str = "providers-manifest.json";
const SIGNATURE_FILE: &str = "providers-manifest.sig";
const RECEIPT_FILE: &str = ".provider-catalog-receipt.json";
const EXPECTED_LANGUAGES: [&str; 10] = [
    "c",
    "cpp",
    "csharp",
    "dart",
    "go",
    "java",
    "javascript",
    "python",
    "rust",
    "typescript",
];
const MAX_PACKS: usize = 32;
const MAX_ARCHIVE_ENTRIES: usize = 200_000;
const MAX_TOTAL_UNPACKED_BYTES: u64 = 8 * 1024 * 1024 * 1024;
const PROVIDER_PUBLIC_KEY: &str = env!("CODEBASE_WORKSPACE_PROVIDER_CATALOG_PUBLIC_KEY");
static ACTIVATION_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
type ProviderActivationProgress<'a> = dyn Fn(&str, u64, u64) + 'a;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProviderCatalog {
    schema_version: u32,
    catalog_version: String,
    key_id: String,
    platform: String,
    packs: Vec<ProviderPack>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProviderPack {
    id: String,
    version: String,
    file_name: String,
    sha256: String,
    compressed_bytes: u64,
    unpacked_bytes: u64,
    languages: Vec<String>,
    entrypoints: Vec<ProviderEntrypoint>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProviderEntrypoint {
    path: String,
    sha256: String,
    bytes: u64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProviderReceipt {
    schema_version: u32,
    catalog_version: String,
    catalog_digest: String,
}

pub(crate) fn resolve_provider_root(
    app_data_dir: &Path,
    engine_dir: &Path,
    progress: Option<&ProviderActivationProgress<'_>>,
) -> Result<PathBuf, String> {
    let expanded = engine_dir.join("provider-bundles");
    if expanded.join("manifest.json").is_file() {
        return Ok(expanded);
    }
    #[cfg(any(debug_assertions, codebase_workspace_internal_build))]
    {
        let source_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .ok_or_else(|| "개발 source root를 계산하지 못했습니다".to_string())?
            .join("code_memory/providers");
        if source_root.join("manifest.json").is_file() {
            return Ok(source_root);
        }
    }
    activate_signed_bundles(app_data_dir, &expanded, progress)
}

fn activate_signed_bundles(
    app_data_dir: &Path,
    bundle_root: &Path,
    progress: Option<&ProviderActivationProgress<'_>>,
) -> Result<PathBuf, String> {
    let _guard = ACTIVATION_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| "provider activation lock이 손상되었습니다".to_string())?;
    let catalog_path = bundle_root.join(CATALOG_FILE);
    let signature_path = bundle_root.join(SIGNATURE_FILE);
    let catalog_bytes = fs::read(&catalog_path)
        .map_err(|error| format!("provider catalog를 읽지 못했습니다: {error}"))?;
    let signature = fs::read_to_string(&signature_path)
        .map_err(|error| format!("provider catalog signature를 읽지 못했습니다: {error}"))?;
    verify_catalog_signature(&catalog_bytes, signature.trim())?;
    let catalog: ProviderCatalog = serde_json::from_slice(&catalog_bytes)
        .map_err(|error| format!("provider catalog 형식이 올바르지 않습니다: {error}"))?;
    validate_catalog(&catalog)?;
    let catalog_digest = lower_sha256_bytes(&catalog_bytes);
    let target_name = format!("{}-{}", catalog.catalog_version, &catalog_digest[..16]);
    let provider_root = app_data_dir.join("managed-providers").join("v2");
    fs::create_dir_all(&provider_root)
        .map_err(|error| format!("managed provider 폴더를 만들지 못했습니다: {error}"))?;
    let target = provider_root.join(target_name);
    if target.is_dir() {
        verify_activated_root(&target, &catalog, &catalog_digest)?;
        return Ok(target);
    }

    let staging_path =
        provider_root.join(format!(".staging-{}-{}", std::process::id(), unix_millis()));
    fs::create_dir(&staging_path)
        .map_err(|error| format!("provider staging 폴더를 만들지 못했습니다: {error}"))?;
    let staging = ProviderStaging {
        path: staging_path,
        root: provider_root,
    };
    let total = u64::try_from(catalog.packs.len()).unwrap_or(u64::MAX);
    for (index, pack) in catalog.packs.iter().enumerate() {
        if let Some(progress) = progress {
            progress(
                &format!("언어 분석 도구 준비 중 · {}", pack.id),
                u64::try_from(index).unwrap_or(u64::MAX),
                total.max(1),
            );
        }
        let archive_path = bundle_root.join(&pack.file_name);
        verify_archive(&archive_path, pack)?;
        extract_archive(&archive_path, &staging.path, pack)?;
    }
    verify_entrypoints(&staging.path, &catalog)?;
    let receipt = ProviderReceipt {
        schema_version: 1,
        catalog_version: catalog.catalog_version.clone(),
        catalog_digest: catalog_digest.clone(),
    };
    write_synced_json(&staging.path.join(RECEIPT_FILE), &receipt)?;
    if let Err(error) = fs::rename(&staging.path, &target) {
        if target.is_dir() {
            verify_activated_root(&target, &catalog, &catalog_digest)?;
            return Ok(target);
        }
        return Err(format!("managed provider를 게시하지 못했습니다: {error}"));
    }
    if let Some(progress) = progress {
        progress("언어 분석 도구 준비 완료", total, total.max(1));
    }
    Ok(target)
}

fn verify_catalog_signature(catalog: &[u8], encoded_signature: &str) -> Result<(), String> {
    let public_key: [u8; 32] = STANDARD
        .decode(PROVIDER_PUBLIC_KEY.trim())
        .map_err(|error| format!("provider public key base64 오류: {error}"))?
        .try_into()
        .map_err(|_| "provider public key 길이가 올바르지 않습니다".to_string())?;
    let signature = Signature::from_slice(
        &STANDARD
            .decode(encoded_signature)
            .map_err(|error| format!("provider signature base64 오류: {error}"))?,
    )
    .map_err(|error| format!("provider signature 형식 오류: {error}"))?;
    VerifyingKey::from_bytes(&public_key)
        .map_err(|error| format!("provider public key 형식 오류: {error}"))?
        .verify(catalog, &signature)
        .map_err(|_| "provider catalog signature가 일치하지 않습니다".to_string())
}

fn validate_catalog(catalog: &ProviderCatalog) -> Result<(), String> {
    if catalog.schema_version != 2
        || catalog.platform != "windows-x86_64"
        || !safe_token(&catalog.catalog_version)
        || catalog.key_id.len() != 16
        || !catalog.key_id.bytes().all(|byte| byte.is_ascii_hexdigit())
        || catalog.packs.is_empty()
        || catalog.packs.len() > MAX_PACKS
    {
        return Err("provider catalog 계약이 올바르지 않습니다".to_string());
    }
    let mut pack_ids = BTreeSet::new();
    let mut archive_names = BTreeSet::new();
    let mut languages = BTreeSet::new();
    let mut entrypoints = BTreeSet::new();
    let mut total_unpacked = 0_u64;
    for pack in &catalog.packs {
        if !safe_token(&pack.id)
            || !safe_token(&pack.version)
            || !safe_leaf(&pack.file_name, ".zip")
            || !valid_sha256(&pack.sha256)
            || pack.compressed_bytes == 0
            || pack.unpacked_bytes == 0
            || !pack_ids.insert(pack.id.clone())
            || !archive_names.insert(pack.file_name.to_ascii_lowercase())
        {
            return Err(format!(
                "provider pack 계약이 올바르지 않습니다: {}",
                pack.id
            ));
        }
        total_unpacked = total_unpacked
            .checked_add(pack.unpacked_bytes)
            .ok_or_else(|| "provider unpacked byte 합계가 넘쳤습니다".to_string())?;
        for language in &pack.languages {
            if !safe_token(language) || !languages.insert(language.clone()) {
                return Err(format!(
                    "provider language가 중복되거나 올바르지 않습니다: {language}"
                ));
            }
        }
        for entrypoint in &pack.entrypoints {
            if checked_relative_path(&entrypoint.path).is_err()
                || !valid_sha256(&entrypoint.sha256)
                || entrypoint.bytes == 0
                || !entrypoints.insert(entrypoint.path.to_ascii_lowercase())
            {
                return Err(format!(
                    "provider entrypoint 계약이 올바르지 않습니다: {}",
                    entrypoint.path
                ));
            }
        }
    }
    if total_unpacked > MAX_TOTAL_UNPACKED_BYTES
        || languages
            != EXPECTED_LANGUAGES
                .into_iter()
                .map(str::to_string)
                .collect::<BTreeSet<_>>()
    {
        return Err("provider catalog의 언어 범위 또는 크기 계약이 다릅니다".to_string());
    }
    Ok(())
}

fn verify_archive(path: &Path, pack: &ProviderPack) -> Result<(), String> {
    let metadata = fs::metadata(path)
        .map_err(|error| format!("provider archive를 확인하지 못했습니다: {error}"))?;
    if !metadata.is_file() || metadata.len() != pack.compressed_bytes {
        return Err(format!(
            "provider archive 크기가 다릅니다: {}",
            path.display()
        ));
    }
    if lower_sha256_file(path)? != pack.sha256.to_ascii_lowercase() {
        return Err(format!(
            "provider archive SHA-256이 다릅니다: {}",
            path.display()
        ));
    }
    Ok(())
}

fn extract_archive(path: &Path, destination: &Path, pack: &ProviderPack) -> Result<(), String> {
    let file =
        File::open(path).map_err(|error| format!("provider archive를 열지 못했습니다: {error}"))?;
    let mut archive = ZipArchive::new(file)
        .map_err(|error| format!("provider ZIP 형식이 올바르지 않습니다: {error}"))?;
    if archive.len() > MAX_ARCHIVE_ENTRIES {
        return Err(format!(
            "provider ZIP 항목 수가 한도를 넘었습니다: {}",
            pack.id
        ));
    }
    let mut seen = BTreeSet::new();
    let mut unpacked = 0_u64;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| format!("provider ZIP 항목을 읽지 못했습니다: {error}"))?;
        if entry.encrypted()
            || entry
                .unix_mode()
                .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err("암호화 또는 symlink provider ZIP 항목은 허용되지 않습니다".to_string());
        }
        let enclosed = entry
            .enclosed_name()
            .ok_or_else(|| "provider ZIP에 경로 이탈 항목이 있습니다".to_string())?;
        let relative = checked_relative_path(
            enclosed
                .to_str()
                .ok_or_else(|| "provider ZIP 경로가 UTF-8이 아닙니다".to_string())?,
        )?;
        if !seen.insert(relative.to_string_lossy().to_ascii_lowercase()) {
            return Err("provider ZIP에 중복 경로가 있습니다".to_string());
        }
        if entry.is_dir() {
            fs::create_dir_all(destination.join(relative))
                .map_err(|error| format!("provider 폴더 추출 실패: {error}"))?;
            continue;
        }
        unpacked = unpacked
            .checked_add(entry.size())
            .filter(|value| *value <= pack.unpacked_bytes)
            .ok_or_else(|| "provider ZIP이 선언된 unpacked 크기를 넘었습니다".to_string())?;
        let output = destination.join(relative);
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("provider 추출 폴더 생성 실패: {error}"))?;
        }
        let mut target = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&output)
            .map_err(|error| format!("provider 파일 추출 실패: {error}"))?;
        let written = std::io::copy(&mut entry, &mut target)
            .map_err(|error| format!("provider 파일 쓰기 실패: {error}"))?;
        if written != entry.size() {
            return Err("provider ZIP 항목 크기가 중앙 디렉터리와 다릅니다".to_string());
        }
    }
    if unpacked != pack.unpacked_bytes {
        return Err(format!(
            "provider ZIP unpacked 크기가 다릅니다: {}",
            pack.id
        ));
    }
    Ok(())
}

fn verify_activated_root(
    root: &Path,
    catalog: &ProviderCatalog,
    catalog_digest: &str,
) -> Result<(), String> {
    let receipt: ProviderReceipt = serde_json::from_slice(
        &fs::read(root.join(RECEIPT_FILE))
            .map_err(|error| format!("provider activation receipt를 읽지 못했습니다: {error}"))?,
    )
    .map_err(|error| format!("provider activation receipt 형식 오류: {error}"))?;
    if receipt.schema_version != 1
        || receipt.catalog_version != catalog.catalog_version
        || receipt.catalog_digest != catalog_digest
    {
        return Err("기존 managed provider cache가 다른 catalog를 가리킵니다".to_string());
    }
    verify_entrypoints(root, catalog)
}

fn verify_entrypoints(root: &Path, catalog: &ProviderCatalog) -> Result<(), String> {
    for pack in &catalog.packs {
        for entrypoint in &pack.entrypoints {
            let relative = checked_relative_path(&entrypoint.path)?;
            let path = root.join(relative);
            let metadata = fs::metadata(&path)
                .map_err(|error| format!("provider entrypoint를 확인하지 못했습니다: {error}"))?;
            if !metadata.is_file()
                || metadata.len() != entrypoint.bytes
                || lower_sha256_file(&path)? != entrypoint.sha256.to_ascii_lowercase()
            {
                return Err(format!(
                    "provider entrypoint 검증 실패: {}",
                    entrypoint.path
                ));
            }
        }
    }
    if !root.join("manifest.json").is_file() {
        return Err("provider manifest가 추출되지 않았습니다".to_string());
    }
    Ok(())
}

fn checked_relative_path(value: &str) -> Result<PathBuf, String> {
    if value.is_empty() || value.len() > 512 || value.chars().any(char::is_control) {
        return Err("provider 상대 경로가 올바르지 않습니다".to_string());
    }
    let path = Path::new(value);
    if path.is_absolute()
        || !path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
    {
        return Err("provider 상대 경로가 root를 벗어납니다".to_string());
    }
    Ok(path.to_path_buf())
}

fn safe_token(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn safe_leaf(value: &str, suffix: &str) -> bool {
    value.ends_with(suffix)
        && Path::new(value).file_name().and_then(|name| name.to_str()) == Some(value)
        && !value.chars().any(char::is_control)
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn lower_sha256_file(path: &Path) -> Result<String, String> {
    let mut file = File::open(path)
        .map_err(|error| format!("SHA-256 대상 파일을 열지 못했습니다: {error}"))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 1024 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("SHA-256 대상 파일을 읽지 못했습니다: {error}"))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn lower_sha256_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn write_synced_json(path: &Path, value: &impl Serialize) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| format!("provider receipt 직렬화 실패: {error}"))?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("provider receipt 생성 실패: {error}"))?;
    file.write_all(&bytes)
        .and_then(|_| file.sync_all())
        .map_err(|error| format!("provider receipt 저장 실패: {error}"))
}

struct ProviderStaging {
    path: PathBuf,
    root: PathBuf,
}

impl Drop for ProviderStaging {
    fn drop(&mut self) {
        if self.path.parent() == Some(self.root.as_path())
            && self
                .path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(".staging-"))
        {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

fn unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use zip::{write::SimpleFileOptions, ZipWriter};

    #[test]
    fn bundled_development_catalog_signature_and_contract_are_valid() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("engines/provider-bundles");
        let catalog_bytes = fs::read(root.join(CATALOG_FILE)).unwrap();
        let signature = fs::read_to_string(root.join(SIGNATURE_FILE)).unwrap();
        verify_catalog_signature(&catalog_bytes, signature.trim()).unwrap();
        let catalog: ProviderCatalog = serde_json::from_slice(&catalog_bytes).unwrap();
        validate_catalog(&catalog).unwrap();
    }

    #[test]
    fn provider_paths_fail_closed() {
        for invalid in ["", "../escape.exe", "/absolute.exe", "a/../../b", "a\u{0}b"] {
            assert!(checked_relative_path(invalid).is_err());
        }
        assert_eq!(
            checked_relative_path("node/runtime/node.exe").unwrap(),
            PathBuf::from("node/runtime/node.exe")
        );
    }

    #[test]
    fn a_bounded_zip_extracts_but_a_path_escape_does_not() {
        let root = std::env::temp_dir().join(format!(
            "codebase-workspace-provider-zip-{}-{}",
            std::process::id(),
            unix_millis()
        ));
        fs::create_dir_all(&root).unwrap();
        let valid_zip = root.join("valid.zip");
        let payload = b"provider executable";
        {
            let mut writer = ZipWriter::new(File::create(&valid_zip).unwrap());
            writer
                .start_file("node/bin/provider.exe", SimpleFileOptions::default())
                .unwrap();
            writer.write_all(payload).unwrap();
            writer.finish().unwrap();
        }
        let pack = ProviderPack {
            id: "node".to_string(),
            version: "test".to_string(),
            file_name: "valid.zip".to_string(),
            sha256: lower_sha256_file(&valid_zip).unwrap(),
            compressed_bytes: fs::metadata(&valid_zip).unwrap().len(),
            unpacked_bytes: u64::try_from(payload.len()).unwrap(),
            languages: vec!["typescript".to_string()],
            entrypoints: Vec::new(),
        };
        let output = root.join("output");
        fs::create_dir(&output).unwrap();
        extract_archive(&valid_zip, &output, &pack).unwrap();
        assert_eq!(
            fs::read(output.join("node/bin/provider.exe")).unwrap(),
            payload
        );

        let escape_zip = root.join("escape.zip");
        {
            let mut writer = ZipWriter::new(File::create(&escape_zip).unwrap());
            writer
                .start_file("../escape.exe", SimpleFileOptions::default())
                .unwrap();
            writer.write_all(b"escape").unwrap();
            writer.finish().unwrap();
        }
        let escape_pack = ProviderPack {
            id: "escape".to_string(),
            version: "test".to_string(),
            file_name: "escape.zip".to_string(),
            sha256: lower_sha256_file(&escape_zip).unwrap(),
            compressed_bytes: fs::metadata(&escape_zip).unwrap().len(),
            unpacked_bytes: 6,
            languages: Vec::new(),
            entrypoints: Vec::new(),
        };
        assert!(extract_archive(&escape_zip, &output, &escape_pack).is_err());
        assert!(!root.join("escape.exe").exists());
        fs::remove_dir_all(root).unwrap();
    }
}
