//! HTTP/WS 계약 경로 정규화와 능력 키 추출.

const STRIP_PREFIXES: &[&str] = &["api", "v1", "v2", "v3", "public", "internal", "backend"];

/// 원시 경로·URL·메서드 접두를 정규화한 계약 경로다.
pub(crate) fn normalize_contract_path(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("<dynamic>") {
        return None;
    }
    let without_method = strip_http_method(trimmed);
    let path_only = strip_url_to_path(without_method);
    if path_only.is_empty() {
        return None;
    }
    let mut normalized = path_only.replace('\\', "/");
    while normalized.contains("//") {
        normalized = normalized.replace("//", "/");
    }
    if normalized.len() > 1 {
        normalized = normalized.trim_end_matches('/').to_string();
    }
    let segments = strip_leading_prefix_segments(mark_instance_segments(
        normalized
            .trim_start_matches('/')
            .split('/')
            .filter(|segment| !segment.is_empty())
            .map(normalize_segment)
            .collect::<Vec<_>>(),
    ));
    if segments.is_empty() {
        return None;
    }
    Some(format!("/{}", segments.join("/")))
}

fn strip_leading_prefix_segments(mut segments: Vec<String>) -> Vec<String> {
    while !segments.is_empty()
        && (STRIP_PREFIXES.contains(&segments[0].as_str()) || segments[0] == ":param")
    {
        segments.remove(0);
    }
    segments
}

fn capability_key_segments(normalized: &str) -> Vec<&str> {
    normalized
        .trim_start_matches('/')
        .split('/')
        .filter(|segment| !segment.is_empty())
        .filter(|segment| *segment != ":param")
        .collect()
}

/// 같은 HTTP 계약을 가리키는 관측을 하나의 식별자로 묶는다.
pub(crate) fn contract_identity(method: Option<&str>, raw: &str) -> Option<String> {
    let path = normalize_contract_path(raw)?;
    let method = method
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_uppercase())
        .unwrap_or_else(|| "*".to_string());
    Some(format!("{method}:{path}"))
}

/// 정규화 경로에서 능력(도메인) 키를 추출한다.
pub(crate) fn capability_key_from_path(raw: &str) -> Option<String> {
    let normalized = normalize_contract_path(raw)?;
    let mut segments = capability_key_segments(&normalized);
    while !segments.is_empty() && STRIP_PREFIXES.contains(&segments[0]) {
        segments.remove(0);
    }
    segments
        .first()
        .map(|segment| segment.to_ascii_lowercase())
        .filter(|key| !key.is_empty() && key != ":param")
}

pub(crate) fn paths_match(a: &str, b: &str) -> bool {
    match (
        normalize_contract_path(a),
        normalize_contract_path(b),
    ) {
        (Some(left), Some(right)) if left == right => true,
        (Some(left), Some(right)) => path_prefix_match(&left, &right),
        _ => false,
    }
}

pub(crate) fn path_prefix_match(left: &str, right: &str) -> bool {
    if left == right {
        return true;
    }
    let left = left.trim_end_matches('/');
    let right = right.trim_end_matches('/');
    left.starts_with(&format!("{right}/")) || right.starts_with(&format!("{left}/"))
}

fn strip_http_method(value: &str) -> &str {
    let upper = value.to_ascii_uppercase();
    for method in [
        "GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS", "CONNECT", "TRACE",
    ] {
        let prefix = format!("{method} ");
        if upper.starts_with(&prefix) {
            return value[prefix.len()..].trim_start();
        }
    }
    value
}

fn strip_url_to_path(value: &str) -> String {
    let value = value.trim();
    if let Some(rest) = value
        .strip_prefix("https://")
        .or_else(|| value.strip_prefix("http://"))
    {
        let without_query = rest.split(['?', '#']).next().unwrap_or(rest);
        if let Some(path_start) = without_query.find('/') {
            return without_query[path_start..].to_string();
        }
        return "/".to_string();
    }
    if value.contains("://") {
        return String::new();
    }
    let path = value.split(['?', '#']).next().unwrap_or(value);
    if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{path}")
    }
}

fn mark_instance_segments(mut segments: Vec<String>) -> Vec<String> {
    // 마지막 세그먼트는 latest/job 같은 계약 단어일 수 있어 인스턴스로 보지 않는다.
    let last = segments.len().saturating_sub(1);
    for index in 1..last {
        if is_instance_value_segment(&segments[index]) {
            segments[index] = ":param".to_string();
        }
    }
    segments
}

fn is_instance_value_segment(segment: &str) -> bool {
    if segment == ":param" {
        return false;
    }
    let len = segment.len();
    if !(3..=6).contains(&len) {
        return false;
    }
    segment
        .chars()
        .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit())
}

fn normalize_segment(segment: &str) -> String {
    let trimmed = segment.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if trimmed.starts_with(':')
        || (trimmed.starts_with('{') && trimmed.ends_with('}'))
        || (trimmed.starts_with('<') && trimmed.ends_with('>'))
    {
        return ":param".to_string();
    }
    if trimmed.chars().all(|ch| ch.is_ascii_digit()) {
        return ":param".to_string();
    }
    if trimmed.len() >= 32 && trimmed.chars().all(|ch| ch.is_ascii_hexdigit() || ch == '-') {
        return ":param".to_string();
    }
    trimmed.to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::{
        capability_key_from_path, contract_identity, normalize_contract_path, path_prefix_match,
        paths_match,
    };

    #[test]
    fn api_버전_접두를_제거하고_능력_키를_만든다() {
        assert_eq!(
            normalize_contract_path("/api/v1/reports/123"),
            Some("/reports/:param".into())
        );
        assert_eq!(capability_key_from_path("/api/v1/reports/123"), Some("reports".into()));
    }

    #[test]
    fn fetch_url과_서버_경로가_같은_계약으로_매칭된다() {
        assert!(paths_match(
            "https://host/api/sessions/abc/events",
            "/sessions/abc/events"
        ));
    }

    #[test]
    fn 선행_파라미터_세그먼트는_건너뛰고_능력_키를_만든다() {
        assert_eq!(
            normalize_contract_path("/{session_id}/participants"),
            Some("/participants".into())
        );
        assert_eq!(
            capability_key_from_path("/{session_id}/participants/contacts"),
            Some("participants".into())
        );
        assert_eq!(capability_key_from_path("/{session_id}"), None);
        assert_eq!(capability_key_from_path("/:param"), None);
    }

    #[test]
    fn 접두_매칭은_세그먼트_경계를_지킨다() {
        assert!(path_prefix_match("/reports", "/reports/123"));
        assert!(!path_prefix_match("/report", "/reports"));
    }

    #[test]
    fn 계약_식별자는_버전_접두와_파라미터를_정규화한다() {
        assert_eq!(
            contract_identity(Some("GET"), "/api/v1/sessions/abc/overview"),
            contract_identity(Some("get"), "/sessions/{session_id}/overview")
        );
        assert_eq!(
            normalize_contract_path("/api/v1/reports/abc/latest"),
            Some("/reports/:param/latest".into())
        );
        assert_eq!(
            normalize_contract_path("/health"),
            Some("/health".into())
        );
        assert_eq!(
            contract_identity(None, "/health"),
            Some("*:/health".into())
        );
        assert_ne!(
            contract_identity(Some("GET"), "/health"),
            contract_identity(Some("POST"), "/health")
        );
    }
}
