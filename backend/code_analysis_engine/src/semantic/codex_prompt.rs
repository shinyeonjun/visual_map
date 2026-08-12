use super::context::SemanticContext;

/// 도메인 의미 분석용 Codex 프롬프트를 생성한다.
///
/// 프롬프트는 청크 번호를 포함해 Codex가 전체 입력 중 어느 부분을 보고
/// 있는지 알 수 있게 하며, 결과는 안정적인 ID만 참조하도록 제한한다.
pub(crate) fn build_domain_review_prompt(
    context: &SemanticContext,
    chunk_index: usize,
    chunk_count: usize,
) -> Result<String, serde_json::Error> {
    let context_json = serde_json::to_string(context)?;
    Ok(include_str!("prompts/domain_review.txt")
        .replace("{chunk_number}", &(chunk_index + 1).to_string())
        .replace("{chunk_count}", &chunk_count.to_string())
        .replace("{context_json}", &context_json))
}

#[cfg(test)]
mod tests {
    use super::build_domain_review_prompt;
    use crate::semantic::context::SemanticContext;

    #[test]
    fn 프롬프트에_청크_정보와_정적_컨텍스트가_포함된다() {
        let context = SemanticContext {
            domains: Vec::new(),
            relations: Vec::new(),
            frameworks: Vec::new(),
        };

        let prompt = build_domain_review_prompt(&context, 1, 3).expect("프롬프트를 생성해야 한다");

        assert!(prompt.contains("청크 2/3"));
        assert!(prompt.contains("\"domains\""));
        assert!(prompt.contains("\"relations\":[]"));
    }
}
