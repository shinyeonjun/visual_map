//! 설정 기반 외부 자원 접근 추출기.

use crate::config::{ResourceNameSource, ResourceRule};
use crate::facts::{
    AccessMode, BindingKind, CallSiteFact, Evidence, FactBundle, ResourceAccess, ResourceKind,
};
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
        let resolved_callee = resolve_imported_callee(bundle, call);
        let Some(rule) = rules.iter().find(|rule| {
            rule.languages.iter().any(|key| key == language_key)
                && rule.callee_patterns.iter().any(|pattern| {
                    callee_matches(pattern, &resolved_callee.value)
                        && (!rule.requires_import
                            || is_unqualified_pattern(pattern)
                            || resolved_callee.imported)
                })
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
            mode: effective_mode(&rule.mode, &resolved_callee.value),
            evidence: vec![evidence],
        });
    }
}

#[derive(Debug, Clone)]
struct ResolvedCallee {
    value: String,
    imported: bool,
}

/// 호출 receiver 또는 함수 이름의 import binding을 적용한다.
///
/// `disk.readFile()`와 `slurp()`처럼 로컬 alias로 호출된 표준 API도
/// 원래 모듈의 resource 규칙에 연결한다. 반대로 import 근거가 없는
/// `fs.readFile()`은 사용자 객체일 수 있으므로 qualified resource로
/// 확정하지 않는다.
fn resolve_imported_callee(bundle: &FactBundle, call: &CallSiteFact) -> ResolvedCallee {
    let head = call_head(&call.callee);
    let Some(target) = unique_import_target(bundle, &call.source_unit_id, head) else {
        return ResolvedCallee {
            value: call.callee.clone(),
            imported: false,
        };
    };
    let suffix = call.callee.strip_prefix(head).unwrap_or_default();
    let target = target.replace("::", ".").trim_end_matches(".*").to_string();
    let target = target
        .strip_suffix(".default")
        .unwrap_or(&target)
        .to_string();
    ResolvedCallee {
        value: format!("{target}{suffix}"),
        imported: true,
    }
}

fn unique_import_target(
    bundle: &FactBundle,
    source_unit_id: &str,
    local_name: &str,
) -> Option<String> {
    let source_file_id = bundle
        .units
        .iter()
        .find(|unit| unit.id == source_unit_id)
        .map(|unit| unit.file_id.as_str());
    let mut targets = bundle
        .bindings
        .iter()
        .filter(|binding| {
            matches!(binding.kind, BindingKind::Import | BindingKind::ImportAlias)
                && binding.local_name == local_name
                && (binding.source_unit_id == source_unit_id
                    || source_file_id.is_some_and(|file_id| {
                        bundle
                            .units
                            .iter()
                            .find(|unit| unit.id == binding.source_unit_id)
                            .is_some_and(|unit| unit.file_id == file_id)
                    }))
        })
        .map(|binding| binding.target_name.clone())
        .collect::<Vec<_>>();
    targets.sort();
    targets.dedup();
    (targets.len() == 1).then(|| targets.remove(0))
}

fn call_head(callee: &str) -> &str {
    callee
        .split_once('.')
        .map(|(head, _)| head)
        .or_else(|| callee.split_once("::").map(|(head, _)| head))
        .or_else(|| callee.split_once("->").map(|(head, _)| head))
        .unwrap_or(callee)
}

fn is_unqualified_pattern(pattern: &str) -> bool {
    !pattern.contains('.') && !pattern.contains("::") && !pattern.contains("->")
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
    if !method_pattern.eq_ignore_ascii_case(method) {
        return false;
    }
    let receiver = callee
        .rsplit_once(['.', ':'])
        .map(|(value, _)| value)
        .and_then(|value| {
            value
                .rsplit_once(['.', ':'])
                .map(|(_, name)| name)
                .or(Some(value))
        })
        .unwrap_or_default()
        .to_ascii_lowercase();
    match receiver_pattern.to_ascii_lowercase().as_str() {
        // `Path.read_text`와 `path.read_text`처럼 표준 파일 receiver의
        // 타입명/변수명만 허용한다. 임의의 `customer.read_text()`를 파일
        // 자원으로 올리면 일반 도메인 메서드가 자원으로 오인된다.
        "path" | "file" => matches!(receiver.as_str(), "path" | "file"),
        _ => false,
    }
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

#[cfg(test)]
mod tests {
    use super::extract;
    use crate::config::{ResourceNameSource, ResourceRule};
    use crate::facts::{BindingKind, CallSiteFact, FactBundle, SymbolBinding};
    use crate::model::{FileEntry, Language, ParseStatus};

    fn file() -> FileEntry {
        FileEntry {
            file_id: "file".to_string(),
            relative_path: "src/file.ts".to_string(),
            language: Language::TypeScript,
            size_bytes: 1,
            line_count: 1,
            modified_unix_ms: None,
            content_hash: None,
            is_test: false,
            parse_status: ParseStatus::NotAnalyzed,
        }
    }

    fn call(callee: &str) -> CallSiteFact {
        CallSiteFact {
            id: format!("call-{callee}"),
            source_unit_id: "source".to_string(),
            callee: callee.to_string(),
            receiver: callee
                .rsplit_once('.')
                .map(|(receiver, _)| receiver.to_string()),
            arguments: vec!["\"data.txt\"".to_string()],
            assigned_name: None,
            evidence: Vec::new(),
        }
    }

    fn file_rule(pattern: &str) -> ResourceRule {
        ResourceRule {
            languages: vec!["typescript".to_string()],
            callee_patterns: vec![pattern.to_string()],
            kind: "file".to_string(),
            mode: "read".to_string(),
            argument_index: 0,
            name_source: ResourceNameSource::Argument,
            requires_import: true,
        }
    }

    #[test]
    fn import_alias의_파일_api는_원래_모듈로_정규화된다() {
        let mut bundle = FactBundle {
            bindings: vec![SymbolBinding {
                id: "binding-disk".to_string(),
                source_unit_id: "source".to_string(),
                local_name: "disk".to_string(),
                target_name: "fs::*".to_string(),
                kind: BindingKind::ImportAlias,
                evidence: Vec::new(),
            }],
            ..FactBundle::default()
        };

        extract(
            Language::TypeScript,
            &[call("disk.readFile")],
            &file(),
            &mut bundle,
            &[file_rule("fs.readFile")],
        );

        assert_eq!(bundle.resources.len(), 1);
    }

    #[test]
    fn import_근거가_없는_가짜_fs_receiver는_파일로_오인하지_않는다() {
        let mut bundle = FactBundle::default();

        extract(
            Language::TypeScript,
            &[call("fs.readFile")],
            &file(),
            &mut bundle,
            &[file_rule("fs.readFile")],
        );

        assert!(bundle.resources.is_empty());
    }
}
