//! Python 호출·decorator 인자에서 정적으로 확정할 수 있는 값을 추출한다.

/// 따옴표로 감싼 정적 문자열만 반환한다.
pub(crate) fn string_literal(value: &str) -> Option<String> {
    let value = value.trim();
    let quote_offset = value
        .char_indices()
        .find(|(_, character)| matches!(character, '\'' | '"'))
        .map(|(index, _)| index)?;
    let prefix = &value[..quote_offset];
    if prefix
        .chars()
        .any(|character| matches!(character.to_ascii_lowercase(), 'f' | 'j'))
    {
        return None;
    }
    let value = &value[quote_offset..];
    let quote = value.chars().next()?;
    if !matches!(quote, '"' | '\'') || !value.ends_with(quote) {
        return None;
    }
    Some(value[quote.len_utf8()..value.len() - quote.len_utf8()].to_string())
}

/// `key="value"` 형태의 정적 keyword 인자를 반환한다.
pub(crate) fn keyword_string(arguments: &[String], key: &str) -> Option<String> {
    arguments.iter().find_map(|argument| {
        let (name, value) = argument.split_once('=')?;
        (name.trim() == key)
            .then(|| string_literal(value.trim()))
            .flatten()
    })
}

/// HTTP route에서 path/rule/url 등으로 사용되는 정적 경로 인자를 반환한다.
///
/// positional 인자와 프레임워크별 대표 keyword를 모두 지원한다. 정적 문자열이
/// 아닌 값은 `None`으로 남겨 동적 경계를 숨기지 않는다.
pub(crate) fn route_path(arguments: &[String]) -> Option<String> {
    arguments
        .iter()
        .find(|argument| !argument.trim().contains('='))
        .and_then(|argument| {
            let trimmed = argument.trim();
            string_literal(trimmed)
        })
        .or_else(|| {
            ["path", "rule", "url", "route"]
                .iter()
                .find_map(|key| keyword_string(arguments, key))
        })
}

/// `methods=["GET", "POST"]` 또는 `methods="GET"`를 안정적인 문자열로
/// 정규화한다. 여러 메서드는 쉼표로 구분한다.
pub(crate) fn methods_argument(arguments: &[String]) -> Option<String> {
    let value = arguments.iter().find_map(|argument| {
        let (name, value) = argument.split_once('=')?;
        (name.trim() == "methods").then_some(value.trim())
    })?;

    if let Some(method) = string_literal(value) {
        return Some(method.to_ascii_uppercase());
    }

    let value = value
        .trim()
        .trim_start_matches(['[', '(', '{'])
        .trim_end_matches([']', ')', '}']);
    let methods = value
        .split(',')
        .filter_map(|item| string_literal(item.trim()))
        .map(|method| method.to_ascii_uppercase())
        .collect::<Vec<_>>();
    (!methods.is_empty()).then(|| methods.join(","))
}

pub(crate) fn join_paths(prefix: &str, path: &str) -> String {
    let prefix = prefix.trim_end_matches('/');
    let path = path.trim_start_matches('/');
    match (prefix.is_empty(), path.is_empty()) {
        (true, true) => "/".into(),
        (true, false) => format!("/{path}"),
        (false, true) => format!("{prefix}/"),
        (false, false) => format!("{prefix}/{path}"),
    }
}
