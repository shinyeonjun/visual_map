/// 라우터 마운트 뒤에만 나오는 잎 동사·명사 키다.
pub(crate) fn is_leaf_capability_key(key: &str) -> bool {
    matches!(
        key,
        "start"
            | "end"
            | "job"
            | "pdf"
            | "markdown"
            | "regenerate"
            | "latest"
            | "overview"
            | "processing"
            | "recording"
            | "transcript"
            | "reprocess"
            | "transition"
            | "search"
            | "timeline"
            | "bulk-transition"
            | "final-status"
            | "save"
            | "file"
            | "text"
            | "respond"
            | "share"
            | "send-email"
            | "download-ics"
            | "login"
            | "logout"
            | "create"
            | "update"
            | "delete"
            | "get"
            | "getall"
            | "list"
            | "index"
            | "show"
            | "edit"
            | "add"
            | "remove"
            | "configure"
            | "confirm"
            | "cancel"
            | "submit"
            | "register"
            | "authenticate"
            | "authorize"
            | "refresh"
            | "verify"
            | "reset"
            | "upload"
            | "download"
    )
}

/// RPC endpoint 클래스 이름에서 도메인 capability 키를 만든다.
///
/// `GreetingEndpoint` → `greeting`, `TransactionsDatabaseEndpoint` → `transactions_database`
pub(crate) fn endpoint_class_capability_key(class_name: &str, suffix: &str) -> Option<String> {
    let stem = class_name.strip_suffix(suffix)?.trim();
    if stem.is_empty() {
        return None;
    }
    Some(canonical_capability_key(&pascal_case_to_snake_case(stem)))
}

fn pascal_case_to_snake_case(value: &str) -> String {
    let mut out = String::new();
    for (index, character) in value.chars().enumerate() {
        if character.is_ascii_uppercase() && index > 0 {
            let previous = value[..index].chars().last();
            let next = value[index + 1..].chars().next();
            if previous.is_some_and(|ch| ch.is_ascii_lowercase())
                || next.is_some_and(|ch| ch.is_ascii_lowercase())
            {
                out.push('_');
            }
        }
        out.push(character.to_ascii_lowercase());
    }
    out
}

/// 비즈니스 지도에 단독 카드로 두지 않는 기술·운영 키다.
pub(crate) fn is_technical_singleton_key(key: &str) -> bool {
    if is_static_page_key(key) {
        return false;
    }
    if matches!(key, "health" | "summary") {
        return true;
    }
    if key.starts_with("test") || key.contains("debug") {
        return true;
    }
    false
}

/// 운영·관측 capability는 클러스터 크기와 무관하게 cross-cutting이다.
pub(crate) fn is_operational_capability_key(key: &str) -> bool {
    if is_technical_singleton_key(key) {
        return true;
    }
    matches!(
        key,
        "runtime"
            | "metrics"
            | "monitor"
            | "readiness"
            | "ready"
            | "ping"
            | "status"
            | "utils"
            | "install"
    )
}

/// 클라이언트 템플릿 등에서 생긴 `:param` 꼬리를 제거한 능력 키다.
pub(crate) fn canonical_capability_key(key: &str) -> String {
    let key = key.trim();
    if let Some(base) = key.strip_suffix(":param") {
        if !base.is_empty() {
            return base.to_string();
        }
    }
    key.to_string()
}

/// 정적 HTML 페이지 라우트 키다.
pub(crate) fn is_static_page_key(key: &str) -> bool {
    key.ends_with(".html") || key.ends_with(".htm")
}

/// 정적 페이지 키에서 흡수 후보를 찾을 때 쓰는 어간이다.
pub(crate) fn static_page_stem(key: &str) -> Option<&str> {
    key.strip_suffix(".html")
        .or_else(|| key.strip_suffix(".htm"))
        .filter(|stem| !stem.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn param_꼬리_키는_명사_키로_정규화된다() {
        assert_eq!(canonical_capability_key("sessions:param"), "sessions");
        assert_eq!(canonical_capability_key("sessions"), "sessions");
    }

    #[test]
    fn runtime은_운영_키다() {
        assert!(is_operational_capability_key("runtime"));
        assert!(is_operational_capability_key("health"));
        assert!(!is_operational_capability_key("sessions"));
    }

    #[test]
    fn endpoint_클래스_이름은_snake_case_능력_키가_된다() {
        assert_eq!(
            endpoint_class_capability_key("GreetingEndpoint", "Endpoint"),
            Some("greeting".into())
        );
        assert_eq!(
            endpoint_class_capability_key("BasicTypesEndpoint", "Endpoint"),
            Some("basic_types".into())
        );
        assert_eq!(
            endpoint_class_capability_key("TransactionsDatabaseEndpoint", "Endpoint"),
            Some("transactions_database".into())
        );
    }

    #[test]
    fn login_create는_잎_동사_키다() {
        assert!(is_leaf_capability_key("login"));
        assert!(is_leaf_capability_key("create"));
        assert!(!is_leaf_capability_key("authentication"));
    }
}
