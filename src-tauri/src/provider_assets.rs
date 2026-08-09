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
const CATALOG_RECEIPT_FILE: &str = ".provider-catalog-receipt.json";
const PACK_RECEIPT_FILE: &str = ".provider-pack-receipt.json";
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

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProviderCatalog {
    schema_version: u32,
    catalog_version: String,
    key_id: String,
    platform: String,
    packs: Vec<ProviderPack>,
}

#[derive(Debug, Serialize, Deserialize)]
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

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProviderEntrypoint {
    path: String,
    sha256: String,
    bytes: u64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProviderCatalogReceipt {
    schema_version: u32,
    catalog_version: String,
    catalog_digest: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProviderPackReceipt {
    schema_version: u32,
    pack_id: String,
    pack_version: String,
    archive_digest: String,
    unpacked_bytes: u64,
}

pub(crate) fn resolve_provider_root(
    app_data_dir: &Path,
    engine_dir: &Path,
    required_languages: &BTreeSet<String>,
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
    activate_signed_bundles(app_data_dir, &expanded, required_languages, progress)
}

fn activate_signed_bundles(
    app_data_dir: &Path,
    bundle_root: &Path,
    required_languages: &BTreeSet<String>,
    progress: Option<&ProviderActivationProgress<'_>>,
) -> Result<PathBuf, String> {
    activate_signed_bundles_with_public_key(
        app_data_dir,
        bundle_root,
        required_languages,
        progress,
        PROVIDER_PUBLIC_KEY,
    )
}

fn activate_signed_bundles_with_public_key(
    app_data_dir: &Path,
    bundle_root: &Path,
    required_languages: &BTreeSet<String>,
    progress: Option<&ProviderActivationProgress<'_>>,
    public_key: &str,
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
    verify_catalog_signature_with_public_key(&catalog_bytes, signature.trim(), public_key)?;
    let catalog: ProviderCatalog = serde_json::from_slice(&catalog_bytes)
        .map_err(|error| format!("provider catalog 형식이 올바르지 않습니다: {error}"))?;
    validate_catalog(&catalog)?;
    let selected_packs = select_packs(&catalog, required_languages)?;
    let catalog_digest = lower_sha256_bytes(&catalog_bytes);
    let store_root = app_data_dir.join("managed-providers").join("v3");
    fs::create_dir_all(&store_root)
        .map_err(|error| format!("managed provider 폴더를 만들지 못했습니다: {error}"))?;
    let provider_root = ensure_catalog_root(&store_root, bundle_root, &catalog, &catalog_digest)?;
    let total = u64::try_from(selected_packs.len()).unwrap_or(u64::MAX);
    for (index, pack) in selected_packs.iter().enumerate() {
        if let Some(progress) = progress {
            progress(
                &format!("언어 분석 도구 준비 중 · {}", pack.id),
                u64::try_from(index).unwrap_or(u64::MAX),
                total.max(1),
            );
        }
        if pack.id == "core" {
            verify_activated_pack(&provider_root, pack)?;
        } else {
            ensure_language_pack(&provider_root, bundle_root, pack)?;
        }
    }
    verify_entrypoints(&provider_root, &selected_packs)?;
    if let Some(progress) = progress {
        progress("언어 분석 도구 준비 완료", total, total.max(1));
    }
    Ok(provider_root)
}

fn ensure_catalog_root(
    store_root: &Path,
    bundle_root: &Path,
    catalog: &ProviderCatalog,
    catalog_digest: &str,
) -> Result<PathBuf, String> {
    let core = catalog
        .packs
        .iter()
        .find(|pack| pack.id == "core")
        .ok_or_else(|| "provider core pack을 찾지 못했습니다".to_string())?;
    let target = store_root.join(format!(
        "{}-{}",
        catalog.catalog_version,
        &catalog_digest[..16]
    ));
    if target.is_dir() {
        verify_activated_catalog_root(&target, catalog, catalog_digest)?;
        verify_activated_pack(&target, core)?;
        return Ok(target);
    }

    let staging_path = store_root.join(format!(
        ".staging-catalog-{}-{}",
        std::process::id(),
        unix_millis()
    ));
    fs::create_dir(&staging_path)
        .map_err(|error| format!("provider catalog staging 폴더를 만들지 못했습니다: {error}"))?;
    let staging = ProviderStaging {
        path: staging_path,
        root: store_root.to_path_buf(),
    };
    let archive_path = bundle_root.join(&core.file_name);
    verify_archive(&archive_path, core)?;
    extract_archive(&archive_path, &staging.path, core)?;
    verify_core_layout(&staging.path)?;
    write_pack_receipt(&staging.path, core)?;
    write_synced_json(
        &staging.path.join(CATALOG_RECEIPT_FILE),
        &ProviderCatalogReceipt {
            schema_version: 3,
            catalog_version: catalog.catalog_version.clone(),
            catalog_digest: catalog_digest.to_string(),
        },
    )?;
    verify_entrypoints(&staging.path, &[core])?;

    if let Err(error) = fs::rename(&staging.path, &target) {
        if target.is_dir() {
            verify_activated_catalog_root(&target, catalog, catalog_digest)?;
            verify_activated_pack(&target, core)?;
            return Ok(target);
        }
        return Err(format!(
            "managed provider catalog를 게시하지 못했습니다: {error}"
        ));
    }
    Ok(target)
}

fn ensure_language_pack(
    provider_root: &Path,
    bundle_root: &Path,
    pack: &ProviderPack,
) -> Result<(), String> {
    let target = provider_root.join(&pack.id);
    if target.is_dir() {
        return verify_activated_pack(provider_root, pack);
    }

    let staging_path = provider_root.join(format!(
        ".staging-{}-{}-{}",
        pack.id,
        std::process::id(),
        unix_millis()
    ));
    fs::create_dir(&staging_path)
        .map_err(|error| format!("provider pack staging 폴더를 만들지 못했습니다: {error}"))?;
    let staging = ProviderStaging {
        path: staging_path,
        root: provider_root.to_path_buf(),
    };
    let archive_path = bundle_root.join(&pack.file_name);
    verify_archive(&archive_path, pack)?;
    extract_archive(&archive_path, &staging.path, pack)?;
    verify_language_pack_layout(&staging.path, pack)?;
    let staged_pack = staging.path.join(&pack.id);
    write_pack_receipt(&staged_pack, pack)?;
    verify_entrypoints(&staging.path, &[pack])?;

    if let Err(error) = fs::rename(&staged_pack, &target) {
        if target.is_dir() {
            verify_activated_pack(provider_root, pack)?;
            return Ok(());
        }
        return Err(format!(
            "managed provider pack을 게시하지 못했습니다: {error}"
        ));
    }
    Ok(())
}

fn select_packs<'a>(
    catalog: &'a ProviderCatalog,
    required_languages: &BTreeSet<String>,
) -> Result<Vec<&'a ProviderPack>, String> {
    let supported = EXPECTED_LANGUAGES
        .into_iter()
        .map(str::to_string)
        .collect::<BTreeSet<_>>();
    if !required_languages.is_subset(&supported) {
        let unsupported = required_languages
            .difference(&supported)
            .cloned()
            .collect::<Vec<_>>();
        return Err(format!(
            "지원하지 않는 source language가 provider 선택에 포함되었습니다: {}",
            unsupported.join(", ")
        ));
    }
    let selected = catalog
        .packs
        .iter()
        .filter(|pack| {
            pack.languages.is_empty()
                || pack
                    .languages
                    .iter()
                    .any(|language| required_languages.contains(language))
        })
        .collect::<Vec<_>>();
    if selected.is_empty() || selected.iter().all(|pack| !pack.languages.is_empty()) {
        return Err("provider core pack을 찾지 못했습니다".to_string());
    }
    Ok(selected)
}

fn verify_catalog_signature_with_public_key(
    catalog: &[u8],
    encoded_signature: &str,
    encoded_public_key: &str,
) -> Result<(), String> {
    let public_key: [u8; 32] = STANDARD
        .decode(encoded_public_key.trim())
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

fn verify_activated_catalog_root(
    root: &Path,
    catalog: &ProviderCatalog,
    catalog_digest: &str,
) -> Result<(), String> {
    let receipt: ProviderCatalogReceipt = serde_json::from_slice(
        &fs::read(root.join(CATALOG_RECEIPT_FILE))
            .map_err(|error| format!("provider activation receipt를 읽지 못했습니다: {error}"))?,
    )
    .map_err(|error| format!("provider activation receipt 형식 오류: {error}"))?;
    if receipt.schema_version != 3
        || receipt.catalog_version != catalog.catalog_version
        || receipt.catalog_digest != catalog_digest
    {
        return Err("기존 managed provider cache가 다른 catalog를 가리킵니다".to_string());
    }
    if !root.join("manifest.json").is_file() {
        return Err("provider manifest가 추출되지 않았습니다".to_string());
    }
    Ok(())
}

fn write_pack_receipt(root: &Path, pack: &ProviderPack) -> Result<(), String> {
    write_synced_json(
        &root.join(PACK_RECEIPT_FILE),
        &ProviderPackReceipt {
            schema_version: 1,
            pack_id: pack.id.clone(),
            pack_version: pack.version.clone(),
            archive_digest: pack.sha256.to_ascii_lowercase(),
            unpacked_bytes: pack.unpacked_bytes,
        },
    )
}

fn verify_activated_pack(root: &Path, pack: &ProviderPack) -> Result<(), String> {
    let pack_root = if pack.id == "core" {
        root.to_path_buf()
    } else {
        root.join(&pack.id)
    };
    let receipt: ProviderPackReceipt = serde_json::from_slice(
        &fs::read(pack_root.join(PACK_RECEIPT_FILE))
            .map_err(|error| format!("provider pack receipt를 읽지 못했습니다: {error}"))?,
    )
    .map_err(|error| format!("provider pack receipt 형식 오류: {error}"))?;
    if receipt.schema_version != 1
        || receipt.pack_id != pack.id
        || receipt.pack_version != pack.version
        || receipt.archive_digest != pack.sha256.to_ascii_lowercase()
        || receipt.unpacked_bytes != pack.unpacked_bytes
    {
        return Err(format!(
            "기존 managed provider pack이 catalog와 다릅니다: {}",
            pack.id
        ));
    }
    verify_entrypoints(root, &[pack])
}

fn verify_core_layout(root: &Path) -> Result<(), String> {
    if !root.join("manifest.json").is_file() {
        return Err("provider core pack에 manifest.json이 없습니다".to_string());
    }
    for entry in fs::read_dir(root)
        .map_err(|error| format!("provider core pack을 열거하지 못했습니다: {error}"))?
    {
        let entry =
            entry.map_err(|error| format!("provider core 항목을 읽지 못했습니다: {error}"))?;
        if entry
            .file_type()
            .map_err(|error| format!("provider core 항목 형식을 읽지 못했습니다: {error}"))?
            .is_dir()
        {
            return Err(
                "provider core pack은 language pack 디렉터리를 포함할 수 없습니다".to_string(),
            );
        }
    }
    Ok(())
}

fn verify_language_pack_layout(root: &Path, pack: &ProviderPack) -> Result<(), String> {
    let mut entries = fs::read_dir(root)
        .map_err(|error| format!("provider pack을 열거하지 못했습니다: {error}"))?;
    let entry = entries
        .next()
        .transpose()
        .map_err(|error| format!("provider pack 항목을 읽지 못했습니다: {error}"))?
        .ok_or_else(|| format!("provider pack이 비어 있습니다: {}", pack.id))?;
    if entries.next().is_some()
        || entry.file_name().to_str() != Some(pack.id.as_str())
        || !entry
            .file_type()
            .map_err(|error| format!("provider pack 항목 형식을 읽지 못했습니다: {error}"))?
            .is_dir()
    {
        return Err(format!(
            "provider pack은 자신의 단일 root만 포함해야 합니다: {}",
            pack.id
        ));
    }
    Ok(())
}

fn verify_entrypoints(root: &Path, packs: &[&ProviderPack]) -> Result<(), String> {
    for pack in packs {
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
    use ed25519_dalek::{Signer, SigningKey};
    use zip::{write::SimpleFileOptions, ZipWriter};

    fn write_test_pack(
        bundle_root: &Path,
        id: &str,
        languages: &[&str],
        files: &[(&str, &[u8])],
        entrypoint_path: &str,
    ) -> ProviderPack {
        let file_name = format!("providers-{id}.zip");
        let archive_path = bundle_root.join(&file_name);
        let mut writer = ZipWriter::new(File::create(&archive_path).unwrap());
        let mut unpacked_bytes = 0_u64;
        for (path, payload) in files {
            writer
                .start_file(*path, SimpleFileOptions::default())
                .unwrap();
            writer.write_all(payload).unwrap();
            unpacked_bytes += u64::try_from(payload.len()).unwrap();
        }
        writer.finish().unwrap();
        let entrypoint = files
            .iter()
            .find(|(path, _)| *path == entrypoint_path)
            .unwrap()
            .1;
        ProviderPack {
            id: id.to_string(),
            version: "test-version".to_string(),
            file_name,
            sha256: lower_sha256_file(&archive_path).unwrap(),
            compressed_bytes: fs::metadata(&archive_path).unwrap().len(),
            unpacked_bytes,
            languages: languages.iter().map(|value| (*value).to_string()).collect(),
            entrypoints: vec![ProviderEntrypoint {
                path: entrypoint_path.to_string(),
                sha256: lower_sha256_bytes(entrypoint),
                bytes: u64::try_from(entrypoint.len()).unwrap(),
            }],
        }
    }

    fn write_signed_test_bundle(bundle_root: &Path) -> (ProviderCatalog, String) {
        fs::create_dir_all(bundle_root).unwrap();
        let core = write_test_pack(
            bundle_root,
            "core",
            &[],
            &[("manifest.json", br#"{"providers":[]}"#)],
            "manifest.json",
        );
        let all = write_test_pack(
            bundle_root,
            "all",
            &EXPECTED_LANGUAGES,
            &[("all/tool.exe", b"test-provider")],
            "all/tool.exe",
        );
        let catalog = ProviderCatalog {
            schema_version: 2,
            catalog_version: "test-catalog".to_string(),
            key_id: "0123456789abcdef".to_string(),
            platform: "windows-x86_64".to_string(),
            packs: vec![core, all],
        };
        let catalog_bytes = serde_json::to_vec(&catalog).unwrap();
        let signing_key = SigningKey::from_bytes(&[7_u8; 32]);
        let signature = signing_key.sign(&catalog_bytes);
        let encoded_signature = STANDARD.encode(signature.to_bytes());
        let encoded_public_key = STANDARD.encode(signing_key.verifying_key().to_bytes());
        fs::write(bundle_root.join(CATALOG_FILE), catalog_bytes).unwrap();
        fs::write(bundle_root.join(SIGNATURE_FILE), encoded_signature).unwrap();
        (catalog, encoded_public_key)
    }

    fn catalog_pack(id: &str, languages: &[&str], entrypoint: &str) -> ProviderPack {
        ProviderPack {
            id: id.to_string(),
            version: "test-version".to_string(),
            file_name: format!("providers-{id}.zip"),
            sha256: "0".repeat(64),
            compressed_bytes: 1,
            unpacked_bytes: 1,
            languages: languages.iter().map(|value| (*value).to_string()).collect(),
            entrypoints: vec![ProviderEntrypoint {
                path: entrypoint.to_string(),
                sha256: "1".repeat(64),
                bytes: 1,
            }],
        }
    }

    fn provider_selection_catalog() -> ProviderCatalog {
        ProviderCatalog {
            schema_version: 2,
            catalog_version: "test-catalog".to_string(),
            key_id: "0123456789abcdef".to_string(),
            platform: "windows-x86_64".to_string(),
            packs: vec![
                catalog_pack("core", &[], "manifest.json"),
                catalog_pack(
                    "node",
                    &["typescript", "javascript", "python"],
                    "node/tool.exe",
                ),
                catalog_pack("java", &["java"], "java/tool.exe"),
                catalog_pack("dotnet", &["csharp"], "dotnet/tool.exe"),
                catalog_pack("clang", &["c", "cpp"], "clang/tool.exe"),
                catalog_pack("go", &["go"], "go/tool.exe"),
                catalog_pack("rust", &["rust"], "rust/tool.exe"),
                catalog_pack("dart", &["dart"], "dart/tool.exe"),
            ],
        }
    }

    #[test]
    fn signed_catalog_signature_and_contract_are_valid_without_release_assets() {
        let root = std::env::temp_dir().join(format!(
            "codebase-workspace-provider-catalog-{}-{}",
            std::process::id(),
            unix_millis()
        ));
        let (catalog, public_key) = write_signed_test_bundle(&root);
        let catalog_bytes = fs::read(root.join(CATALOG_FILE)).unwrap();
        let signature = fs::read_to_string(root.join(SIGNATURE_FILE)).unwrap();
        verify_catalog_signature_with_public_key(&catalog_bytes, signature.trim(), &public_key)
            .unwrap();
        validate_catalog(&catalog).unwrap();
        let mut tampered = catalog_bytes;
        tampered.push(b' ');
        assert!(
            verify_catalog_signature_with_public_key(&tampered, signature.trim(), &public_key)
                .is_err()
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn signed_core_pack_activates_once_into_the_shared_v3_store() {
        let root = std::env::temp_dir().join(format!(
            "codebase-workspace-provider-core-{}-{}",
            std::process::id(),
            unix_millis()
        ));
        let app_data = root.join("app-data");
        let bundle_root = root.join("bundles");
        let (_, public_key) = write_signed_test_bundle(&bundle_root);
        fs::create_dir_all(&app_data).unwrap();
        let required = BTreeSet::new();

        let first = activate_signed_bundles_with_public_key(
            &app_data,
            &bundle_root,
            &required,
            None,
            &public_key,
        )
        .unwrap();
        let second = activate_signed_bundles_with_public_key(
            &app_data,
            &bundle_root,
            &required,
            None,
            &public_key,
        )
        .unwrap();
        assert_eq!(first, second);
        assert_eq!(
            first
                .parent()
                .and_then(Path::file_name)
                .and_then(|name| name.to_str()),
            Some("v3")
        );
        assert!(first.join(CATALOG_RECEIPT_FILE).is_file());
        assert!(first.join(PACK_RECEIPT_FILE).is_file());
        assert!(first.join("manifest.json").is_file());

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn provider_activation_selects_core_and_only_packs_for_detected_languages() {
        let catalog = provider_selection_catalog();
        validate_catalog(&catalog).unwrap();
        let required = ["python".to_string(), "typescript".to_string()]
            .into_iter()
            .collect::<BTreeSet<_>>();
        let selected = select_packs(&catalog, &required).unwrap();
        let ids = selected
            .iter()
            .map(|pack| pack.id.as_str())
            .collect::<Vec<_>>();

        assert_eq!(ids, vec!["core", "node"]);
        assert!(select_packs(&catalog, &["ruby".to_string()].into_iter().collect()).is_err());
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
    fn language_combinations_reuse_one_append_only_catalog_store() {
        let root = std::env::temp_dir().join(format!(
            "codebase-workspace-provider-store-{}-{}",
            std::process::id(),
            unix_millis()
        ));
        let bundle_root = root.join("bundles");
        let store_root = root.join("store");
        fs::create_dir_all(&bundle_root).unwrap();
        fs::create_dir_all(&store_root).unwrap();
        let core = write_test_pack(
            &bundle_root,
            "core",
            &[],
            &[("manifest.json", br#"{"providers":[]}"#)],
            "manifest.json",
        );
        let node = write_test_pack(
            &bundle_root,
            "node",
            &["typescript"],
            &[("node/tool.exe", b"node-provider")],
            "node/tool.exe",
        );
        let java = write_test_pack(
            &bundle_root,
            "java",
            &["java"],
            &[("java/tool.exe", b"java-provider")],
            "java/tool.exe",
        );
        let catalog = ProviderCatalog {
            schema_version: 2,
            catalog_version: "test-catalog".to_string(),
            key_id: "0000000000000000".to_string(),
            platform: "windows-x86_64".to_string(),
            packs: vec![core, node, java],
        };
        let catalog_digest = "a".repeat(64);

        let provider_root =
            ensure_catalog_root(&store_root, &bundle_root, &catalog, &catalog_digest).unwrap();
        ensure_language_pack(&provider_root, &bundle_root, &catalog.packs[1]).unwrap();
        assert!(provider_root.join("node/tool.exe").is_file());
        assert!(!provider_root.join("java").exists());

        let reused =
            ensure_catalog_root(&store_root, &bundle_root, &catalog, &catalog_digest).unwrap();
        assert_eq!(reused, provider_root);
        ensure_language_pack(&reused, &bundle_root, &catalog.packs[2]).unwrap();
        ensure_language_pack(&reused, &bundle_root, &catalog.packs[1]).unwrap();
        assert!(provider_root.join("node/tool.exe").is_file());
        assert!(provider_root.join("java/tool.exe").is_file());
        assert_eq!(
            fs::read_dir(&store_root)
                .unwrap()
                .filter_map(Result::ok)
                .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
                .count(),
            1
        );

        fs::remove_dir_all(root).unwrap();
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
