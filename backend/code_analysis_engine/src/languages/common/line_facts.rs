use crate::config::{ParserPolicy, RoutePatternKind};
use crate::diagnostics::Diagnostic;
use crate::facts::{DecoratorFact, Entrypoint, EntrypointKind, Evidence, FactBundle, SourceSpan};
use crate::model::{FileEntry, Language};
use regex::Regex;
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
        // 지원 언어의 route는 AST·framework adapter가 소유한다. 공통
        // 정규식은 주석·문자열·일반 함수 호출을 route로 오인하기 쉬워서
        // Unknown DSL과 C/C++의 명시적인 route macro만 보조한다.
        .filter(|rule| line_route_rule_allowed(language, &rule.pattern))
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

    super::sql::extract_sql_resources(source, file, unit_index, bundle, sql_pattern.as_ref());
    extract_attribute_facts(language, file, source, unit_index, bundle);
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
        "get" => Some("GET"),
        "post" => Some("POST"),
        "put" => Some("PUT"),
        "patch" => Some("PATCH"),
        "delete" => Some("DELETE"),
        "head" => Some("HEAD"),
        "path" | "controller" => Some(parser_policy.default_http_method.as_str()),
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

fn line_route_rule_allowed(language: Language, pattern: &str) -> bool {
    if language == Language::Unknown {
        return true;
    }
    if language == Language::Java {
        // Spring WebFlux의 `RequestPredicates.GET("/path")`와 JAX-RS/
        // Quarkus의 `@GET`·`@Path`는 호출/annotation 조합을 별도 adapter가
        // 아직 완전히 복원하지 못하는 정적 경계다. 이름이 고정된 Java
        // annotation/DSL만 보조하고 일반 `route(...)` 정규식은 막는다.
        let pattern = pattern.to_ascii_lowercase();
        return pattern.contains("requestpredicates")
            || pattern.contains("getmapping")
            || pattern.contains("postmapping")
            || pattern.contains("putmapping")
            || pattern.contains("patchmapping")
            || pattern.contains("deletemapping")
            || pattern.contains("requestmapping")
            || pattern.contains("@path")
            || pattern.contains("@controller")
            || pattern.contains("@(?:(get|post|put|patch|delete|head)");
    }
    if language == Language::CSharp {
        let pattern = pattern.to_ascii_lowercase();
        // ASP.NET attribute route는 C# AST의 decorator와 framework
        // detection이 서로 다른 파일/프로젝트 경계에 있을 수 있어,
        // 이름이 고정된 attribute 정규식만 보조적으로 허용한다.
        return [
            "httpget",
            "httppost",
            "httpput",
            "httppatch",
            "httpdelete",
            "httphead",
            "route",
        ]
        .iter()
        .any(|marker| pattern.contains(marker));
    }
    if language == Language::Rust {
        let pattern = pattern.to_ascii_lowercase();
        // Rocket/Actix처럼 attribute가 route 자체를 정의하는 경우는
        // framework detection 없이도 원시 HTTP 경계를 잃지 않도록
        // 고정된 HTTP attribute 규칙만 허용한다.
        return pattern.contains("#\\[(get|post|put|patch|delete|head)");
    }
    if !matches!(language, Language::C | Language::Cpp) {
        return false;
    }
    let pattern = pattern.to_ascii_lowercase();
    ["crow_route", "add_method_to", "method_add"]
        .iter()
        .any(|marker| pattern.contains(marker))
}

fn quoted_literal(value: &str) -> Option<String> {
    let value = value.trim();
    let first = value.find(['"', '\''])?;
    let quote = value.as_bytes().get(first).copied()? as char;
    let rest = &value[first + 1..];
    let end = rest.find(quote)?;
    Some(rest[..end].to_string())
}
