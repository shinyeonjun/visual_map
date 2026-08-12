//! 클래스/컨트롤러 레벨 route prefix를 자식 endpoint에 합성한다.

use crate::facts::{CodeUnitKind, Entrypoint, FactStore};
use crate::languages::common::metadata::stable_id;
use std::collections::HashSet;

/// Java `@RequestMapping`·ASP.NET `[Route]`처럼 타입에 붙은 경로를
/// 메서드 endpoint에 적용한다. 타입 자체는 외부 진입점이 아니므로 자식
/// endpoint가 하나라도 합성되면 prefix placeholder를 제거한다.
pub fn compose_class_route_prefixes(facts: &mut FactStore, framework_ids: &[&str]) {
    let class_routes = facts
        .entrypoints
        .iter()
        .filter_map(|entrypoint| {
            let _path = entrypoint.path.as_deref()?;
            let unit = facts.unit(&entrypoint.unit_id)?;
            let class_id =
                if unit.kind == CodeUnitKind::Class || unit.kind == CodeUnitKind::Interface {
                    Some(unit.id.clone())
                } else if unit.kind == CodeUnitKind::File {
                    let line = entrypoint
                        .evidence
                        .first()
                        .map(|evidence| evidence.span.start_line)?;
                    facts
                        .units
                        .values()
                        .filter(|candidate| {
                            (candidate.kind == CodeUnitKind::Class
                                || candidate.kind == CodeUnitKind::Interface)
                                && candidate.file_id == unit.file_id
                                && candidate.span.start_line >= line
                        })
                        .min_by_key(|candidate| candidate.span.start_line)
                        .map(|candidate| candidate.id.clone())
                } else {
                    None
                }?;
            Some((entrypoint.clone(), class_id))
        })
        .filter(|(entrypoint, _)| {
            entrypoint
                .framework_id
                .as_deref()
                .is_some_and(|id| framework_ids.iter().any(|candidate| candidate == &id))
        })
        .collect::<Vec<_>>();
    if class_routes.is_empty() {
        return;
    }

    let mut remove_ids = HashSet::new();
    for (prefix_entrypoint, class_id) in class_routes {
        let prefix = prefix_entrypoint.path.as_deref().unwrap_or("/");
        let child_ids = descendant_ids(facts, &class_id);
        let mut composed = false;
        for entrypoint in &mut facts.entrypoints {
            if entrypoint.id == prefix_entrypoint.id
                || !child_ids.contains(&entrypoint.unit_id)
                || entrypoint.path.is_none()
            {
                continue;
            }
            let path = entrypoint.path.clone().unwrap_or_default();
            let full_path = join_paths(prefix, &path);
            entrypoint.path = Some(full_path.clone());
            entrypoint.name = full_path.clone();
            for evidence in &mut entrypoint.evidence {
                // 화면 표시용 route 값만 합성한다. routeSource/attribute
                // evidence는 원본 코드와의 연결이므로 절대 덮어쓰지 않는다.
                if matches!(evidence.kind.as_str(), "route" | "frameworkDecoratorRoute") {
                    evidence.value = full_path.clone();
                }
            }
            composed = true;
        }
        if composed {
            remove_ids.insert(prefix_entrypoint.id);
        }
    }
    facts
        .entrypoints
        .retain(|entrypoint| !remove_ids.contains(&entrypoint.id));

    // JAX-RS처럼 `@GET`와 `@Path("/users")`가 같은 메서드에 나뉘는
    // 문법에서는 `@GET`만 보고 만든 기본 `/` route가 구체 route와 중복된다.
    // 구체 경로가 있으면 기본 경로만 제거한다.
    let concrete_units = facts
        .entrypoints
        .iter()
        .filter(|entrypoint| {
            framework_ids
                .iter()
                .any(|framework_id| entrypoint.framework_id.as_deref() == Some(*framework_id))
                && entrypoint.path.as_deref().is_some_and(|path| path != "/")
        })
        .map(|entrypoint| entrypoint.unit_id.clone())
        .collect::<HashSet<_>>();
    facts.entrypoints.retain(|entrypoint| {
        !(concrete_units.contains(&entrypoint.unit_id)
            && entrypoint.path.as_deref() == Some("/")
            && framework_ids
                .iter()
                .any(|framework_id| entrypoint.framework_id.as_deref() == Some(*framework_id)))
    });
}

fn descendant_ids(facts: &FactStore, parent_id: &str) -> HashSet<String> {
    let mut result = HashSet::new();
    let mut pending = vec![parent_id.to_string()];
    while let Some(parent) = pending.pop() {
        for unit in facts
            .units
            .values()
            .filter(|unit| unit.parent_id.as_deref() == Some(parent.as_str()))
        {
            if result.insert(unit.id.clone()) {
                pending.push(unit.id.clone());
            }
        }
    }
    result
}

fn join_paths(prefix: &str, path: &str) -> String {
    let prefix = prefix.trim_matches('/');
    let path = path.trim_matches('/');
    if prefix.is_empty() && path.is_empty() {
        "/".to_string()
    } else if prefix.is_empty() {
        format!("/{path}")
    } else if path.is_empty() {
        format!("/{prefix}")
    } else {
        format!("/{prefix}/{path}")
    }
}

#[allow(dead_code)]
fn _stable_route_id(entrypoint: &Entrypoint) -> String {
    stable_id("entry", &entrypoint.id)
}
