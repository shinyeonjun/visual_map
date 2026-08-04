use fs2::FileExt;
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};
use zip::ZipArchive;

const PROVIDER_BUNDLE_SCHEMA_VERSION: u32 = 1;
const PROVIDER_CACHE_MARKER: &str = ".visual-map-provider-bundle";

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProviderBundleManifest {
    schema_version: u32,
    archives: Vec<ProviderArchive>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProviderArchive {
    file_name: String,
    sha256: String,
}

#[derive(Debug, serde::Deserialize)]
struct ExtractedProviderManifest {
    providers: Vec<ExtractedProvider>,
}

#[derive(Debug, serde::Deserialize)]
struct ExtractedProvider {
    path: String,
}

#[derive(Debug, serde::Deserialize)]
struct ExtractedProviderChecksums {
    #[serde(default)]
    files: Vec<ExtractedProviderChecksum>,
}

#[derive(Debug, serde::Deserialize)]
struct ExtractedProviderChecksum {
    path: String,
    bytes: u64,
    sha256: String,
}

static EXTRACTION_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

pub(crate) fn ensure_provider_root(
    engine_dir: &Path,
    cache_dir: &Path,
) -> Result<Option<PathBuf>, String> {
    let manifest_path = engine_dir
        .join("provider-bundles")
        .join("providers-manifest.json");
    if !manifest_path.is_file() {
        return Err(format!(
            "managed provider bundle manifest가 없습니다: {}",
            manifest_path.display()
        ));
    }

    let manifest_bytes = fs::read(&manifest_path)
        .map_err(|error| format!("provider manifest를 읽지 못했습니다: {error}"))?;
    let manifest: ProviderBundleManifest = serde_json::from_slice(&manifest_bytes)
        .map_err(|error| format!("provider manifest 형식이 올바르지 않습니다: {error}"))?;
    if manifest.schema_version != PROVIDER_BUNDLE_SCHEMA_VERSION {
        return Err(format!(
            "지원하지 않는 provider manifest 버전입니다: {}",
            manifest.schema_version
        ));
    }
    if manifest.archives.is_empty() {
        return Err("provider manifest에 압축 파일이 없습니다".to_string());
    }
    for archive in &manifest.archives {
        validate_archive_name(&archive.file_name)?;
        if !is_sha256(&archive.sha256) {
            return Err(format!(
                "provider checksum이 올바르지 않습니다: {}",
                archive.file_name
            ));
        }
    }

    let manifest_hash = sha256_bytes(&manifest_bytes);
    let destination = cache_dir.join("providers");
    let marker = destination.join(PROVIDER_CACHE_MARKER);
    if cached_provider_root_is_usable(&destination, &marker, &manifest_hash) {
        return Ok(Some(destination));
    }

    let lock = EXTRACTION_LOCK.get_or_init(|| Mutex::new(()));
    let _lock = lock
        .lock()
        .map_err(|_| "provider 압축 해제 잠금이 손상됐습니다".to_string())?;
    fs::create_dir_all(cache_dir)
        .map_err(|error| format!("provider 캐시 폴더를 만들지 못했습니다: {error}"))?;
    let process_lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(cache_dir.join("provider-extraction.lock"))
        .map_err(|error| format!("provider 압축 해제 잠금 파일을 열지 못했습니다: {error}"))?;
    process_lock
        .try_lock_exclusive()
        .map_err(|error| format!("다른 앱 인스턴스가 provider 압축을 해제 중입니다: {error}"))?;
    if cached_provider_root_is_usable(&destination, &marker, &manifest_hash) {
        return Ok(Some(destination));
    }

    let temporary = cache_dir.join(format!("providers.tmp-{}", std::process::id()));
    if temporary.exists() {
        fs::remove_dir_all(&temporary)
            .map_err(|error| format!("이전 provider 임시 폴더를 정리하지 못했습니다: {error}"))?;
    }
    fs::create_dir_all(&temporary)
        .map_err(|error| format!("provider 임시 폴더를 만들지 못했습니다: {error}"))?;

    let result = extract_archives(
        engine_dir.join("provider-bundles").as_path(),
        &manifest.archives,
        &temporary,
    )
    .and_then(|()| {
        let checksum_hash = sha256_file(&temporary.join("checksums.json"))?;
        fs::write(
            temporary.join(PROVIDER_CACHE_MARKER),
            cache_marker_contents(&manifest_hash, &checksum_hash),
        )
        .map_err(|error| format!("provider 캐시 검증 표식을 저장하지 못했습니다: {error}"))
    })
    .and_then(|()| {
        let temporary_marker = temporary.join(PROVIDER_CACHE_MARKER);
        cached_provider_root_is_usable(&temporary, &temporary_marker, &manifest_hash)
            .then_some(())
            .ok_or_else(|| "압축 해제된 provider bundle이 완전하지 않습니다".to_string())
    })
    .and_then(|()| {
        if destination.exists() {
            fs::remove_dir_all(&destination)
                .map_err(|error| format!("기존 provider 캐시를 교체하지 못했습니다: {error}"))?;
        }
        fs::rename(&temporary, &destination)
            .map_err(|error| format!("provider 캐시를 활성화하지 못했습니다: {error}"))
    });
    if let Err(error) = result {
        let _ = fs::remove_dir_all(&temporary);
        return Err(error);
    }
    Ok(Some(destination))
}

fn cached_provider_root_is_usable(destination: &Path, marker: &Path, manifest_hash: &str) -> bool {
    if !destination.is_dir() {
        return false;
    }

    for relative in [
        "manifest.json",
        "checksums.json",
        "node/project-model.cjs",
        "node/runtime/node.exe",
    ] {
        if !is_safe_cached_file(destination, relative) {
            return false;
        }
    }

    let Ok(bytes) = fs::read(destination.join("manifest.json")) else {
        return false;
    };
    let Ok(manifest) = serde_json::from_slice::<ExtractedProviderManifest>(&bytes) else {
        return false;
    };
    if manifest.providers.is_empty()
        || !manifest
            .providers
            .iter()
            .all(|provider| is_safe_cached_file(destination, &provider.path))
    {
        return false;
    }

    let Ok(checksum_bytes) = fs::read(destination.join("checksums.json")) else {
        return false;
    };
    let checksum_hash = sha256_bytes(&checksum_bytes);
    if fs::read_to_string(marker)
        .map(|value| !cache_marker_matches(&value, manifest_hash, &checksum_hash))
        .unwrap_or(true)
    {
        return false;
    }
    let Ok(checksums) = serde_json::from_slice::<ExtractedProviderChecksums>(&checksum_bytes)
    else {
        return false;
    };
    let mut checksum_paths = HashSet::new();
    !checksums.files.is_empty()
        && checksums.files.iter().all(|entry| {
            if !is_safe_relative_path(&entry.path) || !is_sha256(&entry.sha256) {
                return false;
            }
            if !checksum_paths.insert(entry.path.clone()) {
                return false;
            }
            let path = destination.join(&entry.path);
            if !is_real_file(&path) {
                return false;
            }
            let Ok(metadata) = fs::metadata(&path) else {
                return false;
            };
            metadata.len() == entry.bytes
                && sha256_file(&path)
                    .map(|hash| hash.eq_ignore_ascii_case(&entry.sha256))
                    .unwrap_or(false)
        })
        && [
            "manifest.json",
            "node/project-model.cjs",
            "node/runtime/node.exe",
        ]
        .into_iter()
        .chain(
            manifest
                .providers
                .iter()
                .map(|provider| provider.path.as_str()),
        )
        .all(|path| checksum_paths.contains(path))
}

fn cache_marker_contents(manifest_hash: &str, checksum_hash: &str) -> String {
    format!("{manifest_hash}\n{checksum_hash}\n")
}

fn cache_marker_matches(value: &str, manifest_hash: &str, checksum_hash: &str) -> bool {
    let mut lines = value.lines();
    lines
        .next()
        .is_some_and(|line| line.trim() == manifest_hash)
        && lines
            .next()
            .is_some_and(|line| line.trim() == checksum_hash)
        && lines.next().is_none()
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
                std::path::Component::Prefix(_)
                    | std::path::Component::RootDir
                    | std::path::Component::ParentDir
            )
        })
}

fn extract_archives(
    bundle_dir: &Path,
    archives: &[ProviderArchive],
    destination: &Path,
) -> Result<(), String> {
    for archive in archives {
        let archive_path = bundle_dir.join(&archive.file_name);
        if !archive_path.is_file() {
            return Err(format!(
                "provider 압축 파일이 없습니다: {}",
                archive.file_name
            ));
        }
        let actual_hash = sha256_file(&archive_path)?;
        if !actual_hash.eq_ignore_ascii_case(&archive.sha256) {
            return Err(format!(
                "provider 압축 파일 checksum이 일치하지 않습니다: {}",
                archive.file_name
            ));
        }

        let file = File::open(&archive_path)
            .map_err(|error| format!("provider 압축 파일을 열지 못했습니다: {error}"))?;
        let mut zip = ZipArchive::new(file)
            .map_err(|error| format!("provider 압축 파일을 읽지 못했습니다: {error}"))?;
        for index in 0..zip.len() {
            let mut entry = zip
                .by_index(index)
                .map_err(|error| format!("provider 압축 항목을 읽지 못했습니다: {error}"))?;
            let Some(relative_path) = entry.enclosed_name() else {
                return Err("provider 압축 파일에 허용되지 않은 경로가 있습니다".to_string());
            };
            let output_path = destination.join(relative_path);
            if entry.is_dir() {
                fs::create_dir_all(&output_path)
                    .map_err(|error| format!("provider 폴더를 풀지 못했습니다: {error}"))?;
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
    }
    Ok(())
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
    use zip::{write::SimpleFileOptions, ZipWriter};

    #[test]
    fn missing_bundle_manifest_fails_closed() {
        let root = std::env::temp_dir().join(format!(
            "visual-map-provider-bundle-missing-test-{}",
            std::process::id()
        ));
        let error = ensure_provider_root(&root.join("engines"), &root.join("cache"))
            .expect_err("a managed provider run must not silently fall back");

        assert!(error.contains("provider bundle manifest"));
    }

    #[test]
    fn extracts_a_verified_provider_bundle_once() {
        let root = std::env::temp_dir().join(format!(
            "visual-map-provider-bundle-test-{}",
            std::process::id()
        ));
        let bundle_dir = root.join("engines").join("provider-bundles");
        let cache_dir = root.join("cache");
        fs::create_dir_all(&bundle_dir).unwrap();
        let archive_path = bundle_dir.join("providers-test.zip");
        let file = File::create(&archive_path).unwrap();
        let mut zip = ZipWriter::new(file);
        let manifest_bytes: &[u8] = br#"{"providers":[{"path":"test/project-model.txt"}]}"#;
        let checksums = serde_json::json!({
            "schema": "code-memory.provider-checksums.v1",
            "files": [
                { "path": "manifest.json", "bytes": manifest_bytes.len(), "sha256": sha256_bytes(manifest_bytes) },
                { "path": "node/project-model.cjs", "bytes": 5, "sha256": sha256_bytes(b"model") },
                { "path": "node/runtime/node.exe", "bytes": 4, "sha256": sha256_bytes(b"node") },
                { "path": "test/project-model.txt", "bytes": 8, "sha256": sha256_bytes(b"provider") }
            ]
        });
        let checksums_bytes = serde_json::to_vec(&checksums).unwrap();
        zip.start_file("test/project-model.txt", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(b"provider").unwrap();
        for (path, contents) in [
            ("manifest.json", manifest_bytes),
            ("checksums.json", checksums_bytes.as_slice()),
            ("node/project-model.cjs", &b"model"[..]),
            ("node/runtime/node.exe", &b"node"[..]),
        ] {
            zip.start_file(path, SimpleFileOptions::default()).unwrap();
            zip.write_all(contents).unwrap();
        }
        zip.finish().unwrap();

        let archive_hash = sha256_file(&archive_path).unwrap();
        let manifest = serde_json::json!({
            "schemaVersion": PROVIDER_BUNDLE_SCHEMA_VERSION,
            "archives": [{
                "fileName": "providers-test.zip",
                "sha256": archive_hash
            }]
        });
        fs::write(
            bundle_dir.join("providers-manifest.json"),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();

        let provider_root = ensure_provider_root(&root.join("engines"), &cache_dir)
            .unwrap()
            .unwrap();
        assert_eq!(
            fs::read(provider_root.join("test/project-model.txt")).unwrap(),
            b"provider"
        );
        assert_eq!(
            ensure_provider_root(&root.join("engines"), &cache_dir)
                .unwrap()
                .unwrap(),
            provider_root
        );

        fs::write(provider_root.join("node/project-model.cjs"), b"tampered").unwrap();
        fs::remove_file(provider_root.join("node/runtime/node.exe")).unwrap();
        let restored = ensure_provider_root(&root.join("engines"), &cache_dir)
            .unwrap()
            .unwrap();
        assert_eq!(
            fs::read(restored.join("node/project-model.cjs")).unwrap(),
            b"model"
        );
        assert!(is_real_file(&restored.join("node/runtime/node.exe")));

        fs::write(restored.join("checksums.json"), b"{}").unwrap();
        let restored_after_checksum_tamper =
            ensure_provider_root(&root.join("engines"), &cache_dir)
                .unwrap()
                .unwrap();
        assert_eq!(
            fs::read(restored_after_checksum_tamper.join("test/project-model.txt")).unwrap(),
            b"provider"
        );

        fs::remove_dir_all(root).unwrap();
    }
}
