//! Codex JSONL 응답에서 도메인 이름 제안만 추출한다.

use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ReviewProposal {
    pub domains: Vec<DomainSuggestion>,
    pub features: Vec<FeatureSuggestion>,
}

impl ReviewProposal {
    pub fn merge_missing(&mut self, supplement: Self) {
        merge_by_id(&mut self.domains, supplement.domains, |item| {
            item.domain_id.as_str()
        });
        merge_by_id(&mut self.features, supplement.features, |item| {
            item.feature_id.as_str()
        });
    }
}

fn merge_by_id<T, F>(destination: &mut Vec<T>, supplement: Vec<T>, id: F)
where
    F: Fn(&T) -> &str,
{
    for item in supplement {
        if let Some(current) = destination
            .iter_mut()
            .find(|current| id(current) == id(&item))
        {
            *current = item;
        } else {
            destination.push(item);
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DomainSuggestion {
    pub domain_id: String,
    /// 여러 기술 도메인을 하나의 비즈니스 도메인으로 묶을 때 사용하는
    /// 대표 원본 domain ID다. 새 ID는 허용하지 않는다.
    #[serde(default)]
    pub canonical_domain_id: Option<String>,
    #[serde(default)]
    pub action: Option<String>,
    pub name: String,
    pub summary: Option<String>,
}

impl DomainSuggestion {
    pub fn is_demoted(&self) -> bool {
        self.action
            .as_deref()
            .is_some_and(|action| action.eq_ignore_ascii_case("demote"))
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeatureSuggestion {
    pub feature_id: String,
    pub name: String,
    #[serde(default)]
    pub summary: Option<String>,
}

pub fn parse_response(stdout: &[u8]) -> Result<ReviewProposal, String> {
    let text = String::from_utf8_lossy(stdout);
    let mut candidates = Vec::new();
    if let Ok(value) = serde_json::from_str::<Value>(&text) {
        collect_strings(&value, &mut candidates);
    }
    for line in text.lines() {
        if let Ok(value) = serde_json::from_str::<Value>(line) {
            collect_strings(&value, &mut candidates);
        }
    }
    candidates.push(text.to_string());

    for candidate in candidates.into_iter().rev() {
        let cleaned = candidate
            .trim()
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim();
        let Some(start) = cleaned.find('{') else {
            continue;
        };
        let Some(end) = cleaned.rfind('}') else {
            continue;
        };
        let candidate_json = &cleaned[start..=end];
        let Ok(value) = serde_json::from_str::<Value>(candidate_json) else {
            continue;
        };
        let has_domains = value
            .as_object()
            .is_some_and(|object| object.contains_key("domains"));
        let has_features = value
            .as_object()
            .is_some_and(|object| object.contains_key("features"));
        if has_domains || has_features {
            if let Ok(proposal) = serde_json::from_value::<ReviewProposal>(value) {
                return Ok(proposal);
            }
        }
    }

    Err("AI 응답에서 도메인 또는 기능 이름 JSON을 찾지 못했습니다.".into())
}

fn collect_strings(value: &Value, output: &mut Vec<String>) {
    match value {
        Value::String(text) => output.push(text.clone()),
        Value::Array(values) => values
            .iter()
            .for_each(|value| collect_strings(value, output)),
        Value::Object(map) => map
            .values()
            .for_each(|value| collect_strings(value, output)),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::parse_response;

    #[test]
    fn 의미_응답에서_도메인_이름을_읽는다() {
        let payload = serde_json::json!({
            "domains": [{"domainId": "domain_a", "name": "auth", "summary": "login"}],
            "features": [{"featureId": "feature_a", "name": "login", "summary": null}],
            "flows": [{"flowId": "flow_a", "name": "login flow", "summary": "validate"}]
        });
        let output = serde_json::json!({
            "type": "item.completed",
            "item": {"type": "agent_message", "text": payload.to_string()}
        })
        .to_string();
        let proposal = parse_response(output.as_bytes()).expect("의미 응답을 읽어야 한다");
        assert_eq!(proposal.domains[0].domain_id, "domain_a");
        assert_eq!(proposal.domains[0].name, "auth");
    }
}
