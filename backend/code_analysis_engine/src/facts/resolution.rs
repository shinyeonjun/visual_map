//! 코드 유닛 이름 기반 참조 해석 책임.

use super::{CodeUnit, FactStore, Reference, ReferenceKind, ResolutionStatus};
use std::collections::{BTreeMap, HashMap};

pub(super) fn resolve(store: &mut FactStore) {
    let index = ReferenceResolutionIndex::build(&store.units);
    let bindings = BindingResolutionIndex::build(store);
    let source_files = store
        .units
        .values()
        .map(|unit| (unit.id.clone(), unit.file_id.clone()))
        .collect::<HashMap<_, _>>();
    for reference in &mut store.references {
        if reference.target_unit_id.is_some() || reference.status == ResolutionStatus::Dynamic {
            continue;
        }

        let source_file_id = source_files.get(&reference.source_unit_id);
        let target_name = bindings
            .resolve(reference, source_file_id.map(String::as_str))
            .unwrap_or_else(|| reference.target_name.clone());
        let target = normalize_target_name(&target_name);
        if target.is_empty() {
            continue;
        }

        let mut candidates = index
            .local_candidates(
                &reference.source_unit_id,
                source_file_id.map(String::as_str),
                &target,
            )
            .unwrap_or_else(|| index.candidates(&target));
        candidates.sort();
        candidates.dedup();

        reference.candidate_unit_ids.clear();
        match candidates.len() {
            1 => {
                reference.target_unit_id = candidates.pop();
                reference.status = ResolutionStatus::Confirmed;
            }
            2.. => {
                // Candidate는 후보가 여러 개인 상태로만 사용한다. 후보
                // 자체를 함께 보존해야 프론트가 모호성의 근거를 보여줄 수
                // 있다.
                reference.candidate_unit_ids = candidates;
                reference.target_unit_id = None;
                reference.status = ResolutionStatus::Candidate;
            }
            0 => {
                // 외부 라이브러리·분석 제외 파일·존재하지 않는 심볼은
                // 프로젝트 내부 후보가 없는 Unknown이다. 추출 단계에서
                // 임시로 Candidate였더라도 최종 해석 결과로 정규화한다.
                reference.target_unit_id = None;
                reference.status = ResolutionStatus::Unknown;
            }
        }
    }
}

/// 파일 import alias와 함수 내부 대입 binding을 참조 해석에 적용한다.
///
/// 정확한 lexical scope resolver가 없는 단계에서도 같은 함수의 대입과 파일
/// 수준 import를 분리해 보수적으로 적용한다. 중복 binding은 추측하지 않는다.
#[derive(Debug, Default)]
struct BindingResolutionIndex {
    by_scope: HashMap<(String, String), Vec<String>>,
    by_file: HashMap<(String, String), Vec<String>>,
}

impl BindingResolutionIndex {
    fn build(store: &FactStore) -> Self {
        let mut index = Self::default();
        for binding in &store.bindings {
            index
                .by_scope
                .entry((binding.source_unit_id.clone(), binding.local_name.clone()))
                .or_default()
                .push(binding.target_name.clone());
            if let Some(unit) = store.unit(&binding.source_unit_id) {
                index
                    .by_file
                    .entry((unit.file_id.clone(), binding.local_name.clone()))
                    .or_default()
                    .push(binding.target_name.clone());
            }
        }
        index
    }

    fn resolve(&self, reference: &Reference, source_file_id: Option<&str>) -> Option<String> {
        if !matches!(
            reference.kind,
            ReferenceKind::Call | ReferenceKind::Constructs
        ) {
            return None;
        }
        let target = reference.target_name.as_str();
        let (head, suffix) = split_head(target);
        let candidates = self
            .by_scope
            .get(&(reference.source_unit_id.clone(), head.to_string()))
            .or_else(|| {
                source_file_id
                    .and_then(|file_id| self.by_file.get(&(file_id.to_string(), head.to_string())))
            })?;
        let target = unique(candidates)?;
        Some(if suffix.is_empty() {
            target.to_string()
        } else {
            format!("{target}{suffix}")
        })
    }
}

fn split_head(value: &str) -> (&str, &str) {
    let separator = value
        .char_indices()
        .find(|(_, character)| *character == '.' || *character == ':')
        .map(|(index, _)| index);
    match separator {
        Some(index) => (&value[..index], &value[index..]),
        None => (value, ""),
    }
}

fn unique(values: &[String]) -> Option<&str> {
    let first = values.first()?;
    values
        .iter()
        .all(|value| value == first)
        .then_some(first.as_str())
}

/// 이름 기반 참조 해석을 위한 프로젝트 단위 인덱스다.
#[derive(Debug, Default)]
struct ReferenceResolutionIndex {
    by_name: HashMap<String, Vec<String>>,
    by_qualified_name: HashMap<String, Vec<String>>,
    by_file_name: HashMap<(String, String), Vec<String>>,
    by_parent_name: HashMap<(String, String), Vec<String>>,
    parent_by_unit: HashMap<String, String>,
}

impl ReferenceResolutionIndex {
    fn build(units: &BTreeMap<String, CodeUnit>) -> Self {
        let mut index = Self::default();
        for unit in units.values() {
            index
                .by_name
                .entry(unit.name.to_ascii_lowercase())
                .or_default()
                .push(unit.id.clone());
            index
                .by_qualified_name
                .entry(unit.qualified_name.to_ascii_lowercase())
                .or_default()
                .push(unit.id.clone());
            index
                .by_file_name
                .entry((unit.file_id.clone(), unit.name.to_ascii_lowercase()))
                .or_default()
                .push(unit.id.clone());
            if let Some(parent_id) = &unit.parent_id {
                index
                    .by_parent_name
                    .entry((parent_id.clone(), unit.name.to_ascii_lowercase()))
                    .or_default()
                    .push(unit.id.clone());
                index
                    .parent_by_unit
                    .insert(unit.id.clone(), parent_id.clone());
            }
        }

        for candidates in index.by_name.values_mut() {
            candidates.sort();
            candidates.dedup();
        }
        for candidates in index.by_qualified_name.values_mut() {
            candidates.sort();
            candidates.dedup();
        }
        index
    }

    fn local_candidates(
        &self,
        source_unit_id: &str,
        source_file_id: Option<&str>,
        target: &str,
    ) -> Option<Vec<String>> {
        let short_name = target
            .rsplit_once("::")
            .map(|(_, name)| name)
            .or_else(|| target.rsplit_once('.').map(|(_, name)| name))
            .or_else(|| target.rsplit_once('/').map(|(_, name)| name))
            .unwrap_or(target)
            .to_ascii_lowercase();

        if target.starts_with("self.") || target.starts_with("this.") {
            let parent_id = self.parent_by_unit.get(source_unit_id)?;
            return self
                .by_parent_name
                .get(&(parent_id.clone(), short_name))
                .cloned();
        }

        if target.contains("::") || target.contains('/') {
            return None;
        }

        let file_id = source_file_id?;
        let candidates = self
            .by_file_name
            .get(&(file_id.to_string(), short_name))?
            .clone();
        Some(candidates)
    }

    fn candidates(&self, target: &str) -> Vec<String> {
        // 언어마다 qualified name의 구분자가 다르다. 원본 경로를 먼저
        // 보존한 뒤 동등한 구분자 표현도 확인하고, 마지막에만 이름 fallback을
        // 사용해 서로 다른 클래스의 같은 메서드를 잘못 확정하지 않도록 한다.
        if let Some(exact) = self.by_qualified_name.get(target) {
            return exact.clone();
        }
        for qualified in [target.replace('.', "::"), target.replace('/', "::")] {
            if let Some(exact) = self.by_qualified_name.get(&qualified) {
                return exact.clone();
            }
        }

        let mut candidates = Vec::new();
        append_candidates(&mut candidates, self.by_name.get(target));
        for separator in ["::", ".", "/"] {
            if let Some((_, name)) = target.rsplit_once(separator) {
                append_candidates(&mut candidates, self.by_name.get(name));
            }
        }
        candidates
    }
}

fn append_candidates(target: &mut Vec<String>, candidates: Option<&Vec<String>>) {
    if let Some(candidates) = candidates {
        target.extend(candidates.iter().cloned());
    }
}

fn normalize_target_name(value: &str) -> String {
    let trimmed = value
        .trim()
        .trim_matches(|character| matches!(character, '"' | '\'' | '`' | ';'))
        .trim_start_matches("./")
        .trim_start_matches("../");
    trimmed.to_ascii_lowercase()
}
