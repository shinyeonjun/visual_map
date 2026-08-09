use codebase_fact_model::analysis::{
    ProgrammingLanguage, ProviderDescriptor, ProviderOrigin, ProviderProtocol,
};
use codebase_fact_model::identity::Sha256Digest;
use codebase_fact_model::validation::Validate;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use crate::{resolve_tool, LANGUAGES};

static PROVIDER_DIGEST_CACHE: OnceLock<Mutex<HashMap<PathBuf, Sha256Digest>>> = OnceLock::new();

pub(super) fn resolve_provider_descriptor(
    language: ProgrammingLanguage,
    provider_label: &str,
    providers_root: Option<&Path>,
) -> Result<Option<ProviderDescriptor>, String> {
    let spec = LANGUAGES
        .iter()
        .find(|candidate| candidate.contract_language == language)
        .ok_or_else(|| format!("no provider specification for {}", language.as_str()))?;
    let (tool, protocol) = match provider_label {
        "scip" => (spec.tool, ProviderProtocol::Scip),
        "native-lsp" if matches!(language, ProgrammingLanguage::C | ProgrammingLanguage::Cpp) => {
            ("clangd", ProviderProtocol::LanguageServerProtocol)
        }
        "native-lsp" => (spec.tool, ProviderProtocol::LanguageServerProtocol),
        other => return Err(format!("unknown provider label for Language IR: {other}")),
    };
    let resolution = resolve_tool(tool, providers_root);
    let Some(path) = resolution.path.as_deref() else {
        return Ok(None);
    };
    let artifact_digest = digest_provider(path)?;
    if resolution
        .artifact_digest
        .is_some_and(|expected| expected != artifact_digest)
    {
        return Err(format!(
            "provider artifact digest does not match its managed catalog: {}",
            path.display()
        ));
    }
    let origin = match resolution.origin {
        "managed-manifest" | "managed-root" => ProviderOrigin::ManagedBundle,
        "path" => ProviderOrigin::SystemPath,
        other => return Err(format!("unsupported provider origin for {tool}: {other}")),
    };
    let descriptor = ProviderDescriptor {
        name: tool.to_string(),
        version: resolution.version,
        protocol,
        origin,
        artifact_digest,
    };
    descriptor
        .validate()
        .map_err(|error| format!("invalid provider descriptor for {tool}: {error}"))?;
    Ok(Some(descriptor))
}

fn digest_provider(path: &Path) -> Result<Sha256Digest, String> {
    let canonical = path.canonicalize().map_err(|error| {
        format!(
            "cannot resolve provider artifact {}: {error}",
            path.display()
        )
    })?;
    if let Some(digest) = PROVIDER_DIGEST_CACHE
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .map_err(|_| "provider digest cache is poisoned".to_string())?
        .get(&canonical)
        .copied()
    {
        return Ok(digest);
    }

    let file = File::open(&canonical).map_err(|error| {
        format!(
            "cannot open provider artifact {}: {error}",
            canonical.display()
        )
    })?;
    let mut reader = BufReader::with_capacity(64 * 1024, file);
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = reader.read(&mut buffer).map_err(|error| {
            format!(
                "cannot hash provider artifact {}: {error}",
                canonical.display()
            )
        })?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let digest = Sha256Digest::parse(&format!("{:x}", hasher.finalize()))
        .map_err(|error| format!("cannot encode provider digest: {error}"))?;
    PROVIDER_DIGEST_CACHE
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .map_err(|_| "provider digest cache is poisoned".to_string())?
        .insert(canonical, digest);
    Ok(digest)
}
