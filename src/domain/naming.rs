/// 기계 키를 사람이 읽을 수 있는 기본 표시 이름으로 만든다.
pub fn label(key: &str) -> String {
    match key.to_ascii_lowercase().as_str() {
        "ws" => return "WebSocket".into(),
        "stt" => return "STT".into(),
        _ => {}
    }
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

#[cfg(test)]
mod tests {
    use super::label;

    #[test]
    fn 기술_약어_키는_읽기_쉬운_라벨로_바꾼다() {
        assert_eq!(label("ws"), "WebSocket");
        assert_eq!(label("stt"), "STT");
    }
}
