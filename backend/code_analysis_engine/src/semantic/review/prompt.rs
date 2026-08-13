//! 도메인·기능·실행 흐름 의미 분석 프롬프트를 만든다.

use super::context::ReviewContext;

pub fn build(
    context: &ReviewContext,
    chunk_index: usize,
    chunk_count: usize,
    maximum_name_length: usize,
    maximum_summary_length: usize,
) -> Result<String, serde_json::Error> {
    let context_json = serde_json::to_string(context)?;
    Ok(include_str!("../prompts/semantic_review.txt")
        .replace("{chunk_number}", &(chunk_index + 1).to_string())
        .replace("{chunk_count}", &chunk_count.to_string())
        .replace("{maximum_name_length}", &maximum_name_length.to_string())
        .replace(
            "{maximum_summary_length}",
            &maximum_summary_length.to_string(),
        )
        .replace("{context_json}", &context_json))
}
