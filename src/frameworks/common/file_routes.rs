//! 파일 구조가 HTTP route를 정의하는 프레임워크의 정적 extractor.

use crate::facts::{CodeUnitKind, Entrypoint, EntrypointKind, Evidence, FactStore};
use crate::frameworks::registry::detector::FrameworkDetection;
use crate::languages::common::metadata::stable_id;
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// Next/Nuxt/SvelteKit처럼 파일 이름이 route가 되는 프레임워크를 추출한다.
pub fn add_file_routes(
    facts: &mut FactStore,
    detections: &[FrameworkDetection],
    framework_id: &str,
) {
    let Some(detection) = detections
        .iter()
        .find(|detection| detection.id == framework_id)
    else {
        return;
    };

    let files = facts
        .units
        .values()
        .filter(|unit| unit.kind == CodeUnitKind::File)
        .cloned()
        .collect::<Vec<_>>();
    let mut handlers_by_file: HashMap<String, Vec<(String, String)>> = HashMap::new();
    for unit in facts.units.values().filter(|unit| {
        matches!(
            unit.kind,
            CodeUnitKind::Function | CodeUnitKind::Method | CodeUnitKind::Constructor
        ) && http_method(&unit.name).is_some()
    }) {
        handlers_by_file
            .entry(unit.file_id.clone())
            .or_default()
            .push((http_method(&unit.name).unwrap_or_default(), unit.id.clone()));
    }
    let mut existing_ids = facts
        .entrypoints
        .iter()
        .map(|entrypoint| entrypoint.id.clone())
        .collect::<HashSet<_>>();
    for file in files {
        if !detection
            .languages
            .iter()
            .any(|language| language == file.language.key())
        {
            continue;
        }
        let Some(route) = file_route(framework_id, &file.relative_path) else {
            continue;
        };
        let handlers = handlers_by_file
            .get(&file.file_id)
            .cloned()
            .unwrap_or_default();

        if handlers.is_empty() {
            add_entrypoint(
                facts,
                &mut existing_ids,
                &file.id,
                "HTTP",
                &route,
                framework_id,
                &file.span,
            );
            continue;
        }
        for (method, unit_id) in handlers {
            add_entrypoint(
                facts,
                &mut existing_ids,
                &unit_id,
                &method,
                &route,
                framework_id,
                &file.span,
            );
        }
    }
}

fn add_entrypoint(
    facts: &mut FactStore,
    existing_ids: &mut HashSet<String>,
    unit_id: &str,
    method: &str,
    path: &str,
    framework_id: &str,
    span: &crate::facts::SourceSpan,
) {
    let id = stable_id(
        "entry",
        &format!("file-route:{unit_id}:{method}:{path}:{framework_id}"),
    );
    if !existing_ids.insert(id.clone()) {
        return;
    }
    facts.entrypoints.push(Entrypoint {
        id,
        unit_id: unit_id.to_string(),
        kind: EntrypointKind::Http,
        name: path.to_string(),
        method: Some(method.to_string()),
        path: Some(path.to_string()),
        framework_id: Some(framework_id.to_string()),
        evidence: vec![Evidence::new("fileRoute", path, span.clone())],
    });
}

fn http_method(name: &str) -> Option<String> {
    let upper = name.to_ascii_uppercase();
    if matches!(
        upper.as_str(),
        "GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "OPTIONS" | "HEAD"
    ) {
        return Some(upper);
    }
    match upper.as_str() {
        "ONREQUEST" => Some("HTTP".to_string()),
        "ONGET" => Some("GET".to_string()),
        "ONPOST" => Some("POST".to_string()),
        "ONPUT" => Some("PUT".to_string()),
        "ONPATCH" => Some("PATCH".to_string()),
        "ONDELETE" => Some("DELETE".to_string()),
        _ => None,
    }
}

fn file_route(framework_id: &str, relative_path: &str) -> Option<String> {
    let normalized = relative_path.replace('\\', "/");
    let route_segments = match framework_id {
        "javascript.nextjs" => next_route_segments(&normalized)?,
        "javascript.nuxt" => nuxt_route_segments(&normalized)?,
        "javascript.sveltekit" => sveltekit_route_segments(&normalized)?,
        "dart.dart_frog" => dart_frog_route_segments(&normalized)?,
        _ => return None,
    };
    let route = route_segments
        .into_iter()
        .filter(|segment| !segment.is_empty())
        .map(route_segment)
        .collect::<Vec<_>>()
        .join("/");
    Some(if route.is_empty() {
        "/".to_string()
    } else {
        format!("/{route}")
    })
}

fn next_route_segments(path: &str) -> Option<Vec<String>> {
    let path = Path::new(path);
    let segments = path
        .iter()
        .map(|segment| segment.to_string_lossy().to_string())
        .collect::<Vec<_>>();
    let file = segments.last()?.as_str();
    let marker = segments
        .iter()
        .position(|segment| segment == "app" || segment == "pages")?;

    if segments[marker] == "app" {
        let is_route_file = matches!(file, "route.ts" | "route.tsx" | "route.js" | "route.jsx");
        if !is_route_file {
            return None;
        }
        return Some(segments[marker + 1..segments.len() - 1].to_vec());
    }

    // Pages Router는 route.ts라는 고정 파일명이 아니라 pages 아래의
    // 모듈 파일 자체가 route다. 특히 pages/api/*.ts는 API 진입점이므로
    // App Router 규칙만 적용하면 정상적인 API가 조용히 누락된다.
    let is_page_module = [".js", ".jsx", ".ts", ".tsx"]
        .iter()
        .any(|suffix| file.ends_with(suffix));
    if !is_page_module || file.starts_with('_') {
        return None;
    }
    let mut route = segments[marker + 1..].to_vec();
    if let Some(last) = route.last_mut() {
        for suffix in [".tsx", ".jsx", ".ts", ".js"] {
            if let Some(stem) = last.strip_suffix(suffix) {
                *last = stem.to_string();
                break;
            }
        }
    }
    route.retain(|segment| segment != "index");
    Some(route)
}

fn nuxt_route_segments(path: &str) -> Option<Vec<String>> {
    let segments = path.split('/').map(str::to_string).collect::<Vec<_>>();
    let marker = segments.iter().position(|segment| segment == "server")?;
    let file = segments.last()?.as_str();
    if !file.ends_with(".ts") && !file.ends_with(".js") {
        return None;
    }
    if !segments
        .get(marker + 1)
        .is_some_and(|segment| segment == "api" || segment == "routes")
    {
        return None;
    }
    let mut route = segments[marker + 2..].to_vec();
    if let Some(last) = route.last_mut() {
        *last = last
            .trim_end_matches(".server.ts")
            .trim_end_matches(".server.js")
            .trim_end_matches(".ts")
            .trim_end_matches(".js")
            .to_string();
    }
    route.retain(|segment| segment != "index");
    Some(route)
}

fn sveltekit_route_segments(path: &str) -> Option<Vec<String>> {
    let segments = path.split('/').map(str::to_string).collect::<Vec<_>>();
    let marker = segments.iter().position(|segment| segment == "routes")?;
    let file = segments.last()?.as_str();
    if !matches!(
        file,
        "+server.ts" | "+server.js" | "+server.tsx" | "+server.jsx"
    ) {
        return None;
    }
    Some(segments[marker + 1..segments.len() - 1].to_vec())
}

fn dart_frog_route_segments(path: &str) -> Option<Vec<String>> {
    let segments = path.split('/').map(str::to_string).collect::<Vec<_>>();
    let marker = segments.iter().position(|segment| segment == "routes")?;
    let file = segments.last()?.as_str();
    if !file.ends_with(".dart") {
        return None;
    }
    let mut route = segments[marker + 1..].to_vec();
    if let Some(last) = route.last_mut() {
        *last = last.trim_end_matches(".dart").to_string();
    }
    route.retain(|segment| segment != "index");
    Some(route)
}

fn route_segment(segment: String) -> String {
    if segment.starts_with("[...") && segment.ends_with(']') {
        return format!("*{}", &segment[4..segment.len() - 1]);
    }
    if segment.starts_with('[') && segment.ends_with(']') {
        return format!(":{}", &segment[1..segment.len() - 1]);
    }
    segment
}
