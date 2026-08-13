//! C++ macro 기반 HTTP route의 정적 정보를 보정하는 공통 내부 도구.
//!
//! CROW_ROUTE와 Drogon의 ADD_METHOD_TO/METHOD_ADD는 일반 함수 호출이 아니라
//! 매크로/DSL이므로 언어 공통 line fact는 일단 경로만 안전하게 추출한다.
//! 이 모듈은 그 line fact에 남겨둔 `routeSource` 근거만 읽어 메서드와 정적
//! 핸들러를 추가한다. 문자열을 실행하거나 동적 값을 추측하지 않는다.

use crate::facts::{CodeUnitKind, Entrypoint, EntrypointKind, FactStore};
use crate::frameworks::registry::detector::FrameworkDetection;
use crate::languages::common::metadata::stable_id;

pub(super) fn refine_drogon_routes(facts: &mut FactStore, detections: &[FrameworkDetection]) {
    if !detections
        .iter()
        .any(|detection| detection.id == "cpp.drogon")
    {
        return;
    }
    let entrypoint_indices = facts
        .entrypoints
        .iter()
        .enumerate()
        .filter(|(_, entrypoint)| {
            entrypoint.kind == EntrypointKind::Http
                && entrypoint
                    .evidence
                    .iter()
                    .find(|evidence| evidence.kind == "routeSource")
                    .is_some_and(|evidence| is_drogon_route_source(&evidence.value))
        })
        .map(|(index, _)| index)
        .collect::<Vec<_>>();

    for index in entrypoint_indices {
        let Some(source) = route_source(&facts.entrypoints[index]) else {
            continue;
        };
        let Some((target, method)) = drogon_route_parts(source) else {
            continue;
        };
        let file_id = route_file_id(&facts.entrypoints[index]);
        let target_unit = resolve_handler(facts, file_id, &target);
        let entrypoint = &mut facts.entrypoints[index];
        entrypoint.method = Some(method);
        if let Some(target_unit) = target_unit {
            entrypoint.unit_id = target_unit;
        }
        entrypoint.framework_id = Some("cpp.drogon".to_string());
        update_route_identity(entrypoint, "cpp.drogon");
        update_route_name(entrypoint);
    }
}

pub(super) fn refine_crow_routes(facts: &mut FactStore, detections: &[FrameworkDetection]) {
    if !detections
        .iter()
        .any(|detection| detection.id == "cpp.crow")
    {
        return;
    }
    let entrypoint_indices = facts
        .entrypoints
        .iter()
        .enumerate()
        .filter(|(_, entrypoint)| {
            entrypoint.kind == EntrypointKind::Http
                && entrypoint
                    .evidence
                    .iter()
                    .find(|evidence| evidence.kind == "routeSource")
                    .is_some_and(|evidence| is_crow_route_source(&evidence.value))
        })
        .map(|(index, _)| index)
        .collect::<Vec<_>>();

    let mut additions = Vec::new();
    for index in entrypoint_indices {
        let Some(source) = route_source(&facts.entrypoints[index]) else {
            continue;
        };
        let methods = crow_methods(source);
        let Some(file_id) = route_file_id(&facts.entrypoints[index]) else {
            continue;
        };
        let target = crow_handler(source);
        let target_unit = target.and_then(|target| resolve_handler(facts, Some(file_id), target));
        let entrypoint = &mut facts.entrypoints[index];
        if let Some(target_unit) = target_unit {
            entrypoint.unit_id = target_unit;
        }
        entrypoint.framework_id = Some("cpp.crow".to_string());
        if let Some(first_method) = methods.first() {
            entrypoint.method = Some(first_method.clone());
            update_route_identity(entrypoint, "cpp.crow");
            update_route_name(entrypoint);
        }
        for method in methods.into_iter().skip(1) {
            let mut duplicate = entrypoint.clone();
            duplicate.method = Some(method);
            duplicate.id = stable_id(
                "entry",
                &format!(
                    "{}:{}:{}:{}",
                    duplicate
                        .evidence
                        .first()
                        .map(|e| e.span.file_id.as_str())
                        .unwrap_or_default(),
                    duplicate.method.as_deref().unwrap_or_default(),
                    duplicate.path.as_deref().unwrap_or_default(),
                    "cpp.crow"
                ),
            );
            update_route_name(&mut duplicate);
            additions.push(duplicate);
        }
    }
    facts.entrypoints.extend(additions);
}

fn is_drogon_route_source(source: &str) -> bool {
    let name = source
        .split_once('(')
        .map(|(name, _)| name.trim().to_ascii_lowercase());
    name.is_some_and(|name| matches!(name.as_str(), "add_method_to" | "method_add"))
}

fn is_crow_route_source(source: &str) -> bool {
    source
        .split_once('(')
        .map(|(name, _)| name.trim().eq_ignore_ascii_case("CROW_ROUTE"))
        .unwrap_or(false)
}

fn route_source(entrypoint: &Entrypoint) -> Option<&str> {
    entrypoint
        .evidence
        .iter()
        .find(|evidence| evidence.kind == "routeSource")
        .map(|evidence| evidence.value.as_str())
}

fn route_file_id(entrypoint: &Entrypoint) -> Option<&str> {
    entrypoint
        .evidence
        .iter()
        .find(|evidence| evidence.kind == "routeSource")
        .map(|evidence| evidence.span.file_id.as_str())
}

fn drogon_route_parts(source: &str) -> Option<(String, String)> {
    let name = source
        .split_once('(')
        .map(|(name, _)| name.trim().to_ascii_lowercase())?;
    if !matches!(name.as_str(), "add_method_to" | "method_add") {
        return None;
    }
    let arguments = macro_arguments(source)?;
    let target = arguments.first()?.trim();
    let method = arguments
        .get(2)
        .and_then(|value| normalize_http_method(value))
        .unwrap_or_else(|| "HTTP".to_string());
    Some((target.to_string(), method))
}

fn crow_methods(source: &str) -> Vec<String> {
    let Some(start) = source.to_ascii_lowercase().find(".methods(") else {
        return vec!["HTTP".to_string()];
    };
    let start = start + ".methods(".len();
    let Some(end) = source[start..].find(')') else {
        return vec!["HTTP".to_string()];
    };
    let inner = &source[start..start + end];
    let methods = split_arguments(inner)
        .into_iter()
        .filter_map(|value| normalize_http_method(value.trim()))
        .collect::<Vec<_>>();
    if methods.is_empty() {
        vec!["HTTP".to_string()]
    } else {
        methods
    }
}

fn crow_handler(source: &str) -> Option<&str> {
    let route_end = source.find(")(")? + 2;
    let tail = source[route_end..].trim();
    let handler = tail.trim_end_matches(';').trim();
    if handler.starts_with('[') || handler.starts_with("[]( ") || handler.is_empty() {
        return None;
    }
    Some(handler.trim_end_matches(')'))
}

fn macro_arguments(source: &str) -> Option<Vec<String>> {
    let open = source.find('(')?;
    let close = source.rfind(')')?;
    (close > open).then(|| split_arguments(&source[open + 1..close]))
}

fn split_arguments(value: &str) -> Vec<String> {
    let mut arguments = Vec::new();
    let mut current = String::new();
    let mut depth = 0_i32;
    let mut quote = None;
    for character in value.chars() {
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
                arguments.push(current.trim().to_string());
                current.clear();
            }
            _ => current.push(character),
        }
    }
    if !current.trim().is_empty() {
        arguments.push(current.trim().to_string());
    }
    arguments
}

fn normalize_http_method(value: &str) -> Option<String> {
    let value = value
        .trim()
        .trim_matches(|character| character == '"' || character == '\'');
    let value = value.split("::").last().unwrap_or(value);
    let value = value.split('.').next_back().unwrap_or(value);
    let upper = value.to_ascii_uppercase();
    matches!(
        upper.as_str(),
        "GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "HEAD" | "OPTIONS" | "CONNECT" | "TRACE"
    )
    .then_some(upper)
}

fn resolve_handler(facts: &FactStore, file_id: Option<&str>, target: &str) -> Option<String> {
    let method_name = target
        .rsplit_once("::")
        .map(|(_, value)| value)
        .or_else(|| target.rsplit_once("->").map(|(_, value)| value))
        .or_else(|| target.rsplit_once('.').map(|(_, value)| value))
        .unwrap_or(target)
        .trim();
    if method_name.is_empty() {
        return None;
    }
    let mut candidates = facts
        .units
        .values()
        .filter(|unit| unit.name == method_name)
        .filter(|unit| {
            matches!(
                unit.kind,
                CodeUnitKind::Function
                    | CodeUnitKind::Method
                    | CodeUnitKind::Constructor
                    | CodeUnitKind::Lambda
            )
        })
        .filter(|unit| file_id.is_none_or(|file_id| unit.file_id == file_id))
        .filter(|unit| target_parent_matches(facts, unit, target))
        .map(|unit| unit.id.clone())
        .collect::<Vec<_>>();
    candidates.sort();
    candidates.dedup();
    if candidates.len() == 1 {
        return candidates.pop();
    }

    // C++ route macro는 보통 헤더에 있고 구현은 `.cc`에 있다. 같은 파일의
    // qualified parent 검사가 실패해도 같은 파일에 해당 이름의 함수가 하나
    // 뿐이면 그 함수로 좁힐 수 있다.
    if let Some(file_id) = file_id {
        let mut same_file = facts
            .units
            .values()
            .filter(|unit| unit.file_id == file_id && unit.name == method_name)
            .filter(|unit| is_handler_unit(unit.kind.clone()))
            .map(|unit| unit.id.clone())
            .collect::<Vec<_>>();
        same_file.sort();
        same_file.dedup();
        if same_file.len() == 1 {
            return same_file.pop();
        }
    }

    // 헤더 선언이 없고 구현 파일에만 함수가 있는 일반적인 C++ 분리
    // 컴파일 모델을 지원한다. 이름이 프로젝트 전체에서 유일할 때만
    // 연결하고, 중복이면 미해결 경계로 남긴다.
    let mut global = facts
        .units
        .values()
        .filter(|unit| unit.name == method_name)
        .filter(|unit| is_handler_unit(unit.kind.clone()))
        .map(|unit| unit.id.clone())
        .collect::<Vec<_>>();
    global.sort();
    global.dedup();
    (global.len() == 1).then(|| global.remove(0))
}

fn is_handler_unit(kind: CodeUnitKind) -> bool {
    matches!(
        kind,
        CodeUnitKind::Function
            | CodeUnitKind::Method
            | CodeUnitKind::Constructor
            | CodeUnitKind::Lambda
    )
}

fn target_parent_matches(facts: &FactStore, unit: &crate::facts::CodeUnit, target: &str) -> bool {
    let Some((parent, _)) = target.rsplit_once("::") else {
        return true;
    };
    let parent = parent
        .rsplit_once("::")
        .map(|(_, value)| value)
        .unwrap_or(parent);
    let mut current = unit.parent_id.as_deref();
    while let Some(parent_id) = current {
        let Some(parent_unit) = facts.unit(parent_id) else {
            break;
        };
        if parent_unit.name == parent {
            return true;
        }
        current = parent_unit.parent_id.as_deref();
    }
    false
}

fn update_route_name(entrypoint: &mut Entrypoint) {
    let method = entrypoint.method.as_deref().unwrap_or("HTTP");
    let path = entrypoint.path.as_deref().unwrap_or("<dynamic>");
    entrypoint.name = format!("{method} {path}");
}

fn update_route_identity(entrypoint: &mut Entrypoint, framework_id: &str) {
    let Some(span) = entrypoint
        .evidence
        .iter()
        .find(|evidence| evidence.kind == "routeSource")
        .map(|evidence| &evidence.span)
    else {
        return;
    };
    entrypoint.id = stable_id(
        "entry",
        &format!(
            "{}:{}:{}:{}:{}",
            framework_id,
            span.file_id,
            span.start_line,
            entrypoint.method.as_deref().unwrap_or("HTTP"),
            entrypoint.path.as_deref().unwrap_or("<dynamic>")
        ),
    );
}

#[cfg(test)]
mod tests {
    use super::{crow_methods, drogon_route_parts};

    #[test]
    fn crow_methods_extracts_multiple_http_methods() {
        assert_eq!(
            crow_methods(
                r#"CROW_ROUTE(app, \"/items\").methods(crow::HTTPMethod::GET, crow::HTTPMethod::POST)(handler);"#
            ),
            vec!["GET", "POST"]
        );
    }

    #[test]
    fn drogon_macro_extracts_handler_and_method() {
        assert_eq!(
            drogon_route_parts(r#"METHOD_ADD(Api::list, \"/items\", Post);"#),
            Some(("Api::list".to_string(), "POST".to_string()))
        );
    }
}
