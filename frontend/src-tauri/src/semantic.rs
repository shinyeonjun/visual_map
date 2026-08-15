use crate::models::{SemanticDomain, SemanticResult};
use std::fs;
use std::path::Path;

pub(crate) fn load_semantic_domains(path: &Path) -> Vec<SemanticDomain> {
    if !path.is_file() {
        return Vec::new();
    }
    fs::read_to_string(path)
        .ok()
        .and_then(|contents| serde_json::from_str::<SemanticResult>(&contents).ok())
        .map(|result| result.domains)
        .unwrap_or_default()
}

pub(crate) fn load_semantic_domains_or_error(path: &Path) -> Result<Vec<SemanticDomain>, String> {
    if !path.is_file() {
        return Ok(Vec::new());
    }
    let contents = fs::read_to_string(path)
        .map_err(|error| format!("semantic 결과를 읽지 못했습니다: {error}"))?;
    let semantic: SemanticResult = serde_json::from_str(&contents)
        .map_err(|error| format!("semantic 결과 형식이 올바르지 않습니다: {error}"))?;
    Ok(semantic.domains)
}
