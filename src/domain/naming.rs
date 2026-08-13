/// 기계 키를 사람이 읽을 수 있는 기본 표시 이름으로 만든다.
pub fn label(key: &str) -> String {
    key.split_whitespace()
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str()),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
