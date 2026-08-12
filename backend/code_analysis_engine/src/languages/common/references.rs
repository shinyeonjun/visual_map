use crate::facts::ResolutionStatus;
use tree_sitter::Node;

use super::metadata::node_text;

pub(super) fn is_import_node(kind: &str) -> bool {
    matches!(
        kind,
        "import_statement"
            | "import_from_statement"
            | "future_import_statement"
            | "import_declaration"
            | "import_specification"
            | "preproc_include"
            | "use_declaration"
            | "using_directive"
            | "package_import"
    )
}

pub(super) fn is_call_node(kind: &str) -> bool {
    matches!(
        kind,
        "call_expression"
            | "call"
            | "function_call_expression"
            | "method_invocation"
            | "invocation_expression"
            | "call_expression_statement"
            | "new_expression"
            | "object_creation_expression"
            | "object_creation"
            | "class_instance_creation_expression"
            | "instance_creation_expression"
    )
}

pub(super) fn call_target_name(node: Node<'_>, source: &[u8]) -> Option<String> {
    let target = if let Some(constructor) = node
        .child_by_field_name("constructor")
        .or_else(|| node.child_by_field_name("type"))
        .or_else(|| node.child_by_field_name("name"))
        .filter(|_| {
            matches!(
                node.kind(),
                "new_expression"
                    | "object_creation_expression"
                    | "object_creation"
                    | "class_instance_creation_expression"
                    | "instance_creation_expression"
            )
        }) {
        callee_expression_name(constructor, source)
            .or_else(|| fallback_call_target_name(&node_text(constructor, source)))
    } else if let Some(function) = node.child_by_field_name("function") {
        callee_expression_name(function, source)
            .or_else(|| fallback_call_target_name(&node_text(function, source)))
    } else if let Some(name) = node.child_by_field_name("name") {
        compact_node_text(name, source)
            .or_else(|| fallback_call_target_name(&node_text(node, source)))
    } else {
        let text = node_text(node, source);
        fallback_call_target_name(&text)
    };

    target.or_else(|| Some(format!("[unknown:{}@{}]", node.kind(), node.start_byte())))
}

pub(super) fn call_resolution_status(target_name: &str) -> ResolutionStatus {
    if target_name.contains('[')
        || target_name.contains(']')
        || ["eval", "getattr", "reflect", "invoke"]
            .iter()
            .any(|pattern| matches_dynamic_pattern(target_name, pattern))
    {
        ResolutionStatus::Dynamic
    } else if target_name.contains('.') || target_name.contains("::") {
        ResolutionStatus::Candidate
    } else {
        ResolutionStatus::Confirmed
    }
}

/// AST의 callee 표현식을 분석 가능한 짧은 이름으로 정규화한다.
///
/// 호출 인자와 이전 호출의 전체 본문은 대상 이름이 아니다. 예를 들어
/// `client.builder().setup(...).run()`의 대상은 전체 체인이 아니라 마지막
/// 메서드 `run`이어야 한다. 단순한 수신 객체(`service.save()`)는 경로를
/// 보존하고, 이미 호출된 수신 객체(`service.builder().save()`)는 마지막
/// 멤버만 반환해 인자 본문이 참조 이름에 섞이지 않도록 한다.
fn callee_expression_name(node: Node<'_>, source: &[u8]) -> Option<String> {
    match node.kind() {
        "call" | "call_expression" | "method_invocation" | "invocation_expression" => node
            .child_by_field_name("function")
            .or_else(|| node.child_by_field_name("name"))
            .and_then(|function| callee_expression_name(function, source)),
        "member_expression"
        | "null_aware_member_expression"
        | "field_expression"
        | "selector_expression"
        | "attribute"
        | "member_access_expression" => member_expression_name(node, source),
        "parenthesized_expression" => node
            .child_by_field_name("expression")
            .or_else(|| node.named_child(0))
            .and_then(|expression| callee_expression_name(expression, source)),
        "arrow_function"
        | "function_expression"
        | "function"
        | "lambda"
        | "lambda_expression"
        | "closure_expression" => Some("[anonymous]".to_string()),
        "generic_function" | "generic_name" | "template_method" => node
            .child_by_field_name("function")
            .or_else(|| node.child_by_field_name("name"))
            .and_then(|function| callee_expression_name(function, source))
            .or_else(|| compact_node_text(node, source)),
        _ => compact_node_text(node, source),
    }
}

fn member_expression_name(node: Node<'_>, source: &[u8]) -> Option<String> {
    let member = ["property", "field", "attribute", "name"]
        .iter()
        .find_map(|field| node.child_by_field_name(field));
    let member_name = member.and_then(|member| compact_node_text(member, source))?;

    let receiver = ["object", "value", "operand", "argument", "expression"]
        .iter()
        .find_map(|field| node.child_by_field_name(field));
    let Some(receiver) = receiver else {
        return Some(member_name);
    };

    let receiver_name = callee_expression_name(receiver, source)?;
    if receiver_name.is_empty() {
        return Some(member_name);
    }

    // 리터럴·이진식·배열·서브스크립트 등은 수신 객체의 원문을 이름으로
    // 보존할 수 없는 복합 표현식이다. 서브스크립트는 호출 대상이 런타임에
    // 결정될 수 있으므로 대괄호 표식을 남겨 동적 경계로 보존한다.
    if matches!(
        receiver.kind(),
        "subscript_expression"
            | "index_expression"
            | "element_access_expression"
            | "computed_member_expression"
    ) {
        return Some(format!("[{member_name}]"));
    }
    if !is_simple_receiver(receiver) {
        return Some(member_name);
    }

    // `a.b().c()`의 receiver는 call_expression이다. 이 receiver를 다시
    // 원문으로 펼치면 호출 인자 전체가 다시 유입되므로 마지막 멤버만 쓴다.
    if matches!(
        receiver.kind(),
        "call" | "call_expression" | "method_invocation" | "invocation_expression"
    ) {
        return Some(member_name);
    }

    let separator = if node.kind() == "field_expression" && node_text(node, source).contains("::") {
        "::"
    } else {
        "."
    };
    Some(format!("{receiver_name}{separator}{member_name}"))
}

fn is_simple_receiver(node: Node<'_>) -> bool {
    match node.kind() {
        "identifier"
        | "field_identifier"
        | "property_identifier"
        | "private_property_identifier"
        | "type_identifier"
        | "namespace_identifier"
        | "scoped_identifier"
        | "qualified_identifier"
        | "qualified_name"
        | "alias_qualified_name"
        | "this"
        | "self"
        | "super"
        | "generic_function"
        | "generic_name"
        | "template_method"
        | "member_expression"
        | "null_aware_member_expression"
        | "field_expression"
        | "selector_expression"
        | "attribute"
        | "member_access_expression" => true,
        "parenthesized_expression" => node
            .child_by_field_name("expression")
            .or_else(|| node.named_child(0))
            .is_some_and(is_simple_receiver),
        _ => false,
    }
}

fn compact_node_text(node: Node<'_>, source: &[u8]) -> Option<String> {
    let text = node_text(node, source);
    let compact = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let compact = compact.trim();
    (!compact.is_empty()).then(|| compact.to_string())
}

fn fallback_call_target_name(text: &str) -> Option<String> {
    let prefix = text.split('(').next().unwrap_or(text).trim();
    if prefix.is_empty() {
        return None;
    }

    // 파서가 오류 상태라 field를 제공하지 못한 경우에도 호출 인자나
    // 줄바꿈이 결과에 들어가지 않도록 마지막 식별자 경로만 보존한다.
    let candidate = prefix
        .rsplit_once("::")
        .map(|(_, name)| name)
        .or_else(|| prefix.rsplit_once("->").map(|(_, name)| name))
        .or_else(|| prefix.rsplit_once('.').map(|(_, name)| name))
        .unwrap_or(prefix)
        .trim();
    (!candidate.is_empty()).then(|| candidate.to_string())
}

/// 동적 호출 패턴을 식별자 경계 기준으로 비교한다.
///
/// `eval`은 `evaluate`나 `retrieval`에 포함됐다는 이유만으로 동적 호출이
/// 되면 안 된다. 반면 `reflect.ValueOf`와 `method.invoke`처럼 점으로
/// 연결된 경로는 각 식별자 토큰이 연속해서 일치하면 패턴으로 인정한다.
pub(super) fn matches_dynamic_pattern(target: &str, pattern: &str) -> bool {
    let target_tokens = identifier_tokens(target);
    let pattern_tokens = identifier_tokens(pattern);
    if pattern_tokens.is_empty() || target_tokens.len() < pattern_tokens.len() {
        return false;
    }
    target_tokens
        .windows(pattern_tokens.len())
        .any(|window| window == pattern_tokens.as_slice())
}

fn identifier_tokens(value: &str) -> Vec<String> {
    value
        .split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
        .filter(|token| !token.is_empty())
        .map(|token| token.to_ascii_lowercase())
        .collect()
}

pub(super) fn normalize_reference_name(text: &str) -> String {
    text.trim()
        .trim_start_matches("import")
        .trim_start_matches("include")
        .trim_matches(|character| matches!(character, '"' | '\'' | '<' | '>' | ';'))
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::{call_resolution_status, call_target_name, matches_dynamic_pattern};
    use crate::facts::ResolutionStatus;
    use tree_sitter::{Language, Node, Parser};

    fn first_call_target(language: Language, source: &str) -> Option<String> {
        fn find_call(node: Node<'_>) -> Option<Node<'_>> {
            if matches!(
                node.kind(),
                "call"
                    | "call_expression"
                    | "function_call_expression"
                    | "method_invocation"
                    | "invocation_expression"
                    | "call_expression_statement"
                    | "new_expression"
                    | "object_creation_expression"
                    | "object_creation"
                    | "class_instance_creation_expression"
                    | "instance_creation_expression"
            ) {
                return Some(node);
            }
            let mut cursor = node.walk();
            let result = node.children(&mut cursor).find_map(find_call);
            result
        }

        let mut parser = Parser::new();
        parser
            .set_language(&language)
            .expect("테스트 언어를 설정해야 한다");
        let tree = parser
            .parse(source, None)
            .expect("테스트 AST를 생성해야 한다");
        let call = find_call(tree.root_node())?;
        call_target_name(call, source.as_bytes())
    }

    #[test]
    fn 체이닝_호출은_인자_본문이_아닌_마지막_멤버를_반환한다() {
        let language = tree_sitter_javascript::LANGUAGE.into();
        let target = first_call_target(
            language,
            "client.builder().setup({ value: veryLongObject() }).run();",
        );
        assert_eq!(target.as_deref(), Some("run"));
    }

    #[test]
    fn 단순한_멤버_호출은_수신_객체_경로를_보존한다() {
        let language = tree_sitter_javascript::LANGUAGE.into();
        let target = first_call_target(language, "service.save(order);");
        assert_eq!(target.as_deref(), Some("service.save"));
    }

    #[test]
    fn javascript_new_생성자는_구성_대상으로_보존한다() {
        let language = tree_sitter_javascript::LANGUAGE.into();
        let target = first_call_target(language, "const service = new Service(options);");
        assert_eq!(target.as_deref(), Some("Service"));
        assert_eq!(
            call_resolution_status("Service"),
            ResolutionStatus::Confirmed
        );
    }

    #[test]
    fn 복합_수신_객체는_문자열_본문을_호출명에_넣지_않는다() {
        let language = tree_sitter_javascript::LANGUAGE.into();
        let target = first_call_target(language, "(\"veryLongPromptText\").trim();");
        assert_eq!(target.as_deref(), Some("trim"));
    }

    #[test]
    fn 익명_함수_즉시_호출은_소스_본문이_아닌_동적_표식으로_남긴다() {
        let language = tree_sitter_javascript::LANGUAGE.into();
        let target = first_call_target(language, "(async () => { await veryLongOperation(); })();");
        assert_eq!(target.as_deref(), Some("[anonymous]"));
        assert_eq!(
            call_resolution_status("[anonymous]"),
            ResolutionStatus::Dynamic
        );
    }

    #[test]
    fn 동적_패턴은_식별자_경계를_지킨다() {
        assert!(matches_dynamic_pattern("eval", "eval"));
        assert!(matches_dynamic_pattern("reflect.ValueOf", "reflect."));
        assert!(matches_dynamic_pattern("method.invoke", "method.invoke"));
        assert!(!matches_dynamic_pattern("evaluate", "eval"));
        assert!(!matches_dynamic_pattern("searchRetrieval", "eval"));
        assert!(!matches_dynamic_pattern("invokeTauri", "invoke"));
        assert!(!matches_dynamic_pattern(
            "String(minuteValue).padStart",
            "eval"
        ));
    }

    #[test]
    fn 기본_동적_호출_판정이_오분류를_만들지_않는다() {
        assert_eq!(call_resolution_status("eval"), ResolutionStatus::Dynamic);
        assert_eq!(
            call_resolution_status("retrieval.search"),
            ResolutionStatus::Candidate
        );
        assert_eq!(
            call_resolution_status("self.evaluate"),
            ResolutionStatus::Candidate
        );
    }
}
