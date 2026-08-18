//! ASP.NET MVC conventional `{controller}/{action}` route inference.
//!
//! Attribute route가 없는 `[HttpGet]`·`[HttpPost]` action은 기본적으로
//! `/{area?}/{controller}/{action}` 패턴으로 노출된다.

use crate::facts::{CodeUnitKind, FactStore};
use std::collections::HashMap;

const FRAMEWORK_IDS: &[&str] = &[
    "csharp.aspnet_core",
    "csharp.aspnet_mvc",
    "csharp.aspnet_web_api",
];

pub(super) fn enrich(facts: &mut FactStore) {
    let controllers = facts
        .units
        .values()
        .filter(|unit| unit.kind == CodeUnitKind::Class && is_controller_class(unit))
        .map(|unit| unit.id.clone())
        .collect::<Vec<_>>();
    if controllers.is_empty() {
        return;
    }

    let mut methods_by_parent: HashMap<String, Vec<String>> = HashMap::new();
    for unit in facts.units.values() {
        if !matches!(unit.kind, CodeUnitKind::Method | CodeUnitKind::Function) {
            continue;
        }
        if let Some(parent_id) = unit.parent_id.as_deref() {
            methods_by_parent
                .entry(parent_id.to_string())
                .or_default()
                .push(unit.id.clone());
        }
    }

    for controller_id in controllers {
        let Some(controller) = facts.unit(&controller_id) else {
            continue;
        };
        let area = area_for_controller(facts, controller);
        let controller_segment = controller_segment(&controller.name);
        let method_ids = methods_by_parent
            .get(&controller_id)
            .cloned()
            .unwrap_or_default();
        for method_id in method_ids {
            let Some(method) = facts.unit(&method_id) else {
                continue;
            };
            let action = action_name(facts, &method_id).unwrap_or_else(|| method.name.clone());
            let conventional_path =
                join_conventional_path(area.as_deref(), &controller_segment, &action);
            for entrypoint in facts.entrypoints.iter_mut() {
                if entrypoint.unit_id != method_id {
                    continue;
                }
                if !entrypoint
                    .framework_id
                    .as_deref()
                    .is_some_and(|id| FRAMEWORK_IDS.contains(&id))
                {
                    continue;
                }
                if entrypoint.kind != crate::facts::EntrypointKind::Http {
                    continue;
                }
                if !needs_conventional_path(entrypoint.path.as_deref()) {
                    continue;
                }
                entrypoint.path = Some(conventional_path.clone());
                entrypoint.name = conventional_path.clone();
                for evidence in &mut entrypoint.evidence {
                    if matches!(evidence.kind.as_str(), "route" | "frameworkDecoratorRoute") {
                        evidence.value = conventional_path.clone();
                    }
                }
            }
        }
    }
}

fn is_controller_class(unit: &crate::facts::CodeUnit) -> bool {
    if !unit.name.ends_with("Controller") {
        return false;
    }
    unit.signature
        .as_deref()
        .is_some_and(|signature| signature.contains("Controller"))
}

fn area_for_controller(facts: &FactStore, controller: &crate::facts::CodeUnit) -> Option<String> {
    for decorator in &facts.decorators {
        if decorator.unit_id != controller.id {
            continue;
        }
        if !decorator.name.eq_ignore_ascii_case("Area") {
            continue;
        }
        if let Some(value) = literal_argument(decorator.arguments.first().map(String::as_str)) {
            return Some(value);
        }
    }

    let path = controller.relative_path.replace('\\', "/");
    let segments: Vec<_> = path.split('/').collect();
    if let Some(index) = segments.iter().position(|segment| *segment == "Areas") {
        return segments.get(index + 1).map(|segment| segment.to_string());
    }
    None
}

fn controller_segment(controller_name: &str) -> String {
    controller_name
        .strip_suffix("Controller")
        .unwrap_or(controller_name)
        .to_string()
}

fn action_name(facts: &FactStore, method_id: &str) -> Option<String> {
    for decorator in &facts.decorators {
        if decorator.unit_id != method_id {
            continue;
        }
        if !decorator.name.eq_ignore_ascii_case("ActionName") {
            continue;
        }
        return literal_argument(decorator.arguments.first().map(String::as_str));
    }
    None
}

fn literal_argument(value: Option<&str>) -> Option<String> {
    let value = value?.trim();
    if value.len() < 2 {
        return None;
    }
    let first = value.chars().next()?;
    let last = value.chars().last()?;
    matches!((first, last), ('"', '"') | ('\'', '\''))
        .then(|| value[1..value.len() - 1].to_string())
}

fn join_conventional_path(area: Option<&str>, controller: &str, action: &str) -> String {
    match area {
        Some(area) => format!("/{area}/{controller}/{action}"),
        None => format!("/{controller}/{action}"),
    }
}

fn needs_conventional_path(path: Option<&str>) -> bool {
    matches!(path, None | Some("") | Some("/"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facts::{DecoratorFact, Entrypoint, EntrypointKind, Evidence, SourceSpan};
    use crate::model::Language;

    fn controller(id: &str, path: &str) -> crate::facts::CodeUnit {
        crate::facts::CodeUnit {
            id: id.into(),
            kind: CodeUnitKind::Class,
            name: "CustomerController".into(),
            qualified_name: "CustomerController".into(),
            file_id: format!("file:{id}"),
            relative_path: path.into(),
            language: Language::CSharp,
            parent_id: None,
            span: SourceSpan::new(format!("file:{id}"), path, 1, 1, 1, 1),
            body_span: None,
            signature: Some("public class CustomerController : Controller".into()),
            parameters: Vec::new(),
            return_type: None,
            visibility: Default::default(),
            modifiers: Vec::new(),
            exported: true,
        }
    }

    fn method(id: &str, parent_id: &str, name: &str, path: &str) -> crate::facts::CodeUnit {
        crate::facts::CodeUnit {
            id: id.into(),
            kind: CodeUnitKind::Method,
            name: name.into(),
            qualified_name: format!("{parent_id}::{name}"),
            file_id: format!("file:{id}"),
            relative_path: path.into(),
            language: Language::CSharp,
            parent_id: Some(parent_id.into()),
            span: SourceSpan::new(format!("file:{id}"), path, 1, 1, 1, 1),
            body_span: None,
            signature: Some(format!("public IActionResult {name}()")),
            parameters: Vec::new(),
            return_type: None,
            visibility: Default::default(),
            modifiers: Vec::new(),
            exported: true,
        }
    }

    #[test]
    fn attribute_route_없는_action은_conventional_path를_받는다() {
        let mut facts = FactStore::default();
        facts.units.insert(
            "class:customer".into(),
            controller("class:customer", "Controllers/CustomerController.cs"),
        );
        facts.units.insert(
            "method:login".into(),
            method(
                "method:login",
                "class:customer",
                "Login",
                "Controllers/CustomerController.cs",
            ),
        );
        facts.entrypoints.push(Entrypoint {
            id: "entry:login".into(),
            unit_id: "method:login".into(),
            kind: EntrypointKind::Http,
            name: "/".into(),
            method: Some("HTTPGET".into()),
            path: Some("/".into()),
            framework_id: Some("csharp.aspnet_core".into()),
            evidence: vec![Evidence::new(
                "route",
                "/",
                SourceSpan::new("file", "Controllers/CustomerController.cs", 1, 1, 1, 1),
            )],
        });

        enrich(&mut facts);

        assert_eq!(
            facts.entrypoints[0].path.as_deref(),
            Some("/Customer/Login")
        );
    }

    #[test]
    fn area_controller는_area_접두를_포함한다() {
        let mut facts = FactStore::default();
        facts.units.insert(
            "class:order".into(),
            controller("class:order", "Areas/Admin/Controllers/OrderController.cs"),
        );
        facts.units.insert(
            "method:list".into(),
            method(
                "method:list",
                "class:order",
                "List",
                "Areas/Admin/Controllers/OrderController.cs",
            ),
        );
        facts.decorators.push(DecoratorFact {
            id: "decorator:area".into(),
            unit_id: "class:order".into(),
            receiver: None,
            name: "Area".into(),
            arguments: vec!["\"Admin\"".into()],
            expression: "Area(\"Admin\")".into(),
            evidence: Vec::new(),
        });
        facts.entrypoints.push(Entrypoint {
            id: "entry:list".into(),
            unit_id: "method:list".into(),
            kind: EntrypointKind::Http,
            name: "/".into(),
            method: Some("HTTPGET".into()),
            path: Some("/".into()),
            framework_id: Some("csharp.aspnet_mvc".into()),
            evidence: Vec::new(),
        });

        enrich(&mut facts);

        assert_eq!(
            facts.entrypoints[0].path.as_deref(),
            Some("/Admin/Customer/List")
        );
    }

    #[test]
    fn actionname_속성이_있으면_메서드_이름_대신_쓴다() {
        let mut facts = FactStore::default();
        facts.units.insert(
            "class:customer".into(),
            controller("class:customer", "Controllers/CustomerController.cs"),
        );
        facts.units.insert(
            "method:post".into(),
            method(
                "method:post",
                "class:customer",
                "Configure",
                "Controllers/CustomerController.cs",
            ),
        );
        facts.decorators.push(DecoratorFact {
            id: "decorator:action".into(),
            unit_id: "method:post".into(),
            receiver: None,
            name: "ActionName".into(),
            arguments: vec!["\"Save\"".into()],
            expression: "ActionName(\"Save\")".into(),
            evidence: Vec::new(),
        });
        facts.entrypoints.push(Entrypoint {
            id: "entry:save".into(),
            unit_id: "method:post".into(),
            kind: EntrypointKind::Http,
            name: "/".into(),
            method: Some("HTTPPOST".into()),
            path: Some("/".into()),
            framework_id: Some("csharp.aspnet_core".into()),
            evidence: Vec::new(),
        });

        enrich(&mut facts);

        assert_eq!(facts.entrypoints[0].path.as_deref(), Some("/Customer/Save"));
    }
}
