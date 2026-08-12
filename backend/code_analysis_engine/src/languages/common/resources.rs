//! 설정 기반 외부 자원 접근 추출기.

use crate::config::{ResourceNameSource, ResourceRule};
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
                    .any(|pattern| callee_matches(pattern, &call.callee))
        }) else {
            continue;
        };
        let name = resource_name(call, rule);
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
            mode: effective_mode(&rule.mode, &call.callee),
            evidence: vec![evidence],
        });
    }
}

fn callee_matches(pattern: &str, callee: &str) -> bool {
    if pattern.eq_ignore_ascii_case(callee) {
        return true;
    }
    let Some((receiver_pattern, method_pattern)) = pattern.rsplit_once(['.', ':']) else {
        return false;
    };
    let Some((_, method)) = callee.rsplit_once(['.', ':']) else {
        return false;
    };
    // `Path.read_text`와 `path.read_text`처럼 타입명과 변수명이 다른
    // 인스턴스 메서드는 메서드와 알려진 receiver 계열이 일치하면 보존한다.
    matches!(
        receiver_pattern.to_ascii_lowercase().as_str(),
        "path" | "file" | "redis" | "cache"
    ) && method_pattern.eq_ignore_ascii_case(method)
}

fn resource_name(call: &CallSiteFact, rule: &ResourceRule) -> String {
    let argument_name = call
        .arguments
        .get(rule.argument_index)
        .and_then(|argument| string_literal(argument));
    match rule.name_source {
        ResourceNameSource::Argument => argument_name,
        ResourceNameSource::Receiver => call.receiver.clone(),
        ResourceNameSource::LiteralOrReceiver => argument_name.or_else(|| call.receiver.clone()),
    }
    .filter(|name| !name.trim().is_empty())
    .unwrap_or_else(|| "<dynamic>".to_string())
}

fn effective_mode(configured: &str, callee: &str) -> AccessMode {
    let configured = parse_mode(configured);
    let method = callee
        .rsplit_once(['.', ':'])
        .map(|(_, method)| method)
        .unwrap_or(callee)
        .to_ascii_lowercase();
    match method.as_str() {
        "get" | "read" | "read_text" | "read_to_string" | "exists" | "head" | "subscribe"
        | "consume" => AccessMode::Read,
        "set" | "write" | "write_text" | "save" | "delete" | "remove" | "insert" | "update"
        | "put" | "post" | "patch" | "publish" | "emit" | "send" | "hset" | "setex" => {
            AccessMode::Write
        }
        "request" | "open" | "connect" | "do" => AccessMode::ReadWrite,
        _ => configured,
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
        "websocket" | "web_socket" => Some(ResourceKind::WebSocket),
        "process" => Some(ResourceKind::Process),
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
