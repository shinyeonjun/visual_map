//! 코드 유닛 이름 기반 참조 해석 책임.

use super::{
    BindingKind, CodeUnit, CodeUnitKind, FactStore, Reference, ReferenceKind, ResolutionStatus,
};
use crate::model::Language;
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
        let source_language = index
            .language_by_unit
            .get(&reference.source_unit_id)
            .copied();
        let binding_target = bindings.resolve(reference, source_file_id.map(String::as_str));
        let target_name = binding_target
            .clone()
            .unwrap_or_else(|| reference.target_name.clone());
        let target = normalize_target_name(&target_name);
        let target = index.expand_relative_target(source_file_id.map(String::as_str), &target);
        if target.is_empty() {
            continue;
        }

        let mut candidates = index
            .local_candidates(
                &reference.source_unit_id,
                source_file_id.map(String::as_str),
                source_language,
                &target,
                &reference.kind,
            )
            .unwrap_or_else(|| index.candidates(source_language, &target, &reference.kind));
        if matches!(
            reference.kind,
            ReferenceKind::Import | ReferenceKind::Include
        ) {
            let qualified_import =
                target.contains('.') || target.contains("::") || target.contains('/');
            candidates.retain(|candidate| qualified_import || index.is_import_target(candidate));
        }
        candidates.sort();
        candidates.dedup();

        // Explicit import alias가 프로젝트에 포함되지 않은 모듈 경로를
        // 가리키지만, 동일한 파일 안에 그 imported symbol의 선언이 있는
        // 부분 fixture도 있다. 이 경우에만 binding이 제공한 마지막
        // symbol을 같은 lexical scope에서 보조 확인한다. 일반 bare name
        // 전역 검색은 여전히 금지한다.
        if candidates.is_empty() {
            if let Some(binding_target) = binding_target.as_deref() {
                if let Some(imported_name) = last_path_segment(binding_target) {
                    candidates = index
                        .local_candidates(
                            &reference.source_unit_id,
                            source_file_id.map(String::as_str),
                            source_language,
                            imported_name,
                            &reference.kind,
                        )
                        .unwrap_or_default();
                    candidates.sort();
                    candidates.dedup();
                }
            }
        }

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
            // 함수 내부 대입·parameter binding을 파일 전체에 노출하면
            // 다른 함수의 호출이 잘못된 대상으로 연결된다. 파일 범위
            // fallback은 import binding처럼 파일 범위를 갖는 사실만
            // 허용한다.
            if matches!(binding.kind, BindingKind::Import | BindingKind::ImportAlias) {
                if let Some(unit) = store.unit(&binding.source_unit_id) {
                    index
                        .by_file
                        .entry((unit.file_id.clone(), binding.local_name.clone()))
                        .or_default()
                        .push(binding.target_name.clone());
                }
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

fn last_path_segment(value: &str) -> Option<&str> {
    value
        .rsplit_once("::")
        .map(|(_, name)| name)
        .or_else(|| value.rsplit_once('.').map(|(_, name)| name))
        .or_else(|| value.rsplit_once('/').map(|(_, name)| name))
        .or_else(|| (!value.is_empty()).then_some(value))
        .filter(|name| !name.is_empty() && *name != "*")
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
    by_language_qualified_name: BTreeMap<(Language, String), Vec<String>>,
    by_language_file_name: BTreeMap<(Language, String, String), Vec<String>>,
    by_language_parent_name: BTreeMap<(Language, String, String), Vec<String>>,
    language_by_unit: HashMap<String, Language>,
    kind_by_unit: HashMap<String, CodeUnitKind>,
    module_by_file: HashMap<String, String>,
    parent_by_unit: HashMap<String, String>,
}

impl ReferenceResolutionIndex {
    fn build(units: &BTreeMap<String, CodeUnit>) -> Self {
        let mut index = Self::default();
        for unit in units.values() {
            index
                .language_by_unit
                .insert(unit.id.clone(), unit.language);
            index
                .kind_by_unit
                .insert(unit.id.clone(), unit.kind.clone());
            if unit.kind == CodeUnitKind::File {
                index
                    .module_by_file
                    .insert(unit.file_id.clone(), unit.qualified_name.clone());
            }
            index
                .by_language_qualified_name
                .entry((unit.language, unit.qualified_name.clone()))
                .or_default()
                .push(unit.id.clone());
            index
                .by_language_file_name
                .entry((unit.language, unit.file_id.clone(), unit.name.clone()))
                .or_default()
                .push(unit.id.clone());
            if let Some(parent_id) = &unit.parent_id {
                index
                    .by_language_parent_name
                    .entry((unit.language, parent_id.clone(), unit.name.clone()))
                    .or_default()
                    .push(unit.id.clone());
                index
                    .parent_by_unit
                    .insert(unit.id.clone(), parent_id.clone());
            }
        }

        for candidates in index.by_language_qualified_name.values_mut() {
            candidates.sort();
            candidates.dedup();
        }
        index
    }

    fn expand_relative_target(&self, source_file_id: Option<&str>, target: &str) -> String {
        let leading_dots = target
            .chars()
            .take_while(|character| *character == '.')
            .count();
        if leading_dots == 0 {
            return target.to_string();
        }
        let Some(module) = source_file_id.and_then(|file| self.module_by_file.get(file)) else {
            return target.trim_start_matches('.').to_string();
        };
        let mut parts = module.split("::").collect::<Vec<_>>();
        parts.truncate(parts.len().saturating_sub(leading_dots));
        let suffix = target.trim_start_matches('.').replace(['.', '/'], "::");
        if !suffix.is_empty() {
            parts.extend(suffix.split("::").filter(|part| !part.is_empty()));
        }
        parts.join("::")
    }

    fn is_import_target(&self, unit_id: &str) -> bool {
        self.kind_by_unit.get(unit_id).is_some_and(|kind| {
            matches!(
                kind,
                CodeUnitKind::File
                    | CodeUnitKind::Module
                    | CodeUnitKind::Package
                    | CodeUnitKind::Namespace
            )
        })
    }

    fn local_candidates(
        &self,
        source_unit_id: &str,
        source_file_id: Option<&str>,
        source_language: Option<Language>,
        target: &str,
        kind: &ReferenceKind,
    ) -> Option<Vec<String>> {
        let language = source_language?;
        let short_name = target
            .rsplit_once("::")
            .map(|(_, name)| name)
            .or_else(|| target.rsplit_once('.').map(|(_, name)| name))
            .or_else(|| target.rsplit_once('/').map(|(_, name)| name))
            .unwrap_or(target)
            .to_string();

        if target.starts_with("self.") || target.starts_with("this.") {
            let parent_id = self.parent_by_unit.get(source_unit_id)?;
            return self
                .by_language_parent_name
                .get(&(language, parent_id.clone(), short_name))
                .cloned();
        }

        if !matches!(
            kind,
            ReferenceKind::Call | ReferenceKind::Constructs | ReferenceKind::Export
        ) {
            return None;
        }

        // receiver·module 경로는 아래의 exact qualified-name 단계에서만
        // 확인한다. suffix(`save`)만으로 다른 객체의 method를 고르면 안 된다.
        if target.contains('.') || target.contains("::") || target.contains('/') {
            return None;
        }

        let mut candidates = Vec::new();

        // 같은 실행 유닛 안에 선언된 중첩 함수와, 실행 유닛의 부모
        // (파일·클래스·모듈) 안에 선언된 형제 심볼만 로컬 후보로 본다.
        // 파일 전체 이름 검색은 함수 경계를 넘어 orphan을 확정하므로
        // 사용하지 않는다.
        if let Some(children) = self.by_language_parent_name.get(&(
            language,
            source_unit_id.to_string(),
            short_name.clone(),
        )) {
            candidates.extend(children.iter().cloned());
        }
        if let Some(parent_id) = self.parent_by_unit.get(source_unit_id) {
            if let Some(siblings) =
                self.by_language_parent_name
                    .get(&(language, parent_id.clone(), short_name.clone()))
            {
                candidates.extend(siblings.iter().cloned());
            }
        }
        if matches!(
            self.kind_by_unit.get(source_unit_id),
            Some(
                CodeUnitKind::File
                    | CodeUnitKind::Module
                    | CodeUnitKind::Package
                    | CodeUnitKind::Namespace
            )
        ) {
            if let Some(file_id) = source_file_id {
                if let Some(top_level) =
                    self.by_language_file_name
                        .get(&(language, file_id.to_string(), short_name))
                {
                    candidates.extend(top_level.iter().cloned());
                }
            }
        }
        candidates.sort();
        candidates.dedup();
        Some(candidates)
    }

    fn candidates(
        &self,
        source_language: Option<Language>,
        target: &str,
        kind: &ReferenceKind,
    ) -> Vec<String> {
        let Some(language) = source_language else {
            return Vec::new();
        };

        // qualified name은 정확히 일치할 때만 후보로 반환한다. bare name을
        // 프로젝트 전체에서 찾아 하나뿐이라는 이유로 확정하지 않는다.
        if let Some(exact) = self
            .by_language_qualified_name
            .get(&(language, target.to_string()))
        {
            return exact.clone();
        }
        for qualified in [target.replace('.', "::"), target.replace('/', "::")] {
            if let Some(exact) = self.by_language_qualified_name.get(&(language, qualified)) {
                return exact.clone();
            }
        }

        if matches!(kind, ReferenceKind::Call | ReferenceKind::Constructs)
            && !target.contains(['.', ':', '/'])
        {
            return Vec::new();
        }
        Vec::new()
    }
}

fn normalize_target_name(value: &str) -> String {
    let trimmed = value
        .trim()
        .trim_matches(|character| matches!(character, '"' | '\'' | '`' | ';'))
        .trim_start_matches("./")
        .trim_start_matches("../");
    trimmed.to_string()
}
