//! 도메인·모듈 이름 전용 Codex 프롬프트.

use super::context::NameContext;

pub(super) fn build(
    context: &NameContext,
    chunk_index: usize,
    chunk_count: usize,
) -> Result<String, serde_json::Error> {
    let context_json = serde_json::to_string(context)?;
    Ok(include_str!("../prompts/name_review.txt")
        .replace("{chunk_number}", &(chunk_index + 1).to_string())
        .replace("{chunk_count}", &chunk_count.to_string())
        .replace("{context_json}", &context_json))
}
