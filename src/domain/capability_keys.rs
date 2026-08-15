//! HTTP 능력 키의 역할 분류다.

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
    )
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
