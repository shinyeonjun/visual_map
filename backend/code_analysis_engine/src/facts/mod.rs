//! 언어별 분석 결과를 엔진 공통 형태로 바꾸는 사실 모델 영역이다.

pub mod control_flow;
pub mod entrypoints;
pub mod evidence;
pub mod frameworks;
pub mod relations;
mod resolution;
pub mod resources;
pub mod semantic;
pub mod units;

use crate::config::AnalysisLimits;
use crate::diagnostics::Diagnostic;
use crate::model::{Language, ParseStatus};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub use control_flow::{ControlFlowFact, ControlFlowKind};
pub use entrypoints::{Entrypoint, EntrypointKind};
pub use evidence::{Evidence, SourceSpan};
pub use frameworks::FrameworkFact;
pub use relations::{Reference, ReferenceKind, ResolutionStatus};
pub use resources::{AccessMode, ResourceAccess, ResourceKind};
pub use semantic::{BindingKind, CallSiteFact, DecoratorFact, SymbolBinding};
pub use units::{CodeParameter, CodeUnit, CodeUnitKind, CodeUnitVisibility, UnitHierarchyIndex};

/// 언어 분석기 하나가 파일에서 반환하는 사실 묶음이다.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct FactBundle {
    pub language: Option<Language>,
    pub file_id: String,
    pub parse_status: ParseStatus,
    pub bindings: Vec<SymbolBinding>,
    pub decorators: Vec<DecoratorFact>,
    pub call_sites: Vec<CallSiteFact>,
    pub units: Vec<CodeUnit>,
    pub references: Vec<Reference>,
    pub entrypoints: Vec<Entrypoint>,
    pub resources: Vec<ResourceAccess>,
    pub control_flow: Vec<ControlFlowFact>,
    pub diagnostics: Vec<Diagnostic>,
}

/// 전체 프로젝트의 공통 Facts 저장소다.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct FactStore {
    pub units: BTreeMap<String, CodeUnit>,
    pub bindings: Vec<SymbolBinding>,
    pub decorators: Vec<DecoratorFact>,
    pub call_sites: Vec<CallSiteFact>,
    pub references: Vec<Reference>,
    pub entrypoints: Vec<Entrypoint>,
    pub resources: Vec<ResourceAccess>,
    pub control_flow: Vec<ControlFlowFact>,
    pub frameworks: Vec<FrameworkFact>,
    pub diagnostics: Vec<Diagnostic>,
}

/// Fact 병합 중 전역 한도에 도달했는지 알려주는 결과다.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FactMergeStats {
    pub truncated: bool,
}

impl FactStore {
    pub fn merge(&mut self, bundle: FactBundle) {
        self.merge_with_limits(bundle, &AnalysisLimits::default());
    }

    /// 파일 Facts를 전역 한도 안에서 병합한다.
    pub fn merge_with_limits(
        &mut self,
        bundle: FactBundle,
        limits: &AnalysisLimits,
    ) -> FactMergeStats {
        let mut stats = FactMergeStats::default();
        stats.truncated |= extend_limited(&mut self.bindings, bundle.bindings, limits.max_bindings);
        stats.truncated |= extend_limited(
            &mut self.decorators,
            bundle.decorators,
            limits.max_decorators,
        );
        stats.truncated |= extend_limited(
            &mut self.call_sites,
            bundle.call_sites,
            limits.max_call_sites,
        );
        for unit in bundle.units {
            if self.units.contains_key(&unit.id) || self.units.len() < limits.max_units {
                self.units.insert(unit.id.clone(), unit);
            } else {
                stats.truncated = true;
            }
        }
        stats.truncated |= extend_limited(
            &mut self.references,
            bundle.references,
            limits.max_references,
        );
        stats.truncated |= extend_limited(
            &mut self.entrypoints,
            bundle.entrypoints,
            limits.max_entrypoints,
        );
        stats.truncated |=
            extend_limited(&mut self.resources, bundle.resources, limits.max_resources);
        stats.truncated |= extend_limited(
            &mut self.control_flow,
            bundle.control_flow,
            limits.max_control_flow_facts,
        );
        self.diagnostics.extend(bundle.diagnostics);
        stats
    }

    pub fn unit(&self, id: &str) -> Option<&CodeUnit> {
        self.units.get(id)
    }

    /// 코드 유닛의 모듈·클래스·함수 계층을 조회할 수 있는 인덱스를 만든다.
    pub fn unit_hierarchy(&self) -> UnitHierarchyIndex {
        UnitHierarchyIndex::build(self.units.values())
    }

    /// 파일별 분석기가 남긴 이름 기반 참조를 프로젝트 전체 단위 ID로 보강한다.
    ///
    /// 이름이 하나로 확정될 때만 `Confirmed`로 올린다. 같은 이름이 여러 개면
    /// `Candidate`, 프로젝트 안에서 찾지 못하면 `Unknown`으로 남긴다.
    pub fn resolve_references(&mut self) {
        resolution::resolve(self);
    }

    /// 전역 한도 적용 뒤 잘려 나간 유닛을 가리키는 고아 사실을 제거하거나
    /// 미확정 상태로 낮춘다. 한도 때문에 일부 데이터가 생략되어도 프론트가
    /// 존재하지 않는 노드 ID를 따라가며 깨지지 않게 하는 무결성 경계다.
    pub fn repair_integrity(&mut self) {
        let unit_ids = self
            .units
            .keys()
            .cloned()
            .collect::<std::collections::HashSet<_>>();
        self.bindings
            .retain(|binding| unit_ids.contains(&binding.source_unit_id));
        self.call_sites
            .retain(|call| unit_ids.contains(&call.source_unit_id));
        self.decorators
            .retain(|decorator| unit_ids.contains(&decorator.unit_id));
        self.control_flow
            .retain(|fact| unit_ids.contains(&fact.owner_unit_id));
        self.resources
            .retain(|resource| unit_ids.contains(&resource.unit_id));
        self.entrypoints
            .retain(|entrypoint| unit_ids.contains(&entrypoint.unit_id));
        self.references
            .retain(|reference| unit_ids.contains(&reference.source_unit_id));

        for reference in &mut self.references {
            reference
                .candidate_unit_ids
                .retain(|candidate| unit_ids.contains(candidate));
            if reference
                .target_unit_id
                .as_ref()
                .is_some_and(|target| !unit_ids.contains(target))
            {
                reference.target_unit_id = None;
                if reference.status != ResolutionStatus::Dynamic {
                    reference.status = if reference.candidate_unit_ids.is_empty() {
                        ResolutionStatus::Unknown
                    } else {
                        ResolutionStatus::Candidate
                    };
                }
            }
        }
    }
}

fn extend_limited<T>(target: &mut Vec<T>, values: Vec<T>, limit: usize) -> bool {
    let available = limit.saturating_sub(target.len());
    let mut truncated = false;
    for (index, value) in values.into_iter().enumerate() {
        if index < available {
            target.push(value);
        } else {
            truncated = true;
            break;
        }
    }
    truncated
}

#[cfg(test)]
mod tests {
    use super::FactStore;
    use crate::facts::SourceSpan;
    use crate::facts::{CodeUnit, CodeUnitKind, Reference, ReferenceKind, ResolutionStatus};
    use crate::model::Language;
    use std::collections::BTreeMap;

    fn unit(id: &str, name: &str, qualified_name: &str) -> CodeUnit {
        unit_with_language(id, name, qualified_name, Language::TypeScript)
    }

    fn unit_with_language(
        id: &str,
        name: &str,
        qualified_name: &str,
        language: Language,
    ) -> CodeUnit {
        CodeUnit {
            id: id.into(),
            kind: CodeUnitKind::Function,
            name: name.into(),
            qualified_name: qualified_name.into(),
            file_id: "file".into(),
            relative_path: "src/file.ts".into(),
            language,
            parent_id: None,
            span: SourceSpan::new("file", "src/file.ts", 1, 1, 1, 1),
            body_span: None,
            signature: None,
            parameters: Vec::new(),
            return_type: None,
            visibility: crate::facts::CodeUnitVisibility::Unknown,
            modifiers: Vec::new(),
            exported: false,
        }
    }

    fn reference(id: &str, target_name: &str, status: ResolutionStatus) -> Reference {
        Reference {
            id: id.into(),
            source_unit_id: "source".into(),
            target_unit_id: None,
            candidate_unit_ids: Vec::new(),
            target_name: target_name.into(),
            kind: ReferenceKind::Call,
            status,
            evidence: Vec::new(),
        }
    }

    #[test]
    fn 참조_인덱스가_정확한_이름과_qualified_이름을_해석한다() {
        let mut store = FactStore {
            units: BTreeMap::from([
                ("source".into(), unit("source", "source", "source")),
                ("unit_a".into(), unit("unit_a", "login", "Auth::login")),
                ("unit_b".into(), unit("unit_b", "login", "User::login")),
                ("unit_c".into(), unit("unit_c", "logout", "Auth::logout")),
                ("unit_d".into(), unit("unit_d", "logout", "User::logout")),
            ]),
            references: vec![
                reference("ref_1", "Auth::logout", ResolutionStatus::Confirmed),
                reference("ref_2", "./Auth::logout", ResolutionStatus::Confirmed),
                reference("ref_3", "login", ResolutionStatus::Confirmed),
                reference("ref_4", "eval", ResolutionStatus::Dynamic),
                reference("ref_5", "external_helper", ResolutionStatus::Confirmed),
            ],
            ..FactStore::default()
        };

        store.resolve_references();

        assert_eq!(
            store.references[0].target_unit_id.as_deref(),
            Some("unit_c")
        );
        assert_eq!(store.references[0].status, ResolutionStatus::Confirmed);
        assert_eq!(
            store.references[1].target_unit_id.as_deref(),
            Some("unit_c")
        );
        assert_eq!(store.references[2].status, ResolutionStatus::Candidate);
        assert_eq!(
            store.references[2].candidate_unit_ids,
            ["unit_a".to_string(), "unit_b".to_string()]
        );
        assert_eq!(store.references[3].status, ResolutionStatus::Dynamic);
        assert!(store.references[3].target_unit_id.is_none());
        assert_eq!(store.references[4].status, ResolutionStatus::Unknown);
        assert!(store.references[4].target_unit_id.is_none());
    }

    #[test]
    fn 참조_해석은_소스_언어가_다른_유닛을_확정하지_않는다() {
        let mut store = FactStore {
            units: BTreeMap::from([
                ("source".into(), unit("source", "source", "source")),
                (
                    "typescript_run".into(),
                    unit("typescript_run", "run", "run"),
                ),
                (
                    "python_run".into(),
                    unit_with_language("python_run", "run", "run", Language::Python),
                ),
            ]),
            references: vec![reference("ref_1", "run", ResolutionStatus::Confirmed)],
            ..FactStore::default()
        };

        store.resolve_references();

        assert_eq!(
            store.references[0].target_unit_id.as_deref(),
            Some("typescript_run")
        );
        assert_eq!(store.references[0].status, ResolutionStatus::Confirmed);
    }

    #[test]
    fn import는_같은_이름의_함수로_전역_fallback하지_않는다() {
        let mut store = FactStore {
            units: BTreeMap::from([
                ("source".into(), unit("source", "source", "source")),
                ("helper".into(), unit("helper", "helper", "helper")),
            ]),
            references: vec![Reference {
                kind: ReferenceKind::Import,
                ..reference("ref_1", "helper", ResolutionStatus::Candidate)
            }],
            ..FactStore::default()
        };

        store.resolve_references();

        assert!(store.references[0].target_unit_id.is_none());
        assert_eq!(store.references[0].status, ResolutionStatus::Unknown);
    }
}
