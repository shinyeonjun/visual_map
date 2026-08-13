use crate::facts::{
    BindingKind, CodeUnitKind, FactBundle, ReferenceKind, ResolutionStatus, SymbolBinding,
};
use crate::model::Language;
use std::collections::HashMap;

use super::references::matches_dynamic_pattern;

/// 모든 언어 분석 결과에 공통으로 적용하는 Facts 정규화 단계다.
pub fn normalize_bundle(bundle: &mut FactBundle) {
    normalize_language_units(bundle);
    bundle.units.sort_by(|left, right| left.id.cmp(&right.id));
    bundle.units.dedup_by(|left, right| left.id == right.id);
    bundle
        .references
        .sort_by(|left, right| left.id.cmp(&right.id));
    bundle
        .references
        .dedup_by(|left, right| left.id == right.id);
    bundle
        .entrypoints
        .sort_by(|left, right| left.id.cmp(&right.id));
    bundle
        .entrypoints
        .dedup_by(|left, right| left.id == right.id);
    bundle
        .resources
        .sort_by(|left, right| left.id.cmp(&right.id));
    bundle.resources.dedup_by(|left, right| left.id == right.id);
}

/// 공통 walker가 언어별 receiver·signature 정보를 잃지 않도록, AST를 다시
/// 파싱하지 않고도 확정 가능한 선언 종류만 보정한다. 문법 자체가 필요한
/// 분류는 각 language adapter가 담당하고 여기서는 보수적인 후처리만 한다.
fn normalize_language_units(bundle: &mut FactBundle) {
    let parent_names: HashMap<_, _> = bundle
        .units
        .iter()
        .map(|unit| (unit.id.clone(), unit.name.clone()))
        .collect();
    let property_units: std::collections::HashSet<_> = bundle
        .decorators
        .iter()
        .filter(|decorator| decorator.name.eq_ignore_ascii_case("property"))
        .map(|decorator| decorator.unit_id.clone())
        .collect();

    for unit in &mut bundle.units {
        let signature = unit.signature.as_deref().unwrap_or_default();
        match unit.language {
            Language::Go => {
                if unit.kind == CodeUnitKind::Function
                    && signature.trim_start().starts_with("func (")
                {
                    unit.kind = CodeUnitKind::Method;
                } else if unit.kind == CodeUnitKind::Record {
                    if signature.to_ascii_lowercase().contains("interface") {
                        unit.kind = CodeUnitKind::Interface;
                    } else if signature.to_ascii_lowercase().contains("struct") {
                        unit.kind = CodeUnitKind::Struct;
                    }
                }
            }
            Language::Cpp | Language::Dart | Language::JavaScript | Language::TypeScript => {
                let parent_name = unit
                    .parent_id
                    .as_ref()
                    .and_then(|parent| parent_names.get(parent));
                if unit.language == Language::Dart
                    && unit.kind == CodeUnitKind::Method
                    && unit.signature.as_deref().is_some_and(|signature| {
                        signature
                            .split_whitespace()
                            .any(|token| token == "get" || token == "set")
                    })
                {
                    unit.kind = CodeUnitKind::Property;
                } else if matches!(unit.kind, CodeUnitKind::Function | CodeUnitKind::Method)
                    && (unit.name == "constructor"
                        || parent_name.is_some_and(|name| {
                            unit.name == *name || unit.name.starts_with(&format!("{name}."))
                        }))
                {
                    unit.kind = CodeUnitKind::Constructor;
                }
            }
            Language::Python if property_units.contains(&unit.id) => {
                if matches!(unit.kind, CodeUnitKind::Function | CodeUnitKind::Method) {
                    unit.kind = CodeUnitKind::Property;
                }
            }
            _ => {}
        }
    }
}

/// 언어별 리플렉션·문자열 디스패치 패턴을 동적 경계로 표시한다.
pub fn mark_dynamic_calls(bundle: &mut FactBundle, patterns: &[String]) {
    let normalized_patterns: Vec<String> = patterns
        .iter()
        .map(|pattern| pattern.to_ascii_lowercase())
        .collect();
    let units = bundle.units.clone();
    let bindings = bundle.bindings.clone();
    for reference in &mut bundle.references {
        if reference.kind != ReferenceKind::Call {
            continue;
        }
        let local_name = call_head(&reference.target_name);
        let bound_target =
            unique_dynamic_target(&units, &bindings, &reference.source_unit_id, local_name);
        let target = bound_target
            .as_deref()
            .unwrap_or(&reference.target_name)
            .to_ascii_lowercase();
        // 명시적 import가 없는 동일 파일의 함수는 이름이 `eval`이나
        // `getattr`이어도 정적 로컬 호출이다. 반대로 import binding이
        // 있으면 같은 이름의 로컬 선언보다 binding 출처를 우선한다.
        if bound_target.is_none()
            && is_local_function(&units, &reference.source_unit_id, local_name)
        {
            continue;
        }
        if normalized_patterns
            .iter()
            .any(|pattern| matches_dynamic_pattern(&target, pattern))
        {
            reference.status = ResolutionStatus::Dynamic;
        }
    }
}

fn call_head(target: &str) -> &str {
    target
        .split_once('.')
        .map(|(head, _)| head)
        .or_else(|| target.split_once("::").map(|(head, _)| head))
        .or_else(|| target.split_once("->").map(|(head, _)| head))
        .unwrap_or(target)
}

fn unique_dynamic_target(
    units: &[crate::facts::CodeUnit],
    bindings: &[SymbolBinding],
    source_unit_id: &str,
    local_name: &str,
) -> Option<String> {
    let source_file_id = units
        .iter()
        .find(|unit| unit.id == source_unit_id)
        .map(|unit| unit.file_id.as_str());
    let mut targets = bindings
        .iter()
        .filter(|binding| {
            matches!(
                binding.kind,
                BindingKind::Import | BindingKind::ImportAlias | BindingKind::Assignment
            ) && binding.local_name == local_name
                && (binding.source_unit_id == source_unit_id
                    || source_file_id.is_some_and(|file_id| {
                        units
                            .iter()
                            .find(|unit| unit.id == binding.source_unit_id)
                            .is_some_and(|unit| unit.file_id == file_id)
                    }))
        })
        .map(|binding| binding.target_name.clone())
        .collect::<Vec<_>>();
    targets.sort();
    targets.dedup();
    (targets.len() == 1).then(|| targets.remove(0))
}

fn is_local_function(units: &[crate::facts::CodeUnit], source_unit_id: &str, name: &str) -> bool {
    let source_file_id = units
        .iter()
        .find(|unit| unit.id == source_unit_id)
        .map(|unit| unit.file_id.as_str());
    units.iter().any(|unit| {
        unit.name == name
            && unit.file_id == source_file_id.unwrap_or_default()
            && matches!(
                unit.kind,
                CodeUnitKind::Function | CodeUnitKind::Method | CodeUnitKind::Constructor
            )
    })
}

#[cfg(test)]
mod tests {
    use super::mark_dynamic_calls;
    use crate::facts::{
        BindingKind, CodeUnit, CodeUnitKind, FactBundle, Reference, ReferenceKind,
        ResolutionStatus, SourceSpan, SymbolBinding,
    };
    use crate::model::Language;

    fn unit(id: &str, name: &str) -> CodeUnit {
        CodeUnit {
            id: id.to_string(),
            kind: CodeUnitKind::Function,
            name: name.to_string(),
            qualified_name: format!("file::{name}"),
            file_id: "file".to_string(),
            relative_path: "src/file.py".to_string(),
            language: Language::Python,
            parent_id: None,
            span: SourceSpan::new("file", "src/file.py", 1, 1, 1, 1),
            body_span: None,
            signature: None,
            parameters: Vec::new(),
            return_type: None,
            visibility: crate::facts::CodeUnitVisibility::Unknown,
            modifiers: Vec::new(),
            exported: false,
        }
    }

    fn call(source_unit_id: &str, target_name: &str) -> Reference {
        Reference {
            id: format!("reference-{target_name}"),
            source_unit_id: source_unit_id.to_string(),
            target_unit_id: None,
            candidate_unit_ids: Vec::new(),
            target_name: target_name.to_string(),
            kind: ReferenceKind::Call,
            status: ResolutionStatus::Confirmed,
            evidence: Vec::new(),
        }
    }

    #[test]
    fn 로컬_eval_함수는_이름만으로_동적_경계가_되지_않는다() {
        let mut bundle = FactBundle {
            units: vec![unit("source", "invoke"), unit("local-eval", "eval")],
            references: vec![call("source", "eval")],
            ..FactBundle::default()
        };

        mark_dynamic_calls(&mut bundle, &["eval".to_string()]);

        assert_eq!(bundle.references[0].status, ResolutionStatus::Confirmed);
    }

    #[test]
    fn import된_eval은_동적_경계로_보존된다() {
        let mut bundle = FactBundle {
            units: vec![unit("source", "invoke"), unit("local-eval", "eval")],
            bindings: vec![SymbolBinding {
                id: "binding-eval".to_string(),
                source_unit_id: "source".to_string(),
                local_name: "eval".to_string(),
                target_name: "builtins::eval".to_string(),
                kind: BindingKind::Import,
                evidence: Vec::new(),
            }],
            references: vec![call("source", "eval")],
            ..FactBundle::default()
        };

        mark_dynamic_calls(&mut bundle, &["eval".to_string()]);

        assert_eq!(bundle.references[0].status, ResolutionStatus::Dynamic);
    }

    #[test]
    fn 동적_api의_대입_alias도_동적_경계로_보존된다() {
        let mut bundle = FactBundle {
            units: vec![unit("source", "invoke")],
            bindings: vec![SymbolBinding {
                id: "binding-require".to_string(),
                source_unit_id: "source".to_string(),
                local_name: "runtimeRequire".to_string(),
                target_name: "require".to_string(),
                kind: BindingKind::Assignment,
                evidence: Vec::new(),
            }],
            references: vec![call("source", "runtimeRequire")],
            ..FactBundle::default()
        };

        mark_dynamic_calls(&mut bundle, &["require".to_string()]);

        assert_eq!(bundle.references[0].status, ResolutionStatus::Dynamic);
    }
}
