//! 코드 유닛 이름 기반 참조 해석 책임.

use super::{
    BindingKind, CodeUnit, CodeUnitKind, FactStore, Reference, ReferenceKind, ResolutionStatus,
};
use crate::model::Language;
use rayon::prelude::*;
use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap};

pub(super) fn resolve(store: &mut FactStore) {
    let index = ReferenceResolutionIndex::build(&store.units);
    let bindings = BindingResolutionIndex::build(store);
    store
        .references
        .par_iter_mut()
        .for_each(|reference| resolve_one(reference, &index, &bindings));
}

fn resolve_one(
    reference: &mut Reference,
    index: &ReferenceResolutionIndex,
    bindings: &BindingResolutionIndex,
) {
    if reference.target_unit_id.is_some() {
        return;
    }
    let preserve_dynamic = reference.status == ResolutionStatus::Dynamic;

    let source_file_id = index.file_by_unit.get(&reference.source_unit_id);
    let source_language = index
        .language_by_unit
        .get(&reference.source_unit_id)
        .copied();
    let binding_target = bindings.resolve(reference, source_file_id.map(String::as_str));
    let target_name = binding_target
        .as_deref()
        .unwrap_or(reference.target_name.as_str());
    let target = normalize_target_name(target_name);
    let target = index.expand_relative_target(source_file_id.map(String::as_str), target);
    if target.is_empty() {
        return;
    }

    let mut candidates = index
        .local_candidates(
            &reference.source_unit_id,
            source_file_id.map(String::as_str),
            source_language,
            target.as_ref(),
            &reference.kind,
        )
        .unwrap_or_else(|| index.candidates(source_language, target.as_ref(), &reference.kind));
    if matches!(
        reference.kind,
        ReferenceKind::Import | ReferenceKind::Include
    ) {
        let qualified_import =
            target.contains('.') || target.contains("::") || target.contains('/');
        candidates.retain(|candidate| qualified_import || index.is_import_target(candidate));
        sort_dedup(&mut candidates);
    }

    reference.candidate_unit_ids.clear();
    match candidates.len() {
        1 => {
            reference.target_unit_id = candidates.pop();
            reference.status = if preserve_dynamic {
                ResolutionStatus::Dynamic
            } else {
                ResolutionStatus::Confirmed
            };
        }
        2.. => {
            // Candidate는 후보가 여러 개인 상태로만 사용한다. 후보
            // 자체를 함께 보존해야 프론트가 모호성의 근거를 보여줄 수
            // 있다.
            reference.candidate_unit_ids = candidates;
            reference.target_unit_id = None;
            reference.status = if preserve_dynamic {
                ResolutionStatus::Dynamic
            } else {
                ResolutionStatus::Candidate
            }
        }
        0 => {
            // 외부 라이브러리·분석 제외 파일·존재하지 않는 심볼은
            // 프로젝트 내부 후보가 없는 Unknown이다. 추출 단계에서
            // 임시로 Candidate였더라도 최종 해석 결과로 정규화한다.
            reference.target_unit_id = None;
            reference.status = if preserve_dynamic {
                ResolutionStatus::Dynamic
            } else {
                ResolutionStatus::Unknown
            };
        }
    }
}

fn sort_dedup(candidates: &mut Vec<String>) {
    if candidates.len() > 1 {
        candidates.sort_unstable();
        candidates.dedup();
    }
}

/// 파일 import alias와 함수 내부 대입 binding을 참조 해석에 적용한다.
///
/// 정확한 lexical scope resolver가 없는 단계에서도 같은 함수의 대입과 파일
/// 수준 import를 분리해 보수적으로 적용한다. 중복 binding은 추측하지 않는다.
#[derive(Debug, Default)]
struct BindingResolutionIndex {
    by_scope: HashMap<String, HashMap<String, Vec<String>>>,
    by_file: HashMap<String, HashMap<String, Vec<String>>>,
}

impl BindingResolutionIndex {
    fn build(store: &FactStore) -> Self {
        let mut index = Self::default();
        for binding in &store.bindings {
            index
                .by_scope
                .entry(binding.source_unit_id.clone())
                .or_default()
                .entry(binding.local_name.clone())
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
                        .entry(unit.file_id.clone())
                        .or_default()
                        .entry(binding.local_name.clone())
                        .or_default()
                        .push(binding.target_name.clone());
                }
            }
        }
        index
    }

    fn resolve<'a>(
        &'a self,
        reference: &Reference,
        source_file_id: Option<&str>,
    ) -> Option<Cow<'a, str>> {
        if !matches!(
            reference.kind,
            ReferenceKind::Call
                | ReferenceKind::Constructs
                | ReferenceKind::Uses
                | ReferenceKind::Implements
                | ReferenceKind::Extends
        ) {
            return None;
        }
        let target = reference.target_name.as_str();
        let (head, suffix) = split_head(target);
        let candidates = self
            .by_scope
            .get(&reference.source_unit_id)
            .and_then(|names| names.get(head))
            .or_else(|| {
                source_file_id
                    .and_then(|file_id| self.by_file.get(file_id).and_then(|names| names.get(head)))
            })?;
        let target = unique(candidates)?;
        Some(if suffix.is_empty() {
            Cow::Borrowed(target)
        } else {
            Cow::Owned(format!("{target}{suffix}"))
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
    by_language_qualified_name: HashMap<Language, HashMap<String, Vec<String>>>,
    by_language_qualified_suffix: HashMap<Language, HashMap<String, Vec<String>>>,
    by_file_name: HashMap<String, HashMap<String, Vec<String>>>,
    by_parent_name: HashMap<String, HashMap<String, Vec<String>>>,
    injected_receiver_types: HashMap<String, HashMap<String, String>>,
    language_by_unit: HashMap<String, Language>,
    kind_by_unit: HashMap<String, CodeUnitKind>,
    file_by_unit: HashMap<String, String>,
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
            index
                .file_by_unit
                .insert(unit.id.clone(), unit.file_id.clone());
            if unit.kind == CodeUnitKind::File {
                index
                    .module_by_file
                    .insert(unit.file_id.clone(), unit.qualified_name.clone());
            }
            index
                .by_language_qualified_name
                .entry(unit.language)
                .or_default()
                .entry(unit.qualified_name.clone())
                .or_default()
                .push(unit.id.clone());
            insert_qualified_suffixes(
                &mut index.by_language_qualified_suffix,
                unit.language,
                &unit.qualified_name,
                &unit.id,
            );
            index
                .by_file_name
                .entry(unit.file_id.clone())
                .or_default()
                .entry(unit.name.clone())
                .or_default()
                .push(unit.id.clone());
            if let Some(parent_id) = &unit.parent_id {
                index
                    .by_parent_name
                    .entry(parent_id.clone())
                    .or_default()
                    .entry(unit.name.clone())
                    .or_default()
                    .push(unit.id.clone());
                index
                    .parent_by_unit
                    .insert(unit.id.clone(), parent_id.clone());
            }
            if unit.kind == CodeUnitKind::Constructor {
                if let Some(class_id) = &unit.parent_id {
                    for parameter in &unit.parameters {
                        let Some(type_name) = parameter
                            .type_annotation
                            .as_deref()
                            .map(str::trim)
                            .filter(|value| !value.is_empty())
                        else {
                            continue;
                        };
                        index
                            .injected_receiver_types
                            .entry(class_id.clone())
                            .or_default()
                            .insert(parameter.name.clone(), type_name.to_string());
                    }
                }
            }
        }

        for names in index.by_language_qualified_name.values_mut() {
            sort_candidate_lists(names);
        }
        for names in index.by_language_qualified_suffix.values_mut() {
            sort_candidate_lists(names);
        }
        for names in index.by_file_name.values_mut() {
            sort_candidate_lists(names);
        }
        for names in index.by_parent_name.values_mut() {
            sort_candidate_lists(names);
        }
        index
    }

    fn expand_relative_target<'a>(
        &self,
        source_file_id: Option<&str>,
        target: &'a str,
    ) -> Cow<'a, str> {
        let leading_dots = target
            .chars()
            .take_while(|character| *character == '.')
            .count();
        if leading_dots == 0 {
            return Cow::Borrowed(target);
        }
        let Some(module) = source_file_id.and_then(|file| self.module_by_file.get(file)) else {
            return Cow::Borrowed(
                target
                    .strip_prefix("./")
                    .or_else(|| target.strip_prefix("../"))
                    .unwrap_or(target),
            );
        };
        let mut parts = module.split("::").collect::<Vec<_>>();
        parts.truncate(parts.len().saturating_sub(leading_dots));
        let suffix = target.trim_start_matches('.').replace(['.', '/'], "::");
        if !suffix.is_empty() {
            parts.extend(suffix.split("::").filter(|part| !part.is_empty()));
        }
        Cow::Owned(parts.join("::"))
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
            .unwrap_or(target);

        if target.starts_with("self.") || target.starts_with("this.") {
            let parent_id = self.parent_by_unit.get(source_unit_id)?;
            let body = target
                .strip_prefix("self.")
                .or_else(|| target.strip_prefix("this."))
                .unwrap_or(target);
            if let Some((receiver, method)) = body.rsplit_once('.') {
                let injected = self.injected_method_candidates(
                    parent_id,
                    receiver,
                    method,
                    language,
                );
                if !injected.is_empty() {
                    return Some(injected);
                }
            }
            return Some(self.parent_named(parent_id, short_name, language));
        }

        if !matches!(
            kind,
            ReferenceKind::Call
                | ReferenceKind::Constructs
                | ReferenceKind::Export
                | ReferenceKind::Uses
                | ReferenceKind::Implements
                | ReferenceKind::Extends
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
        if let Some(children) = self
            .by_parent_name
            .get(source_unit_id)
            .and_then(|names| names.get(short_name))
        {
            candidates.extend(self.same_language(children, language));
        }
        if let Some(parent_id) = self.parent_by_unit.get(source_unit_id) {
            if let Some(siblings) = self
                .by_parent_name
                .get(parent_id)
                .and_then(|names| names.get(short_name))
            {
                candidates.extend(self.same_language(siblings, language));
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
                if let Some(top_level) = self
                    .by_file_name
                    .get(file_id)
                    .and_then(|names| names.get(short_name))
                {
                    candidates.extend(self.same_language(top_level, language));
                }
            }
        }
        sort_dedup(&mut candidates);
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
        let names = self.by_language_qualified_name.get(&language);

        // qualified name은 정확히 일치할 때만 후보로 반환한다. bare name을
        // 프로젝트 전체에서 찾아 하나뿐이라는 이유로 확정하지 않는다.
        if let Some(exact) = names.and_then(|names| names.get(target)) {
            return exact.clone();
        }
        if target.contains('.') {
            if let Some(exact) = names.and_then(|names| names.get(&target.replace('.', "::"))) {
                return exact.clone();
            }
        }
        if target.contains('/') {
            if let Some(exact) = names.and_then(|names| names.get(&target.replace('/', "::"))) {
                return exact.clone();
            }
        }

        // exact match 실패 시 suffix matching: import 경로에 패키지 루트
        // 접두사가 빠진 경우(예: services::X vs backend::services::X)를 처리한다.
        let normalized = if target.contains('.') || target.contains('/') {
            Cow::Owned(target.replace(['.', '/'], "::"))
        } else {
            Cow::Borrowed(target)
        };
        if normalized.contains("::") {
            if let Some(suffix_matches) = self
                .by_language_qualified_suffix
                .get(&language)
                .and_then(|suffixes| suffixes.get(normalized.as_ref()))
            {
                return suffix_matches.clone();
            }
        }

        if matches!(kind, ReferenceKind::Call | ReferenceKind::Constructs)
            && !target.contains(['.', ':', '/'])
        {
            return Vec::new();
        }
        Vec::new()
    }

    fn parent_named(&self, parent_id: &str, name: &str, language: Language) -> Vec<String> {
        self.by_parent_name
            .get(parent_id)
            .and_then(|names| names.get(name))
            .map(|ids| self.same_language(ids, language))
            .unwrap_or_default()
    }

    fn injected_method_candidates(
        &self,
        class_id: &str,
        receiver: &str,
        method: &str,
        language: Language,
    ) -> Vec<String> {
        let Some(type_name) = self
            .injected_receiver_types
            .get(class_id)
            .and_then(|fields| fields.get(receiver))
        else {
            return Vec::new();
        };
        let mut candidates = Vec::new();
        for class_unit in self.class_units_for_type(language, type_name) {
            candidates.extend(self.parent_named(&class_unit, method, language));
        }
        sort_dedup(&mut candidates);
        candidates
    }

    fn class_units_for_type(&self, language: Language, type_name: &str) -> Vec<String> {
        let short = type_name
            .split('<')
            .next()
            .unwrap_or(type_name)
            .rsplit('.')
            .next()
            .unwrap_or(type_name)
            .trim();
        if short.is_empty() {
            return Vec::new();
        }
        let suffix = format!("::{short}");
        self.by_language_qualified_name
            .get(&language)
            .into_iter()
            .flatten()
            .filter(|(qualified, _)| qualified.as_str() == short || qualified.ends_with(&suffix))
            .flat_map(|(_, ids)| ids.iter().cloned())
            .filter(|id| self.kind_by_unit.get(id) == Some(&CodeUnitKind::Class))
            .collect()
    }

    fn same_language(&self, unit_ids: &[String], language: Language) -> Vec<String> {
        unit_ids
            .iter()
            .filter(|unit_id| self.language_by_unit.get(*unit_id) == Some(&language))
            .cloned()
            .collect()
    }
}

fn sort_candidate_lists(names: &mut HashMap<String, Vec<String>>) {
    for candidates in names.values_mut() {
        if candidates.len() > 1 {
            candidates.sort_unstable();
            candidates.dedup();
        }
    }
}

fn insert_qualified_suffixes(
    index: &mut HashMap<Language, HashMap<String, Vec<String>>>,
    language: Language,
    qualified_name: &str,
    unit_id: &str,
) {
    let parts = qualified_name
        .split("::")
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if parts.len() < 3 {
        return;
    }
    for start in 1..parts.len() - 1 {
        let suffix = parts[start..].join("::");
        index
            .entry(language)
            .or_default()
            .entry(suffix)
            .or_default()
            .push(unit_id.to_string());
    }
}

fn normalize_target_name(value: &str) -> &str {
    value
        .trim()
        .trim_matches(|character| matches!(character, '"' | '\'' | '`' | ';'))
}
