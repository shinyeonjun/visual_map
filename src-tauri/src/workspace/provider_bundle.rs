use base64::{engine::general_purpose::STANDARD, Engine as _};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use fs2::FileExt;
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    sync::{Mutex, OnceLock},
};
use zip::ZipArchive;

const PROVIDER_CATALOG_SCHEMA_VERSION: u32 = 2;
const DEVELOPMENT_PUBLIC_KEY: &str = "IVL40Zt5HSRFMkLhXy6rbLfP+ntqXtMAl5YOBpiB2xI=";
const PACK_MARKER_SCHEMA: &str = "visual-map.provider-pack.v1";
static INSTALL_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProviderCatalog {
    schema_version: u32,
    catalog_version: String,
    key_id: String,
    platform: String,
    packs: Vec<ProviderPack>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProviderPack {
    id: String,
    version: String,
    file_name: String,
    sha256: String,
    compressed_bytes: u64,
    unpacked_bytes: u64,
    #[serde(default)]
    languages: Vec<String>,
    entrypoints: Vec<ProviderEntrypoint>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProviderEntrypoint {
    path: String,
    bytes: u64,
    sha256: String,
}

pub(crate) fn ensure_provider_root(
    engine_dir: &Path,
    cache_dir: &Path,
    required_languages: &[String],
) -> Result<Option<PathBuf>, String> {
    let bundle_dir = engine_dir.join("provider-bundles");
    let catalog_path = bundle_dir.join("providers-manifest.json");
    let signature_path = bundle_dir.join("providers-manifest.sig");
    let catalog_bytes = fs::read(&catalog_path).map_err(|error| {
        format!(
            "managed provider catalog를 읽지 못했습니다 ({}): {error}",
            catalog_path.display()
        )
    })?;
    verify_catalog_signature(
        &catalog_bytes,
        &fs::read_to_string(&signature_path).map_err(|error| {
            format!(
                "provider catalog 서명을 읽지 못했습니다 ({}): {error}",
                signature_path.display()
            )
        })?,
    )?;
    let catalog: ProviderCatalog = serde_json::from_slice(&catalog_bytes)
        .map_err(|error| format!("provider catalog 형식이 올바르지 않습니다: {error}"))?;
    validate_catalog(&catalog)?;
    let required = resolve_required_packs(&catalog, required_languages)?;
    let catalog_hash = sha256_bytes(&catalog_bytes);
    let root_id = catalog_hash[..16].to_ascii_lowercase();
    let roots_dir = cache_dir.join("provider-roots");
    let destination = roots_dir.join(root_id);

    let lock = INSTALL_LOCK.get_or_init(|| Mutex::new(()));
    let _lock = lock
        .lock()
        .map_err(|_| "provider 설치 잠금이 손상됐습니다".to_string())?;
    fs::create_dir_all(cache_dir)
        .map_err(|error| format!("provider 캐시 폴더를 만들지 못했습니다: {error}"))?;
    let process_lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(cache_dir.join("provider-install.lock"))
        .map_err(|error| format!("provider 설치 잠금 파일을 열지 못했습니다: {error}"))?;
    process_lock
        .try_lock_exclusive()
        .map_err(|error| format!("다른 앱 인스턴스가 provider를 준비 중입니다: {error}"))?;

    let packs = catalog
        .packs
        .iter()
        .map(|pack| (pack.id.as_str(), pack))
        .collect::<HashMap<_, _>>();
    for pack_id in required {
        let pack = packs
            .get(pack_id.as_str())
            .ok_or_else(|| format!("provider pack을 찾을 수 없습니다: {pack_id}"))?;
        ensure_pack(&bundle_dir, cache_dir, &destination, &catalog_hash, pack)?;
    }
    cleanup_legacy_provider_root(cache_dir);
    cleanup_old_catalog_roots(&roots_dir, &destination);
    Ok(Some(destination))
}

fn verify_catalog_signature(catalog: &[u8], encoded_signature: &str) -> Result<(), String> {
    let encoded_public_key =
        option_env!("VISUAL_MAP_PROVIDER_CATALOG_PUBLIC_KEY").unwrap_or(DEVELOPMENT_PUBLIC_KEY);
    let public_key: [u8; 32] = STANDARD
        .decode(encoded_public_key.trim())
        .map_err(|error| format!("provider public key가 올바르지 않습니다: {error}"))?
        .try_into()
        .map_err(|_| "provider public key 길이가 올바르지 않습니다".to_string())?;
    let signature = Signature::from_slice(
        &STANDARD
            .decode(encoded_signature.trim())
            .map_err(|error| format!("provider catalog 서명이 올바르지 않습니다: {error}"))?,
    )
    .map_err(|error| format!("provider catalog 서명이 올바르지 않습니다: {error}"))?;
    VerifyingKey::from_bytes(&public_key)
        .map_err(|error| format!("provider public key가 올바르지 않습니다: {error}"))?
        .verify(catalog, &signature)
        .map_err(|_| "provider catalog 서명이 일치하지 않습니다".to_string())
}

fn validate_catalog(catalog: &ProviderCatalog) -> Result<(), String> {
    if catalog.schema_version != PROVIDER_CATALOG_SCHEMA_VERSION {
        return Err(format!(
            "지원하지 않는 provider catalog 버전입니다: {}",
            catalog.schema_version
        ));
    }
    if !is_safe_id(&catalog.catalog_version) || !is_safe_id(&catalog.key_id) {
        return Err("provider catalog 식별자가 올바르지 않습니다".to_string());
    }
    let platform = format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH);
    if catalog.platform != platform {
        return Err(format!(
            "현재 플랫폼용 provider catalog가 아닙니다: {} (필요: {platform})",
            catalog.platform
        ));
    }
    let mut ids = HashSet::new();
    for pack in &catalog.packs {
        if !is_safe_id(&pack.id) || !ids.insert(pack.id.as_str()) {
            return Err(format!("provider pack ID가 올바르지 않습니다: {}", pack.id));
        }
        validate_archive_name(&pack.file_name)?;
        if pack.version.trim().is_empty()
            || !is_sha256(&pack.sha256)
            || pack.compressed_bytes == 0
            || pack.unpacked_bytes == 0
            || pack.entrypoints.is_empty()
        {
            return Err(format!(
                "provider pack 계약이 올바르지 않습니다: {}",
                pack.id
            ));
        }
        let mut entrypoints = HashSet::new();
        for entry in &pack.entrypoints {
            if !is_safe_relative_path(&entry.path)
                || !is_sha256(&entry.sha256)
                || !entrypoints.insert(entry.path.as_str())
            {
                return Err(format!(
                    "provider entrypoint가 올바르지 않습니다: {}",
                    entry.path
                ));
            }
            if pack.id != "core"
                && Path::new(&entry.path).components().next()
                    != Some(Component::Normal(pack.id.as_ref()))
            {
                return Err(format!(
                    "provider entrypoint가 pack 경계를 벗어났습니다: {}",
                    entry.path
                ));
            }
        }
    }
    if !ids.contains("core") {
        return Err("provider catalog에 core pack이 없습니다".to_string());
    }
    Ok(())
}

fn resolve_required_packs(
    catalog: &ProviderCatalog,
    required_languages: &[String],
) -> Result<Vec<String>, String> {
    let mut selected = vec!["core".to_string()];
    for language in required_languages {
        let matches = catalog
            .packs
            .iter()
            .filter(|pack| pack.languages.iter().any(|candidate| candidate == language))
            .map(|pack| pack.id.clone())
            .collect::<Vec<_>>();
        if matches.is_empty() {
            return Err(format!("{language}용 managed provider pack이 없습니다"));
        }
        for pack_id in matches {
            if !selected.contains(&pack_id) {
                selected.push(pack_id);
            }
        }
    }
    Ok(selected)
}

fn ensure_pack(
    bundle_dir: &Path,
    cache_dir: &Path,
    destination: &Path,
    catalog_hash: &str,
    pack: &ProviderPack,
) -> Result<(), String> {
    if pack_is_ready(destination, catalog_hash, pack) {
        return Ok(());
    }
    let archive = acquire_archive(bundle_dir, pack)?;
    let temporary = cache_dir.join(format!(
        "provider-pack.tmp-{}-{}",
        std::process::id(),
        pack.id
    ));
    if temporary.exists() {
        fs::remove_dir_all(&temporary)
            .map_err(|error| format!("이전 provider 임시 폴더를 지우지 못했습니다: {error}"))?;
    }
    fs::create_dir_all(&temporary)
        .map_err(|error| format!("provider 임시 폴더를 만들지 못했습니다: {error}"))?;
    let result = extract_pack(&archive, &temporary, pack)
        .and_then(|()| verify_entrypoints(&temporary, pack))
        .and_then(|()| install_pack(&temporary, destination, pack))
        .and_then(|()| write_pack_marker(destination, catalog_hash, pack))
        .and_then(|()| {
            pack_is_ready(destination, catalog_hash, pack)
                .then_some(())
                .ok_or_else(|| format!("provider pack 설치가 완전하지 않습니다: {}", pack.id))
        });
    let _ = fs::remove_dir_all(&temporary);
    result
}

fn acquire_archive(bundle_dir: &Path, pack: &ProviderPack) -> Result<PathBuf, String> {
    let bundled = bundle_dir.join(&pack.file_name);
    if !bundled.is_file() {
        return Err(format!("설치 파일에 provider pack이 없습니다: {}", pack.id));
    }
    archive_matches(&bundled, pack)
        .then_some(bundled)
        .ok_or_else(|| format!("설치된 provider pack 검증에 실패했습니다: {}", pack.id))
}

fn archive_matches(path: &Path, pack: &ProviderPack) -> bool {
    fs::metadata(path)
        .ok()
        .is_some_and(|metadata| metadata.is_file() && metadata.len() == pack.compressed_bytes)
        && sha256_file(path)
            .map(|hash| hash.eq_ignore_ascii_case(&pack.sha256))
            .unwrap_or(false)
}

fn extract_pack(archive: &Path, destination: &Path, pack: &ProviderPack) -> Result<(), String> {
    let file = File::open(archive)
        .map_err(|error| format!("provider 압축 파일을 열지 못했습니다: {error}"))?;
    let mut zip = ZipArchive::new(file)
        .map_err(|error| format!("provider 압축 파일을 읽지 못했습니다: {error}"))?;
    let mut unpacked = 0u64;
    let mut paths = HashSet::new();
    for index in 0..zip.len() {
        let mut entry = zip
            .by_index(index)
            .map_err(|error| format!("provider 압축 항목을 읽지 못했습니다: {error}"))?;
        let Some(relative) = entry.enclosed_name() else {
            return Err("provider 압축 파일에 허용되지 않은 경로가 있습니다".to_string());
        };
        if !paths.insert(relative.clone()) {
            return Err("provider 압축 파일에 중복 경로가 있습니다".to_string());
        }
        if pack.id != "core"
            && relative.components().next() != Some(Component::Normal(pack.id.as_ref()))
        {
            return Err(format!(
                "provider archive가 pack 경계를 벗어났습니다: {}",
                pack.id
            ));
        }
        unpacked = unpacked
            .checked_add(entry.size())
            .ok_or_else(|| "provider archive 크기가 올바르지 않습니다".to_string())?;
        if unpacked > pack.unpacked_bytes {
            return Err(format!(
                "provider archive 해제 크기가 catalog를 초과합니다: {}",
                pack.id
            ));
        }
        let output_path = destination.join(relative);
        if entry.is_dir() {
            fs::create_dir_all(&output_path)
                .map_err(|error| format!("provider 폴더를 만들지 못했습니다: {error}"))?;
            continue;
        }
        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("provider 상위 폴더를 만들지 못했습니다: {error}"))?;
        }
        let mut output = File::create(&output_path)
            .map_err(|error| format!("provider 파일을 만들지 못했습니다: {error}"))?;
        std::io::copy(&mut entry, &mut output)
            .map_err(|error| format!("provider 파일을 풀지 못했습니다: {error}"))?;
        output
            .flush()
            .map_err(|error| format!("provider 파일을 저장하지 못했습니다: {error}"))?;
    }
    if unpacked != pack.unpacked_bytes {
        return Err(format!(
            "provider archive 해제 크기가 catalog와 다릅니다: {}",
            pack.id
        ));
    }
    Ok(())
}

fn install_pack(temporary: &Path, destination: &Path, pack: &ProviderPack) -> Result<(), String> {
    if pack.id == "core" {
        if destination.exists() {
            fs::remove_dir_all(destination)
                .map_err(|error| format!("기존 provider core를 교체하지 못했습니다: {error}"))?;
        }
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("provider root를 만들지 못했습니다: {error}"))?;
        }
        return fs::rename(temporary, destination)
            .map_err(|error| format!("provider core를 활성화하지 못했습니다: {error}"));
    }
    fs::create_dir_all(destination)
        .map_err(|error| format!("provider root를 만들지 못했습니다: {error}"))?;
    let source = temporary.join(&pack.id);
    let target = destination.join(&pack.id);
    if target.exists() {
        fs::remove_dir_all(&target)
            .map_err(|error| format!("기존 provider pack을 교체하지 못했습니다: {error}"))?;
    }
    fs::rename(source, target)
        .map_err(|error| format!("provider pack을 활성화하지 못했습니다: {error}"))
}

fn pack_is_ready(destination: &Path, catalog_hash: &str, pack: &ProviderPack) -> bool {
    fs::read_to_string(pack_marker(destination, &pack.id))
        .map(|contents| contents == pack_marker_contents(catalog_hash, pack))
        .unwrap_or(false)
        && verify_entrypoints(destination, pack).is_ok()
}

fn verify_entrypoints(root: &Path, pack: &ProviderPack) -> Result<(), String> {
    for entry in &pack.entrypoints {
        let path = root.join(&entry.path);
        if !is_safe_cached_file(root, &entry.path)
            || fs::metadata(&path)
                .map(|metadata| metadata.len() != entry.bytes)
                .unwrap_or(true)
            || !sha256_file(&path)?.eq_ignore_ascii_case(&entry.sha256)
        {
            return Err(format!(
                "provider entrypoint 검증에 실패했습니다: {}",
                entry.path
            ));
        }
    }
    Ok(())
}

fn write_pack_marker(
    destination: &Path,
    catalog_hash: &str,
    pack: &ProviderPack,
) -> Result<(), String> {
    let marker = pack_marker(destination, &pack.id);
    if let Some(parent) = marker.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("provider marker 폴더를 만들지 못했습니다: {error}"))?;
    }
    fs::write(marker, pack_marker_contents(catalog_hash, pack))
        .map_err(|error| format!("provider marker를 저장하지 못했습니다: {error}"))
}

fn pack_marker(destination: &Path, pack_id: &str) -> PathBuf {
    destination.join(".packs").join(format!("{pack_id}.ready"))
}

fn pack_marker_contents(catalog_hash: &str, pack: &ProviderPack) -> String {
    format!(
        "{PACK_MARKER_SCHEMA}\n{catalog_hash}\n{}\n{}\n",
        pack.id, pack.sha256
    )
}

fn cleanup_legacy_provider_root(cache_dir: &Path) {
    let legacy = cache_dir.join("providers");
    if legacy.is_dir() {
        let _ = fs::remove_dir_all(legacy);
    }
}

fn cleanup_old_catalog_roots(roots_dir: &Path, current: &Path) {
    let Ok(entries) = fs::read_dir(roots_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path != current && path.is_dir() {
            let _ = fs::remove_dir_all(path);
        }
    }
}

fn validate_archive_name(name: &str) -> Result<(), String> {
    let path = Path::new(name);
    if name.is_empty()
        || path.file_name().and_then(|value| value.to_str()) != Some(name)
        || !name.ends_with(".zip")
        || name.chars().any(char::is_control)
    {
        return Err(format!("provider 압축 파일명이 올바르지 않습니다: {name}"));
    }
    Ok(())
}

fn is_safe_id(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
}

fn is_safe_cached_file(destination: &Path, relative: &str) -> bool {
    if !is_safe_relative_path(relative) || !is_real_file(&destination.join(relative)) {
        return false;
    }
    let Ok(root) = fs::canonicalize(destination) else {
        return false;
    };
    let Ok(path) = fs::canonicalize(destination.join(relative)) else {
        return false;
    };
    path.starts_with(root)
}

fn is_real_file(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|metadata| metadata.file_type().is_file())
        .unwrap_or(false)
}

fn is_safe_relative_path(value: &str) -> bool {
    let path = Path::new(value);
    !value.is_empty()
        && !path.is_absolute()
        && !value.chars().any(char::is_control)
        && path.components().all(|component| {
            !matches!(
                component,
                Component::Prefix(_) | Component::RootDir | Component::ParentDir
            )
        })
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file = File::open(path)
        .map_err(|error| format!("provider checksum 파일을 열지 못했습니다: {error}"))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("provider checksum을 읽지 못했습니다: {error}"))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:X}", hasher.finalize()))
}

fn sha256_bytes(bytes: &[u8]) -> String {
    format!("{:X}", Sha256::digest(bytes))
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use zip::{write::SimpleFileOptions, ZipWriter};

    #[test]
    fn installs_only_the_packs_required_by_the_provider_plan() {
        let root = test_root("selective");
        let bundle_dir = root.join("engines/provider-bundles");
        let cache_dir = root.join("cache");
        fs::create_dir_all(&bundle_dir).unwrap();
        let core = create_pack(&bundle_dir, "core", &[("manifest.json", b"{}")]);
        let node = create_pack(
            &bundle_dir,
            "node",
            &[
                ("node/project-model.cjs", b"model"),
                ("node/runtime/node.exe", b"node"),
            ],
        );
        let java = create_pack(
            &bundle_dir,
            "java",
            &[("java/runtime/bin/java.exe", b"java")],
        );
        write_catalog(&bundle_dir, vec![core, node, java]);

        let provider_root = ensure_provider_root(
            &root.join("engines"),
            &cache_dir,
            &["typescript".to_string()],
        )
        .unwrap()
        .unwrap();

        assert!(provider_root.join("manifest.json").is_file());
        assert!(provider_root.join("node/runtime/node.exe").is_file());
        assert!(!provider_root.join("java").exists());
        fs::write(provider_root.join("node/runtime/node.exe"), b"tampered").unwrap();
        ensure_provider_root(
            &root.join("engines"),
            &cache_dir,
            &["typescript".to_string()],
        )
        .unwrap();
        assert_eq!(
            fs::read(provider_root.join("node/runtime/node.exe")).unwrap(),
            b"node"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_tampered_catalog() {
        let root = test_root("trust");
        let bundle_dir = root.join("engines/provider-bundles");
        fs::create_dir_all(&bundle_dir).unwrap();
        let core = create_pack(&bundle_dir, "core", &[("manifest.json", b"{}")]);
        write_catalog(&bundle_dir, vec![core]);

        fs::write(bundle_dir.join("providers-manifest.json"), b"{}").unwrap();
        let error =
            ensure_provider_root(&root.join("engines"), &root.join("cache"), &[]).unwrap_err();
        assert!(error.contains("서명이 일치하지 않습니다"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_missing_bundled_pack_without_network_fallback() {
        let root = test_root("missing-bundle");
        let bundle_dir = root.join("engines/provider-bundles");
        fs::create_dir_all(&bundle_dir).unwrap();
        let core = create_pack(&bundle_dir, "core", &[("manifest.json", b"{}")]);
        let node = create_pack(&bundle_dir, "node", &[("node/runtime/node.exe", b"node")]);
        let node_archive = bundle_dir.join(&node.file_name);
        write_catalog(&bundle_dir, vec![core, node]);
        fs::remove_file(node_archive).unwrap();

        let error = ensure_provider_root(
            &root.join("engines"),
            &root.join("cache"),
            &["typescript".to_string()],
        )
        .unwrap_err();
        assert!(error.contains("설치 파일에 provider pack이 없습니다"));
        fs::remove_dir_all(root).unwrap();
    }

    struct TestPack {
        id: String,
        version: String,
        file_name: String,
        sha256: String,
        compressed_bytes: u64,
        unpacked_bytes: u64,
        entrypoints: Vec<serde_json::Value>,
    }

    fn create_pack(bundle_dir: &Path, id: &str, files: &[(&str, &[u8])]) -> TestPack {
        let file_name = format!("providers-{id}.zip");
        let path = bundle_dir.join(&file_name);
        let file = File::create(&path).unwrap();
        let mut zip = ZipWriter::new(file);
        let mut unpacked_bytes = 0;
        let mut entrypoints = Vec::new();
        for (name, contents) in files {
            zip.start_file(*name, SimpleFileOptions::default()).unwrap();
            zip.write_all(contents).unwrap();
            unpacked_bytes += contents.len() as u64;
            entrypoints.push(serde_json::json!({
                "path": name,
                "bytes": contents.len(),
                "sha256": sha256_bytes(contents)
            }));
        }
        zip.finish().unwrap();
        let sha256 = sha256_file(&path).unwrap();
        TestPack {
            id: id.to_string(),
            version: sha256[..16].to_string(),
            file_name,
            sha256,
            compressed_bytes: fs::metadata(path).unwrap().len(),
            unpacked_bytes,
            entrypoints,
        }
    }

    fn write_catalog(bundle_dir: &Path, packs: Vec<TestPack>) {
        let packs = packs
            .into_iter()
            .map(|pack| {
                let languages = match pack.id.as_str() {
                    "node" => vec!["typescript", "javascript", "python"],
                    "rust" => vec!["rust"],
                    "java" => vec!["java"],
                    _ => Vec::new(),
                };
                serde_json::json!({
                    "id": pack.id,
                    "version": pack.version,
                    "fileName": pack.file_name,
                    "sha256": pack.sha256,
                    "compressedBytes": pack.compressed_bytes,
                    "unpackedBytes": pack.unpacked_bytes,
                    "languages": languages,
                    "entrypoints": pack.entrypoints
                })
            })
            .collect::<Vec<_>>();
        let catalog = serde_json::to_vec(&serde_json::json!({
            "schemaVersion": PROVIDER_CATALOG_SCHEMA_VERSION,
            "catalogVersion": "test-v1",
            "keyId": "development-v1",
            "platform": format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
            "packs": packs
        }))
        .unwrap();
        let signature = SigningKey::from_bytes(&[0x42; 32]).sign(&catalog);
        fs::write(bundle_dir.join("providers-manifest.json"), &catalog).unwrap();
        fs::write(
            bundle_dir.join("providers-manifest.sig"),
            STANDARD.encode(signature.to_bytes()),
        )
        .unwrap();
    }

    fn test_root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "visual-map-provider-pack-{name}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        root
    }
}
