//! 호출 사실을 HTTP/WebSocket 진입점으로 materialize하는 공통 adapter 도구.

use crate::facts::{CodeUnitKind, Entrypoint, EntrypointKind, FactStore};
use crate::frameworks::registry::detector::FrameworkDetection;
use crate::languages::common::metadata::stable_id;
use std::collections::{HashMap, HashSet};

use super::route_path::{is_plausible_http_path, join_paths, string_literal};
use super::FrameworkApplicabilityIndex;

/// 한 프레임워크의 호출 기반 route 규칙이다.
#[derive(Debug, Clone)]
pub struct CallRouteRule {
    pub framework_id: &'static str,
    pub call_names: &'static [&'static str],
    /// 생성자 근거가 없는 legacy/전역 등록 API에서만 이름 기반 receiver
    /// 힌트를 사용한다. 생성자 근거가 있는 프레임워크는 실제 binding을
    /// 통해 receiver를 확정한다.
    pub receiver_names: &'static [&'static str],
    pub receiver_constructors: &'static [&'static str],
    pub route_methods: &'static [&'static str],
    pub fixed_method: Option<&'static str>,
    pub path_argument_index: usize,
    pub target_argument_index: Option<usize>,
    pub prefix_methods: &'static [&'static str],
    pub kind: EntrypointKind,
}

/// 감지된 프레임워크가 실제 프로젝트에 있을 때만 호출 기반 route를 추가한다.
pub fn add_call_routes(
    facts: &mut FactStore,
    detections: &[FrameworkDetection],
    rule: CallRouteRule,
) {
    if !detections
        .iter()
        .any(|detection| detection.id == rule.framework_id)
    {
        return;
    }
    let applicability = FrameworkApplicabilityIndex::new(detections);

    let prefixes = collect_prefixes(facts, rule.prefix_methods);
    let file_prefixes = collect_file_prefixes(facts, rule.prefix_methods);
    let unit_index = RouteUnitIndex::new(facts);
    let allowed_receivers = collect_allowed_receivers(facts, &rule);
    let mut existing_ids = facts
        .entrypoints
        .iter()
        .map(|entrypoint| entrypoint.id.clone())
        .collect::<HashSet<_>>();
    let calls = facts.call_sites.clone();
    for call in &calls {
        let applies = applicability.applies(facts, &call.source_unit_id, rule.framework_id);
        if !applies {
            continue;
        }
        let call_name = call_method_name(&call.callee);
        if !rule.call_names.is_empty()
            && !rule
                .call_names
                .iter()
                .any(|candidate| candidate.eq_ignore_ascii_case(call_name))
        {
            continue;
        }
        let method = rule
            .fixed_method
            .map(str::to_string)
            .or_else(|| route_method(&call.callee, rule.route_methods));
        let Some(method) = method else {
            continue;
        };
        if !allowed_receivers.is_empty()
            && !call_receiver(&call.callee).is_some_and(|receiver| {
                allowed_receivers.contains(&(call.source_unit_id.clone(), receiver.to_string()))
            })
        {
            continue;
        }
        let path = call
            .arguments
            .get(rule.path_argument_index)
            .and_then(|argument| string_literal(argument))
            .or_else(|| alternate_route_literal(call, rule.path_argument_index));
        let receiver = call_receiver(&call.callee);
        let prefix = receiver
            .and_then(|name| prefixes.get(&(call.source_unit_id.clone(), name.to_string())))
            .cloned()
            .or_else(|| {
                source_file_id(facts, &call.source_unit_id)
                    .and_then(|file_id| file_prefixes.get(file_id).cloned())
            })
            .or_else(|| endpoint_group_prefix(facts, &call.source_unit_id, rule.framework_id));
        let full_path = compose_route_path(prefix.as_deref(), path.as_deref());
        if full_path
            .as_deref()
            .is_some_and(|value| !is_plausible_http_path(value))
        {
            continue;
        }
        let display_path = full_path.clone().unwrap_or_else(|| "<dynamic>".to_string());
        let unit_id = rule
            .target_argument_index
            .and_then(|target_index| {
                unit_index.resolve(
                    &call.source_unit_id,
                    call.arguments.get(target_index).map(String::as_str),
                )
            })
            .unwrap_or_else(|| call.source_unit_id.clone());

        let id = stable_id(
            "entry",
            &format!(
                "{}:{}:{}:{}",
                call.id, method, display_path, rule.framework_id
            ),
        );
        if !existing_ids.insert(id.clone()) {
            continue;
        }
        let generic_ids = facts
            .entrypoints
            .iter()
            .filter(|entrypoint| {
                (entrypoint.framework_id.is_none()
                    && entrypoint.unit_id == unit_id
                    && entrypoint.kind == rule.kind
                    && entrypoint.path == full_path
                    && entrypoint.method.as_deref() == Some(method.as_str()))
                    || (entrypoint
                        .evidence
                        .first()
                        .is_some_and(|evidence| evidence.kind == "route")
                        && entrypoint.unit_id == unit_id
                        && entrypoint.kind == rule.kind
                        && entrypoint.path == full_path
                        && entrypoint.method.as_deref() == Some(method.as_str()))
            })
            .map(|entrypoint| entrypoint.id.clone())
            .collect::<HashSet<_>>();
        facts
            .entrypoints
            .retain(|entrypoint| !generic_ids.contains(&entrypoint.id));
        let evidence = call
            .evidence
            .first()
            .cloned()
            .map(|mut evidence| {
                evidence.kind = "frameworkRoute".to_string();
                evidence.value = display_path.clone();
                evidence
            })
            .into_iter()
            .collect();
        facts.entrypoints.push(Entrypoint {
            id,
            unit_id,
            kind: rule.kind.clone(),
            name: display_path,
            method: Some(method),
            path: full_path,
            framework_id: Some(rule.framework_id.to_string()),
            evidence,
        });
    }
}

fn collect_allowed_receivers(facts: &FactStore, rule: &CallRouteRule) -> HashSet<(String, String)> {
    if rule.receiver_names.is_empty() && rule.receiver_constructors.is_empty() {
        return HashSet::new();
    }

    let mut allowed = if rule.receiver_constructors.is_empty() {
        facts
            .call_sites
            .iter()
            .filter_map(|call| {
                let receiver = call_receiver(&call.callee)?;
                rule.receiver_names
                    .iter()
                    .any(|name| name.eq_ignore_ascii_case(receiver))
                    .then(|| (call.source_unit_id.clone(), receiver.to_string()))
            })
            .collect::<HashSet<_>>()
    } else {
        HashSet::new()
    };

    for call in &facts.call_sites {
        if call.assigned_name.is_none() {
            let receiver = call_receiver(&call.callee);
            if let Some(receiver) = receiver {
                if let Some(unit) = facts.unit(&call.source_unit_id) {
                    let parameter_type_matches = unit.parameters.iter().any(|parameter| {
                        parameter.name == receiver
                            && parameter
                                .type_annotation
                                .as_deref()
                                .is_some_and(|type_name| {
                                    rule.receiver_constructors.iter().any(|constructor| {
                                        type_name_matches(type_name, constructor)
                                    })
                                })
                    });
                    let signature_type_matches =
                        unit.signature.as_deref().is_some_and(|signature| {
                            signature_has_typed_parameter(
                                signature,
                                receiver,
                                rule.receiver_constructors,
                            )
                        });
                    if parameter_type_matches || signature_type_matches {
                        allowed.insert((call.source_unit_id.clone(), receiver.to_string()));
                    }
                }
            }
            continue;
        }
        let method = call_method_name(&call.callee);
        let imported_method = imported_call_method(facts, call);
        if rule.receiver_constructors.iter().any(|name| {
            name.eq_ignore_ascii_case(method)
                || imported_method
                    .as_deref()
                    .is_some_and(|candidate| name.eq_ignore_ascii_case(candidate))
        }) {
            allowed.insert((
                call.source_unit_id.clone(),
                call.assigned_name.clone().unwrap_or_default(),
            ));
        }
    }

    // `app.Group(...)`·`router.use(...)`처럼 framework 객체에서 파생된
    // router도 route 수신 객체로 승격한다.
    for _ in 0..facts.call_sites.len().min(8) {
        let mut changed = false;
        for call in &facts.call_sites {
            let Some(receiver) = call_receiver(&call.callee) else {
                continue;
            };
            if !allowed.contains(&(call.source_unit_id.clone(), receiver.to_string())) {
                continue;
            }
            if !rule
                .prefix_methods
                .iter()
                .any(|name| name.eq_ignore_ascii_case(call_method_name(&call.callee)))
            {
                continue;
            }
            let Some(assigned_name) = call.assigned_name.as_deref() else {
                continue;
            };
            changed |= allowed.insert((call.source_unit_id.clone(), assigned_name.to_string()));
        }
        if !changed {
            break;
        }
    }
    allowed
}

fn type_name_matches(type_name: &str, expected: &str) -> bool {
    type_name
        .rsplit(['.', ':'])
        .next()
        .is_some_and(|name| name.eq_ignore_ascii_case(expected))
}

fn signature_has_typed_parameter(
    signature: &str,
    parameter_name: &str,
    expected_types: &[&str],
) -> bool {
    let tokens = signature
        .split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    tokens.windows(2).any(|window| {
        let parameter_first = window[0].eq_ignore_ascii_case(parameter_name)
            && expected_types
                .iter()
                .any(|expected| window[1].eq_ignore_ascii_case(expected));
        let type_first = expected_types
            .iter()
            .any(|expected| window[0].eq_ignore_ascii_case(expected))
            && window[1].eq_ignore_ascii_case(parameter_name);
        parameter_first || type_first
    })
}

fn imported_call_method(facts: &FactStore, call: &crate::facts::CallSiteFact) -> Option<String> {
    let head = call_head(&call.callee);
    let source_file_id = facts
        .unit(&call.source_unit_id)
        .map(|unit| unit.file_id.as_str());
    let mut targets = facts
        .bindings
        .iter()
        .filter(|binding| {
            matches!(
                binding.kind,
                crate::facts::BindingKind::Import | crate::facts::BindingKind::ImportAlias
            ) && binding.local_name == head
                && (binding.source_unit_id == call.source_unit_id
                    || source_file_id.is_some_and(|file_id| {
                        facts
                            .unit(&binding.source_unit_id)
                            .is_some_and(|unit| unit.file_id == file_id)
                    }))
        })
        .map(|binding| binding.target_name.clone())
        .collect::<Vec<_>>();
    targets.sort();
    targets.dedup();
    let target = (targets.len() == 1).then(|| targets.remove(0))?;
    let normalized_target = target.replace("::", ".");
    let target = normalized_target
        .trim_end_matches(".*")
        .trim_end_matches("::*");
    let target = target
        .strip_suffix(".default")
        .or_else(|| target.strip_suffix("::default"))
        .unwrap_or(target);
    Some(call_method_name(target).to_string())
}

fn call_head(callee: &str) -> &str {
    callee
        .split_once('.')
        .map(|(head, _)| head)
        .or_else(|| callee.split_once("::").map(|(head, _)| head))
        .or_else(|| callee.split_once("->").map(|(head, _)| head))
        .unwrap_or(callee)
}

fn collect_prefixes(
    facts: &FactStore,
    prefix_methods: &[&str],
) -> HashMap<(String, String), String> {
    let mut prefixes = HashMap::new();
    for call in &facts.call_sites {
        let method = call_method_name(&call.callee);
        if !prefix_methods
            .iter()
            .any(|candidate| candidate.eq_ignore_ascii_case(method))
        {
            continue;
        }
        let Some(path) = call
            .arguments
            .first()
            .and_then(|argument| string_literal(argument))
        else {
            continue;
        };
        // `router := parent.Group("/api")`처럼 새 router를 반환하는
        // 호출은 assigned_name을 사용한다. 반면 Express/Go의
        // `app.use("/api", router)`·`r.Mount("/api", router)`는 반환값을
        // 대입하지 않고 두 번째 인자로 mount 대상을 전달한다.
        let local_name = call.assigned_name.as_deref().or_else(|| {
            call.arguments
                .get(1)
                .and_then(|argument| simple_name(argument))
        });
        let Some(local_name) = local_name else {
            continue;
        };
        let receiver_prefix = call_receiver(&call.callee)
            .and_then(|receiver| prefixes.get(&(call.source_unit_id.clone(), receiver.to_string())))
            .cloned();
        let full_path = receiver_prefix
            .as_deref()
            .map(|prefix| join_paths(prefix, &path))
            .unwrap_or(path);
        prefixes.insert(
            (call.source_unit_id.clone(), local_name.to_string()),
            full_path,
        );
    }
    prefixes
}

/// 다른 파일의 router를 `app.use("/api", router)`처럼 mount한 경우의
/// 파일 단위 prefix를 만든다.
///
/// 언어 공통 Facts에는 모듈 해석기가 없으므로 상대 경로 import와 파일
/// 유닛만 사용한다. 같은 파일에 서로 다른 prefix로 여러 번 mount되는
/// 경우에는 하나를 추측하지 않고 제외한다.
fn collect_file_prefixes(facts: &FactStore, prefix_methods: &[&str]) -> HashMap<String, String> {
    let file_units = facts
        .units
        .values()
        .filter(|unit| unit.kind == CodeUnitKind::File)
        // route resolver의 file_prefixes 키는 파일 유닛 ID가 아니라
        // 모든 코드 유닛에 공통으로 들어 있는 file_id(파일 식별자)다.
        .map(|unit| (unit.relative_path.replace('\\', "/"), unit.file_id.clone()))
        .collect::<HashMap<_, _>>();
    let mut candidates: HashMap<String, Option<String>> = HashMap::new();

    for call in &facts.call_sites {
        let method = call_method_name(&call.callee);
        if !prefix_methods
            .iter()
            .any(|candidate| candidate.eq_ignore_ascii_case(method))
            || call.assigned_name.is_some()
        {
            continue;
        }
        let Some(path) = call
            .arguments
            .first()
            .and_then(|argument| string_literal(argument))
        else {
            continue;
        };
        let Some(local_name) = call
            .arguments
            .get(1)
            .and_then(|argument| simple_name(argument))
        else {
            continue;
        };
        let Some(source_file) = source_file_unit(facts, &call.source_unit_id) else {
            continue;
        };
        let Some(import) = facts.references.iter().find(|reference| {
            reference.kind == crate::facts::ReferenceKind::Import
                && reference.source_unit_id == source_file.id
                && reference.target_name.starts_with('.')
                && reference
                    .evidence
                    .iter()
                    .any(|evidence| import_declares_local(&evidence.value, local_name))
        }) else {
            continue;
        };
        let Some(target_path) =
            resolve_relative_module(&file_units, &source_file.relative_path, &import.target_name)
        else {
            continue;
        };
        let entry = candidates
            .entry(target_path)
            .or_insert_with(|| Some(path.clone()));
        if entry.as_deref() != Some(path.as_str()) {
            *entry = None;
        }
    }

    candidates
        .into_iter()
        .filter_map(|(file_id, prefix)| prefix.map(|prefix| (file_id, prefix)))
        .collect()
}

fn source_file_id<'a>(facts: &'a FactStore, unit_id: &str) -> Option<&'a str> {
    facts.unit(unit_id).map(|unit| unit.file_id.as_str())
}

fn source_file_unit<'a>(facts: &'a FactStore, unit_id: &str) -> Option<&'a crate::facts::CodeUnit> {
    let file_id = source_file_id(facts, unit_id)?;
    facts
        .units
        .values()
        .find(|unit| unit.kind == CodeUnitKind::File && unit.file_id == file_id)
}

fn import_declares_local(import_text: &str, local_name: &str) -> bool {
    let before_from = import_text
        .split_once(" from ")
        .or_else(|| import_text.split_once("\tfrom "))
        .map(|(head, _)| head)
        .unwrap_or(import_text)
        .trim();
    let body = before_from
        .strip_prefix("import")
        .unwrap_or(before_from)
        .trim();
    if body.starts_with("*") {
        return body
            .split_once(" as ")
            .is_some_and(|(_, value)| value.trim() == local_name);
    }
    if let (Some(open), Some(close)) = (body.find('{'), body.rfind('}')) {
        return body[open + 1..close].split(',').any(|part| {
            let mut names = part.trim().split(" as ").map(str::trim);
            names.next() == Some(local_name) || names.next().is_some_and(|name| name == local_name)
        });
    }
    body.split(',')
        .next()
        .is_some_and(|value| value.trim() == local_name)
}

fn resolve_relative_module(
    file_units: &HashMap<String, String>,
    source_path: &str,
    module: &str,
) -> Option<String> {
    let source_dir = source_path
        .rsplit_once('/')
        .map(|(dir, _)| dir)
        .unwrap_or_default();
    let mut segments = source_dir
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();
    for segment in module.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop()?;
            }
            value => segments.push(value),
        }
    }
    let base = segments.join("/");
    for candidate in [
        base.clone(),
        format!("{base}.ts"),
        format!("{base}.tsx"),
        format!("{base}.js"),
        format!("{base}.jsx"),
        format!("{base}/index.ts"),
        format!("{base}/index.tsx"),
        format!("{base}/index.js"),
        format!("{base}/index.jsx"),
    ] {
        if let Some(file_id) = file_units.get(&candidate) {
            return Some(file_id.clone());
        }
    }
    None
}

fn simple_name(value: &str) -> Option<&str> {
    let value = value.trim();
    (!value.is_empty()
        && value.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '_' || character == '$'
        }))
    .then_some(value)
}

fn route_method(callee: &str, methods: &[&str]) -> Option<String> {
    let method = call_method_name(callee);
    let method = method.strip_prefix("Map").unwrap_or(method);
    methods
        .iter()
        .find(|candidate| candidate.eq_ignore_ascii_case(method))
        .map(|method| method.to_ascii_uppercase())
}

fn call_method_name(callee: &str) -> &str {
    callee
        .rsplit_once('.')
        .map(|(_, method)| method)
        .or_else(|| callee.rsplit_once("::").map(|(_, method)| method))
        .or_else(|| callee.rsplit_once("->").map(|(_, method)| method))
        .unwrap_or(callee)
}

fn call_receiver(callee: &str) -> Option<&str> {
    callee
        .rsplit_once('.')
        .map(|(receiver, _)| receiver)
        .or_else(|| callee.rsplit_once("::").map(|(receiver, _)| receiver))
        .or_else(|| callee.rsplit_once("->").map(|(receiver, _)| receiver))
}

struct RouteUnitIndex {
    by_file_name: HashMap<(String, String), Vec<String>>,
    by_name: HashMap<String, Vec<String>>,
    aliases_by_file: HashMap<(String, String), Vec<String>>,
    file_by_source_unit: HashMap<String, String>,
}

impl RouteUnitIndex {
    fn new(facts: &FactStore) -> Self {
        let mut index = Self {
            by_file_name: HashMap::new(),
            by_name: HashMap::new(),
            aliases_by_file: HashMap::new(),
            file_by_source_unit: HashMap::new(),
        };
        for unit in facts.units.values() {
            index
                .file_by_source_unit
                .insert(unit.id.clone(), unit.file_id.clone());
            let name = unit.name.to_ascii_lowercase();
            index
                .by_file_name
                .entry((unit.file_id.clone(), name.clone()))
                .or_default()
                .push(unit.id.clone());
            index.by_name.entry(name).or_default().push(unit.id.clone());
        }
        for binding in &facts.bindings {
            let Some(file_id) = index.file_by_source_unit.get(&binding.source_unit_id) else {
                continue;
            };
            index
                .aliases_by_file
                .entry((file_id.clone(), binding.local_name.to_ascii_lowercase()))
                .or_default()
                .push(binding.target_name.clone());
        }
        for candidates in index.by_file_name.values_mut() {
            candidates.sort();
            candidates.dedup();
        }
        for candidates in index.by_name.values_mut() {
            candidates.sort();
            candidates.dedup();
        }
        index
    }

    fn resolve(&self, source_unit_id: &str, expression: Option<&str>) -> Option<String> {
        let expression = expression?.trim();
        if expression.is_empty() {
            return None;
        }
        let expression = string_literal(expression).unwrap_or_else(|| expression.to_string());
        let local_name = expression
            .rsplit_once('.')
            .map(|(_, value)| value)
            .or_else(|| expression.rsplit_once("::").map(|(_, value)| value))
            .or_else(|| expression.rsplit_once("->").map(|(_, value)| value))
            .unwrap_or(&expression)
            .trim()
            .to_ascii_lowercase();
        let file_id = self.file_by_source_unit.get(source_unit_id)?;
        if let Some(same_file) = self
            .by_file_name
            .get(&(file_id.clone(), local_name.clone()))
        {
            if same_file.len() == 1 {
                return same_file.first().cloned();
            }
        }
        let aliases = self
            .aliases_by_file
            .get(&(file_id.clone(), local_name))
            .into_iter()
            .flatten()
            .map(|target| {
                target
                    .rsplit_once('.')
                    .map(|(_, name)| name)
                    .or_else(|| target.rsplit_once("::").map(|(_, name)| name))
                    .unwrap_or(target)
                    .to_ascii_lowercase()
            })
            .collect::<HashSet<_>>();
        let mut candidates = aliases
            .iter()
            .filter_map(|name| self.by_name.get(name))
            .flatten()
            .cloned()
            .collect::<Vec<_>>();
        candidates.sort();
        candidates.dedup();
        (candidates.len() == 1).then(|| candidates.remove(0))
    }
}

fn compose_route_path(prefix: Option<&str>, path: Option<&str>) -> Option<String> {
    match (prefix, path) {
        (Some(prefix), Some(path)) => Some(join_paths(prefix, path)),
        (None, Some(path)) => Some(join_paths("", path)),
        (Some(prefix), None) => Some(join_paths(prefix, "")),
        (None, None) => None,
    }
}

fn alternate_route_literal(call: &crate::facts::CallSiteFact, path_index: usize) -> Option<String> {
    if path_index != 0 {
        return None;
    }
    call.arguments
        .get(1)
        .and_then(|argument| string_literal(argument))
}

fn endpoint_group_prefix(
    facts: &FactStore,
    source_unit_id: &str,
    framework_id: &str,
) -> Option<String> {
    if framework_id != "csharp.minimal_api" {
        return None;
    }
    let mut current = facts.unit(source_unit_id)?;
    while current.kind != CodeUnitKind::Class {
        let parent_id = current.parent_id.as_deref()?;
        current = facts.unit(parent_id)?;
    }
    if !current
        .relative_path
        .replace('\\', "/")
        .contains("/Endpoints/")
    {
        return None;
    }
    Some(format!("/api/{}", current.name))
}
