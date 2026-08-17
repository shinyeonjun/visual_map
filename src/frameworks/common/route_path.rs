//! HTTP route 문자열·데코레이터 인자에서 경로를 추출한다.

/// 따옴표 문자열 또는 `{ path: 'auth' }` 객체에서 route path를 읽는다.
pub(crate) fn route_path_literal(value: &str) -> Option<String> {
    let value = value.trim();
    string_literal(value).or_else(|| object_property_string(value, "path"))
}

pub(crate) fn string_literal(value: &str) -> Option<String> {
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

fn object_property_string(expression: &str, key: &str) -> Option<String> {
    let expression = expression.trim();
    if !expression.starts_with('{') {
        return None;
    }
    for segment in split_top_level_segments(expression) {
        let Some((property, value)) = segment.split_once(':') else {
            continue;
        };
        let property = property.trim().trim_matches(|ch| ch == '"' || ch == '\'');
        if property == key {
            return string_literal(value.trim());
        }
    }
    None
}

fn split_top_level_segments(expression: &str) -> Vec<String> {
    let inner = expression.trim().trim_start_matches('{').trim_end_matches('}');
    let mut segments = Vec::new();
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
            '{' => {
                depth += 1;
                current.push(character);
            }
            '}' => {
                depth -= 1;
                current.push(character);
            }
            ',' if depth == 0 => {
                if !current.trim().is_empty() {
                    segments.push(current.trim().to_string());
                }
                current.clear();
            }
            _ => current.push(character),
        }
    }
    if !current.trim().is_empty() {
        segments.push(current.trim().to_string());
    }
    segments
}

/// 설정 키·식별자가 route로 오인된 경우를 걸러낸다.
pub(crate) fn is_plausible_http_path(path: &str) -> bool {
    let path = path.trim();
    if path.is_empty() || path == "<dynamic>" {
        return false;
    }
    if path.contains("://") {
        return false;
    }
    // `file.maxFileSize`, `app.apiPrefix` 같은 dotted identifier
    if path.contains('.')
        && !path.contains('{')
        && !path.contains('}')
        && !path.contains('<')
        && !path.contains(':')
    {
        let segment = path.trim_start_matches('/');
        if segment.contains('.') && !segment.contains('/') {
            return false;
        }
    }
    true
}

pub(crate) fn join_paths(prefix: &str, path: &str) -> String {
    let prefix = prefix.trim_matches('/');
    let path = path.trim_start_matches('/');
    if prefix.is_empty() && path.is_empty() {
        "/".to_string()
    } else if prefix.is_empty() {
        format!("/{path}")
    } else if path.is_empty() {
        format!("/{prefix}")
    } else {
        format!("/{prefix}/{path}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 객체_인자에서_path를_읽는다() {
        assert_eq!(
            route_path_literal("{ path: 'auth', version: '1' }").as_deref(),
            Some("auth")
        );
        assert_eq!(
            route_path_literal(r#"{ path: "users" }"#).as_deref(),
            Some("users")
        );
    }

    #[test]
    fn dotted_config_키는_route가_아니다() {
        assert!(!is_plausible_http_path("/file.maxFileSize"));
        assert!(!is_plausible_http_path("/app.apiPrefix"));
        assert!(is_plausible_http_path("/users/:id"));
        assert!(is_plausible_http_path("/api/v1/auth"));
    }
}
