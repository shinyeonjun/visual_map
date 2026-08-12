//! 설정 기반 외부 자원 접근 추출기.

use crate::config::ResourceRule;
use crate::facts::{AccessMode, CallSiteFact, Evidence, FactBundle, ResourceAccess, ResourceKind};
use crate::languages::common::metadata::stable_id;
use crate::model::{FileEntry, Language};

pub(super) fn extract(
    language: Language,
    call_sites: &[CallSiteFact],
    file: &FileEntry,
    bundle: &mut FactBundle,
    rules: &[ResourceRule],
) {
    let language_key = language.key();
    for call in call_sites {
        let Some(rule) = rules.iter().find(|rule| {
            rule.languages.iter().any(|key| key == language_key)
                && rule
                    .callee_patterns
                    .iter()
                    .any(|pattern| pattern.eq_ignore_ascii_case(&call.callee))
        }) else {
            continue;
        };
        let name = call
            .arguments
            .get(rule.argument_index)
            .and_then(|argument| string_literal(argument))
            .unwrap_or_else(|| "<dynamic>".to_string());
        let Some(kind) = parse_kind(&rule.kind) else {
            continue;
        };
        let id = stable_id("resource", &format!("{}:{:?}:{}", call.id, kind, name));
        if bundle.resources.iter().any(|resource| resource.id == id) {
            continue;
        }
        let evidence = call
            .evidence
            .first()
            .cloned()
            .map(|mut evidence| {
                evidence.kind = "resourceCall".to_string();
                evidence.value = name.clone();
                evidence
            })
            .unwrap_or_else(|| {
                Evidence::new(
                    "resourceCall",
                    name.clone(),
                    crate::facts::SourceSpan::new(
                        file.file_id.clone(),
                        file.relative_path.clone(),
                        1,
                        1,
                        1,
                        1,
                    ),
                )
            });
        bundle.resources.push(ResourceAccess {
            id,
            unit_id: call.source_unit_id.clone(),
            kind,
            name,
            mode: parse_mode(&rule.mode),
            evidence: vec![evidence],
        });
    }
}

fn string_literal(value: &str) -> Option<String> {
    let value = value.trim();
    if value.len() < 2 {
        return None;
    }
    let first = value.chars().next()?;
    let last = value.chars().last()?;
    if !matches!((first, last), ('"', '"') | ('\'', '\'') | ('`', '`')) {
        return None;
    }
    Some(value[1..value.len() - 1].to_string())
}

fn parse_kind(value: &str) -> Option<ResourceKind> {
    match value.to_ascii_lowercase().as_str() {
        "table" => Some(ResourceKind::Table),
        "collection" => Some(ResourceKind::Collection),
        "cache" => Some(ResourceKind::Cache),
        "externalapi" | "external_api" => Some(ResourceKind::ExternalApi),
        "network" => Some(ResourceKind::Network),
        "environment" | "env" => Some(ResourceKind::Environment),
        "eventtopic" | "event_topic" => Some(ResourceKind::EventTopic),
        "file" => Some(ResourceKind::File),
        "unknown" => Some(ResourceKind::Unknown),
        _ => None,
    }
}

fn parse_mode(value: &str) -> AccessMode {
    match value.to_ascii_lowercase().as_str() {
        "read" => AccessMode::Read,
        "write" => AccessMode::Write,
        "readwrite" | "read_write" => AccessMode::ReadWrite,
        _ => AccessMode::Unknown,
    }
}
