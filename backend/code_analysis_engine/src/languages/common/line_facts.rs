use crate::config::{ParserPolicy, RoutePatternKind};
use crate::diagnostics::Diagnostic;
use crate::facts::{
    DecoratorFact, Entrypoint, EntrypointKind, Evidence, FactBundle, ResourceAccess, ResourceKind,
    SourceSpan,
};
use crate::model::{FileEntry, Language};
use regex::Regex;
use std::collections::HashSet;
use std::path::Path;

use super::metadata::stable_id;
use super::unit_index::UnitSpanIndex;

pub(super) fn extract_line_facts(
    language: Language,
    file: &FileEntry,
    source: &str,
    unit_index: &UnitSpanIndex,
    bundle: &mut FactBundle,
    parser_policy: &ParserPolicy,
) {
    let language_key = language.key();
    let route_patterns: Vec<_> = parser_policy
        .route_rules
        .iter()
        .filter(|rule| rule.languages.iter().any(|key| key == language_key))
        .filter_map(|rule| match Regex::new(&rule.pattern) {
            Ok(pattern) => Some((pattern, rule.kind)),
            Err(error) => {
                bundle.diagnostics.push(Diagnostic::warning(
                    "PARSER_RULE_INVALID",
                    format!("경로 분석 규칙을 컴파일하지 못했습니다: {error}"),
                    Path::new(&file.relative_path),
                ));
                None
            }
        })
        .collect();
    let sql_pattern = match Regex::new(&parser_policy.sql_pattern) {
        Ok(pattern) => Some(pattern),
        Err(error) => {
            bundle.diagnostics.push(Diagnostic::warning(
                "PARSER_RULE_INVALID",
                format!("SQL 분석 규칙을 컴파일하지 못했습니다: {error}"),
                Path::new(&file.relative_path),
            ));
            None
        }
    };

    for (index, line) in source.lines().enumerate() {
        let line_number = index as u32 + 1;
        let route = (language == Language::Java)
            .then(|| java_annotation_route(line, parser_policy))
            .flatten()
            .or_else(|| {
                route_patterns.iter().find_map(|(pattern, kind)| {
                    pattern.captures(line).map(|captures| match kind {
                        RoutePatternKind::MethodAndPath => (
                            captures
                                .get(1)
                                .map(|value| value.as_str().to_uppercase())
                                .unwrap_or_else(|| parser_policy.default_http_method.clone()),
                            captures
                                .get(2)
                                .map(|value| value.as_str().to_string())
                                .unwrap_or_else(|| parser_policy.default_http_path.clone()),
                        ),
                        RoutePatternKind::PathOnly => (
                            parser_policy.default_http_method.clone(),
                            captures
                                .get(1)
                                .map(|value| value.as_str().to_string())
                                .unwrap_or_else(|| parser_policy.default_http_path.clone()),
                        ),
                        RoutePatternKind::Attribute => (
                            captures
                                .get(1)
                                .map(|value| value.as_str().to_uppercase())
                                .unwrap_or_else(|| parser_policy.default_http_method.clone()),
                            captures
                                .get(2)
                                .map(|value| value.as_str().to_string())
                                .unwrap_or_else(|| parser_policy.default_http_path.clone()),
                        ),
                    })
                })
            });
        if let Some((method, raw_path)) = route {
            let path = normalize_route_path(&raw_path);
            let unit_id = if is_annotation_line(line) {
                unit_index.unit_for_annotation_line(line_number)
            } else {
                unit_index.unit_for_line(line_number)
            };
            let span = SourceSpan::new(
                file.file_id.clone(),
                file.relative_path.clone(),
                line_number,
                1,
                line_number,
                line.chars().count() as u32 + 1,
            );
            let id = stable_id("entry", &format!("{}:{}:{}", file.file_id, method, path));
            if !bundle.entrypoints.iter().any(|entry| entry.id == id) {
                bundle.entrypoints.push(Entrypoint {
                    id,
                    unit_id,
                    kind: EntrypointKind::Http,
                    name: path.clone(),
                    method: Some(method),
                    path: Some(path.clone()),
                    framework_id: None,
                    evidence: vec![
                        Evidence::new("route", path, span.clone()),
                        // 프레임워크 macro/DSL adapter가 경로·메서드·핸들러를
                        // 재구성할 수 있도록 원본 한 줄을 근거로 남긴다.
                        // route 값만으로는 C++의 ADD_METHOD_TO, CROW_ROUTE처럼
                        // 호출 인자에 들어 있는 정적 정보를 복원할 수 없다.
                        Evidence::new("routeSource", line.trim(), span),
                    ],
                });
            }
        }
    }

    extract_sql_resources(source, file, unit_index, bundle, sql_pattern.as_ref());
    extract_attribute_facts(language, file, source, unit_index, bundle);
}

/// SQL은 문자열·템플릿·멀티라인 heredoc 안에 놓이는 경우가 많으므로 한 줄
/// 정규식만으로 처리하지 않는다. SQL 시작 키워드부터 최대 몇 줄을 하나의
/// 정적 문장 후보로 묶고, 설정된 테이블 패턴을 모두 추출한다.
fn extract_sql_resources(
    source: &str,
    file: &FileEntry,
    unit_index: &UnitSpanIndex,
    bundle: &mut FactBundle,
    sql_pattern: Option<&Regex>,
) {
    let Some(sql_pattern) = sql_pattern else {
        return;
    };
    let lines = source.lines().collect::<Vec<_>>();
    let mut seen = HashSet::new();
    for start in 0..lines.len() {
        if !looks_like_sql_start(lines[start]) {
            continue;
        }
        let limit = (start + 16).min(lines.len());
        let mut end = start + 1;
        while end < limit {
            let line = lines[end];
            end += 1;
            if line.contains(';') || line.contains('`') {
                break;
            }
        }
        let statement = lines[start..end].join(" ");
        if !looks_like_sql_statement(&statement) {
            continue;
        }
        let line_number = start as u32 + 1;
        let unit_id = unit_index.unit_for_line(line_number);
        let span = SourceSpan::new(
            file.file_id.clone(),
            file.relative_path.clone(),
            line_number,
            1,
            end as u32,
            lines[end.saturating_sub(1)].chars().count() as u32 + 1,
        );
        for captures in sql_pattern.captures_iter(&statement) {
            let Some(name) = captures.get(1).map(|value| value.as_str().to_string()) else {
                continue;
            };
            let mode = captures
                .get(0)
                .map(|match_value| {
                    sql_table_access_mode(&statement, match_value.start(), match_value.as_str())
                })
                .unwrap_or_else(|| sql_access_mode(&statement));
            let key = format!("{}:{}:{:?}:{}", unit_id, line_number, mode, name);
            if !seen.insert(key) {
                continue;
            }
            let id = stable_id(
                "resource",
                &format!("{}:{}:{:?}:{}", file.file_id, line_number, mode, name),
            );
            if bundle.resources.iter().any(|resource| resource.id == id) {
                continue;
            }
            bundle.resources.push(ResourceAccess {
                id,
                unit_id: unit_id.clone(),
                kind: ResourceKind::Table,
                name: name.clone(),
                mode: mode.clone(),
                evidence: vec![Evidence::new("resource", name, span.clone())],
            });
        }
    }
}

fn looks_like_sql_start(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    let trimmed = lower.trim_start();
    [
        "select ",
        "insert ",
        "update ",
        "delete ",
        "merge ",
        "with ",
        "create table",
        "alter table",
        "drop table",
    ]
    .iter()
    .any(|prefix| trimmed.starts_with(prefix))
        || lower.contains(" select ")
        || lower.contains("select ")
        || lower.contains(" insert ")
        || lower.contains("insert into ")
        || lower.contains(" update ")
        || lower.contains("update ")
        || lower.contains(" delete ")
        || lower.contains("delete from ")
}

fn sql_access_mode(statement: &str) -> crate::facts::AccessMode {
    let lower = statement.to_ascii_lowercase();
    let has_read = lower.contains("select ") || lower.trim_start().starts_with("select");
    let has_write = [
        "insert ",
        "update ",
        "delete ",
        "merge ",
        "create table",
        "alter table",
        "drop table",
    ]
    .iter()
    .any(|keyword| lower.contains(keyword));
    match (has_read, has_write) {
        (true, true) => crate::facts::AccessMode::ReadWrite,
        (true, false) => crate::facts::AccessMode::Read,
        (false, true) => crate::facts::AccessMode::Write,
        (false, false) => crate::facts::AccessMode::Unknown,
    }
}

/// SQL 문장 전체가 아니라 테이블을 소개한 절을 기준으로 접근 모드를
/// 판정한다. 예를 들어 `INSERT INTO audit SELECT ... FROM users`는
/// `audit=Write`, `users=Read`가 되어야 한다. 문장 전체를 ReadWrite로
/// 칠하면 프론트의 DB 관계가 실제 코드보다 강하게 표시된다.
fn sql_table_access_mode(
    statement: &str,
    match_start: usize,
    match_text: &str,
) -> crate::facts::AccessMode {
    let keyword = match_text
        .split_whitespace()
        .map(|token| token.to_ascii_lowercase())
        .find(|token| {
            matches!(
                token.as_str(),
                "from" | "into" | "update" | "join" | "table"
            )
        })
        .unwrap_or_default();
    let before_match = statement[..match_start].to_ascii_lowercase();
    let nearest_statement_keyword = ["select", "delete", "insert", "update", "merge"]
        .iter()
        .filter_map(|keyword| {
            before_match
                .rfind(keyword)
                .map(|position| (position, *keyword))
        })
        .max_by_key(|(position, _)| *position)
        .map(|(_, keyword)| keyword);
    match keyword.as_str() {
        "from" if nearest_statement_keyword == Some("delete") => crate::facts::AccessMode::Write,
        "from" | "join" => crate::facts::AccessMode::Read,
        "into" | "update" | "table" => {
            if keyword == "into" && nearest_statement_keyword == Some("merge") {
                crate::facts::AccessMode::ReadWrite
            } else {
                crate::facts::AccessMode::Write
            }
        }
        _ => sql_access_mode(statement),
    }
}

/// Java annotation, C# attribute, Rust attribute를 프레임워크 비의존 원시
/// decorator로 보존한다. 프레임워크 adapter는 이름과 인자만 보고 route,
/// command, main 같은 의미를 추가한다.
fn extract_attribute_facts(
    language: Language,
    file: &FileEntry,
    source: &str,
    unit_index: &UnitSpanIndex,
    bundle: &mut FactBundle,
) {
    if !matches!(language, Language::Java | Language::CSharp | Language::Rust) {
        return;
    }
    for (index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        let (marker, expression) = if let Some(value) = trimmed.strip_prefix("#[") {
            ("#[", value.trim_end_matches(']').trim())
        } else if let Some(value) = trimmed.strip_prefix('@') {
            ("@", value.trim())
        } else if let Some(value) = trimmed.strip_prefix('[') {
            ("[", value.trim_end_matches(']').trim())
        } else {
            continue;
        };
        if expression.is_empty() {
            continue;
        }
        let (name, arguments) = split_attribute_expression(expression);
        let line_number = index as u32 + 1;
        let span = SourceSpan::new(
            file.file_id.clone(),
            file.relative_path.clone(),
            line_number,
            1,
            line_number,
            line.chars().count() as u32 + 1,
        );
        let expression = format!("{marker}{expression}");
        let id = stable_id(
            "decorator",
            &format!("{}:{}:{}", file.file_id, line_number, expression),
        );
        if bundle.decorators.iter().any(|decorator| decorator.id == id) {
            continue;
        }
        bundle.decorators.push(DecoratorFact {
            id,
            unit_id: unit_index.unit_for_annotation_line(line_number),
            receiver: name
                .rsplit_once("::")
                .map(|(receiver, _)| receiver.to_string()),
            name: name
                .rsplit_once("::")
                .map(|(_, value)| value)
                .or_else(|| name.rsplit_once('.').map(|(_, value)| value))
                .unwrap_or(&name)
                .trim()
                .trim_end_matches("Attribute")
                .to_string(),
            arguments,
            expression: expression.clone(),
            evidence: vec![Evidence::new("attribute", expression, span)],
        });
    }
}

fn split_attribute_expression(expression: &str) -> (String, Vec<String>) {
    let Some(open) = expression.find('(') else {
        return (expression.trim().to_string(), Vec::new());
    };
    let name = expression[..open].trim().to_string();
    let inner = expression[open + 1..].trim_end_matches(')').trim();
    let mut arguments = Vec::new();
    let mut current = String::new();
    let mut depth = 0_i32;
    let mut quote = None;
    for character in inner.chars() {
        if let Some(active) = quote {
            current.push(character);
            if character == active {
                quote = None;
            }
            continue;
        }
        match character {
            '\'' | '"' => {
                quote = Some(character);
                current.push(character);
            }
            '(' | '[' | '{' => {
                depth += 1;
                current.push(character);
            }
            ')' | ']' | '}' => {
                depth -= 1;
                current.push(character);
            }
            ',' if depth == 0 => {
                if !current.trim().is_empty() {
                    arguments.push(current.trim().to_string());
                }
                current.clear();
            }
            _ => current.push(character),
        }
    }
    if !current.trim().is_empty() {
        arguments.push(current.trim().to_string());
    }
    (name, arguments)
}

fn is_annotation_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with('@') || trimmed.starts_with("#[") || trimmed.starts_with('[')
}

fn normalize_route_path(path: &str) -> String {
    let path = path.trim();
    if path.is_empty() || path == "/" || path.starts_with('/') || path.starts_with('<') {
        path.to_string()
    } else {
        format!("/{path}")
    }
}

fn java_annotation_route(line: &str, parser_policy: &ParserPolicy) -> Option<(String, String)> {
    let expression = line.trim_start().strip_prefix('@')?.trim();
    let (name, arguments) = split_attribute_expression(expression);
    let name = name
        .rsplit_once('.')
        .map(|(_, value)| value)
        .unwrap_or(&name);
    let name_lower = name.to_ascii_lowercase();
    let mapping_method = match name_lower.as_str() {
        "getmapping" => Some("GET"),
        "postmapping" => Some("POST"),
        "putmapping" => Some("PUT"),
        "patchmapping" => Some("PATCH"),
        "deletemapping" => Some("DELETE"),
        "requestmapping" => None,
        _ => return None,
    };

    let path = arguments
        .iter()
        .find_map(|argument| {
            let value = argument
                .split_once('=')
                .filter(|(key, _)| {
                    matches!(
                        key.trim().to_ascii_lowercase().as_str(),
                        "value" | "path" | "url"
                    )
                })
                .map(|(_, value)| value)
                .or_else(|| (!argument.contains('=')).then_some(argument.as_str()))?;
            quoted_literal(value)
        })
        .unwrap_or_else(|| parser_policy.default_http_path.clone());
    let method = mapping_method
        .map(str::to_string)
        .or_else(|| {
            arguments.iter().find_map(|argument| {
                let (key, value) = argument.split_once('=')?;
                (key.trim().eq_ignore_ascii_case("method")).then(|| {
                    value
                        .trim()
                        .rsplit_once("::")
                        .map(|(_, value)| value)
                        .unwrap_or_else(|| {
                            value
                                .trim()
                                .rsplit_once('.')
                                .map(|(_, value)| value)
                                .unwrap_or(value.trim())
                        })
                        .to_ascii_uppercase()
                })
            })
        })
        .unwrap_or_else(|| parser_policy.default_http_method.clone());
    Some((method, path))
}

fn quoted_literal(value: &str) -> Option<String> {
    let value = value.trim();
    let first = value.find(['"', '\''])?;
    let quote = value.as_bytes().get(first).copied()? as char;
    let rest = &value[first + 1..];
    let end = rest.find(quote)?;
    Some(rest[..end].to_string())
}

/// 일반 코드의 `from x import y`, 주석, 문자열을 SQL 테이블로 오인하지 않도록
/// SQL 문장 형태가 확인되는 경우에만 설정된 정규식을 적용한다.
fn looks_like_sql_statement(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    let trimmed = lower.trim_start();
    if trimmed.starts_with('#')
        || trimmed.starts_with("//")
        || trimmed.starts_with("/*")
        || trimmed.starts_with('*')
    {
        return false;
    }

    (lower.contains("select ") && lower.contains(" from "))
        || lower.contains("insert into ")
        || (lower.contains("update ") && lower.contains(" set "))
        || lower.contains("delete from ")
        || lower.contains("merge into ")
        || (lower.contains("join ") && lower.contains(" on "))
        || lower.contains("create table ")
        || lower.contains("alter table ")
        || lower.contains("drop table ")
        || (trimmed.starts_with("with ") && lower.contains(" select ") && lower.contains(" from "))
}
