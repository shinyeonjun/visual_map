use std::collections::{BTreeMap, HashMap, HashSet};
use std::env;
use std::fs;
use std::path::Path;
use std::time::Instant;

use crate::{DocumentOutput, LANGUAGES};
mod facts;
mod fastapi;
mod loader;
mod schema;
pub(crate) use facts::*;
pub(crate) use fastapi::*;
pub(crate) use loader::*;
pub(crate) use schema::*;

#[cfg(test)]
mod tests;

#[cfg(test)]
pub(crate) fn analyze(
    project_root: &Path,
    documents: &[DocumentOutput],
    pack_root: &Path,
) -> Result<Analysis, String> {
    let snapshot = crate::load_source_snapshot(project_root);
    analyze_with_sources(project_root, documents, pack_root, &snapshot)
}

pub(crate) fn analyze_with_sources(
    project_root: &Path,
    documents: &[DocumentOutput],
    pack_root: &Path,
    snapshot: &crate::SourceSnapshot,
) -> Result<Analysis, String> {
    let debug_timing = env::var_os("CODE_MEMORY_FRAMEWORK_TIMING").is_some();
    let load_started = Instant::now();
    let packs = load_packs(pack_root)?;
    if debug_timing {
        eprintln!(
            "framework timing=load_packs elapsed_ms={} packs={}",
            load_started.elapsed().as_millis(),
            packs.len()
        );
    }
    let project_sources = &snapshot.files;
    let mut source_indexes: HashMap<String, Vec<usize>> = HashMap::new();
    for (index, (path, _)) in project_sources.iter().enumerate() {
        if let Some(language) = LANGUAGES
            .iter()
            .find(|item| path_matches_language(path, item.extensions))
        {
            source_indexes
                .entry(language.id.to_string())
                .or_default()
                .push(index);
        }
    }
    let metadata_sources = collect_metadata_sources(project_root);
    let mut source_signal_cache = HashMap::<(String, String), bool>::new();
    let mut metadata_signal_cache = HashMap::<(String, String), bool>::new();
    let play_routes = fs::read_to_string(project_root.join("conf").join("routes"))
        .ok()
        .map(|text| (String::from("conf/routes"), text));
    let mut frameworks = Vec::new();
    let mut relations = Vec::new();
    let symbol_index = build_framework_symbol_index(documents);

    for pack in packs {
        let pack_started = debug_timing.then(Instant::now);
        if !LANGUAGES.iter().any(|item| item.id == pack.language) {
            continue;
        }
        let mut sources: Vec<(&str, &str)> = source_indexes
            .get(pack.language.as_str())
            .into_iter()
            .flatten()
            .filter_map(|index| project_sources.get(*index))
            .map(|(path, source)| (path.as_str(), source.as_str()))
            .collect();
        let mut matched_signals = HashSet::new();
        let mut matched_files = HashSet::new();
        for &(relative, text) in &sources {
            for signal in &pack.signals {
                if cached_source_signal_match(&mut source_signal_cache, relative, text, signal) {
                    matched_signals.insert(signal.clone());
                    matched_files.insert(relative.to_string());
                }
            }
        }
        for (relative, text) in &metadata_sources {
            if !metadata_matches_language(&relative, &pack.language) {
                continue;
            }
            for signal in &pack.signals {
                if cached_metadata_signal_match(&mut metadata_signal_cache, relative, text, signal)
                {
                    matched_signals.insert(signal.clone());
                    matched_files.insert(relative.clone());
                }
            }
        }
        if pack.id == "play" {
            if let Some((relative, text)) = play_routes.as_ref() {
                for signal in &pack.signals {
                    if cached_source_signal_match(&mut source_signal_cache, relative, text, signal)
                    {
                        matched_signals.insert(signal.clone());
                        matched_files.insert(relative.clone());
                    }
                }
                sources.push((relative.as_str(), text.as_str()));
            }
        }

        if let Some(started) = pack_started {
            eprintln!(
                "framework timing=signals id={} elapsed_ms={} matched={}",
                pack.id,
                started.elapsed().as_millis(),
                matched_signals.len()
            );
        }

        // ponytail: two independent source signals avoid activating a pack from
        // one incidental identifier; replace with dependency-aware manifests if
        // single-signal frameworks become necessary.
        if matched_signals.len() < 2 {
            continue;
        }

        // Signals identify the files that actually belong to the detected
        // framework. Scanning every source file for every detected pack makes
        // route packs degrade toward O(packs * all_source_lines). Keep
        // convention-based route files as candidates even when their imports
        // are elsewhere; all other files need direct signal evidence.
        let source_signal_files: HashSet<&str> = sources
            .iter()
            .map(|(path, _)| *path)
            .filter(|path| matched_files.contains(*path))
            .collect();
        let restrict_to_signal_files = !source_signal_files.is_empty();
        let has_route_rules = pack.rules.iter().any(|rule| rule == "HTTP_ROUTE");
        let candidate_sources: Vec<(&str, &str)> = sources
            .iter()
            .copied()
            .filter(|(path, source)| {
                if has_route_rules {
                    has_route_syntax_candidate(source)
                        || file_system_route(&pack, path, source).is_some()
                        || source_signal_files.contains(path)
                } else {
                    !restrict_to_signal_files || source_signal_files.contains(*path)
                }
            })
            .collect();

        let mut facts = Vec::new();
        let fastapi_context =
            (pack.id == "fastapi").then(|| build_fastapi_route_context(&candidate_sources));
        for &(path, source) in &candidate_sources {
            if pack.rules.iter().any(|value| value == "HTTP_ROUTE") {
                extract_routes_with_index(
                    &pack,
                    path,
                    source,
                    fastapi_context.as_ref(),
                    &symbol_index,
                    &mut facts,
                );
            }
            extract_generic_facts_with_index(&pack, path, source, &symbol_index, &mut facts);
        }
        for fact in &mut facts {
            if fact.symbol.is_none() {
                let resolution = if fact.properties.contains_key("target") {
                    "framework_alias"
                } else {
                    "unresolved"
                };
                fact.properties
                    .entry("resolution".to_string())
                    .or_insert_with(|| resolution.to_string());
            }
        }
        for fact in &facts {
            if let Some(handler) = &fact.symbol {
                if fact.kind == "HTTP_ROUTE" {
                    let Some(_path) = &fact.path else { continue };
                    let Some(_method) = &fact.method else {
                        continue;
                    };
                    relations.push(FrameworkRelation {
                        from: handler.clone(),
                        to: fact.id.clone(),
                        kind: "HANDLES".to_string(),
                        framework: pack.id.clone(),
                        path: fact.source_file.clone(),
                        range: fact.source_range.clone(),
                        evidence: fact.evidence.clone(),
                    });
                } else if fact.kind == "RPC_ENDPOINT"
                    && pack.outputs.iter().any(|value| value == "HANDLES")
                {
                    relations.push(FrameworkRelation {
                        from: handler.clone(),
                        to: fact.id.clone(),
                        kind: "HANDLES".to_string(),
                        framework: pack.id.clone(),
                        path: fact.source_file.clone(),
                        range: fact.source_range.clone(),
                        evidence: fact.evidence.clone(),
                    });
                } else if let Some(kind) = relation_kind_for_fact(&fact.kind) {
                    relations.push(FrameworkRelation {
                        from: handler.clone(),
                        to: fact.id.clone(),
                        kind: kind.to_string(),
                        framework: pack.id.clone(),
                        path: fact.source_file.clone(),
                        range: fact.source_range.clone(),
                        evidence: fact.evidence.clone(),
                    });
                }
            }
        }

        let mut matched_signals: Vec<String> = matched_signals.into_iter().collect();
        let mut matched_files: Vec<String> = matched_files.into_iter().collect();
        matched_signals.sort();
        matched_files.sort();
        frameworks.push(FrameworkOutput {
            id: pack.id.clone(),
            language: pack.language,
            name: pack.name,
            kind: pack.kind,
            adapter: pack.adapter,
            status: "detected".to_string(),
            matched_signals,
            files: matched_files,
            facts,
        });
        if let Some(started) = pack_started {
            eprintln!(
                "framework timing=pack id={} elapsed_ms={}",
                pack.id,
                started.elapsed().as_millis()
            );
        }
    }

    Ok(Analysis {
        frameworks,
        relations,
    })
}

fn has_route_syntax_candidate(source: &str) -> bool {
    [
        ".route(",
        ".get(",
        ".post(",
        ".put(",
        ".patch(",
        ".delete(",
        "get(",
        "post(",
        "put(",
        "patch(",
        "delete(",
        "::get(",
        "::post(",
        "::put(",
        "::patch(",
        "::delete(",
        ".Get(",
        ".Post(",
        ".Put(",
        ".Patch(",
        ".Delete(",
        "GET(",
        "POST(",
        "PUT(",
        "PATCH(",
        "DELETE(",
        "MapGet(",
        "MapPost(",
        "MapPut(",
        "MapPatch(",
        "MapDelete(",
        "HttpGet(",
        "HttpPost(",
        "HttpPut(",
        "HttpPatch(",
        "HttpDelete(",
        "@GetMapping",
        "@Get(",
        "@PostMapping",
        "@Post(",
        "@PutMapping",
        "@Put(",
        "@PatchMapping",
        "@Patch(",
        "@DeleteMapping",
        "@Delete(",
        "#[get(",
        "[Get(",
        "#[post(",
        "[Post(",
        "#[put(",
        "[Put(",
        "#[patch(",
        "[Patch(",
        "#[delete(",
        "[Delete(",
        ".add_url_rule(",
        ".add_api_route(",
        ".addRoute(",
        ".and_then(",
        "Route(",
        "Router(",
        "Route::new",
        ".at(",
        "path(",
        "CROW_ROUTE",
        "ADD_METHOD_TO",
        "@app.route",
        "@router.route",
        "@route(",
        "#[Route",
        "[Route(",
        "HandleFunc(",
        "scope(",
        "r.on ",
        "@page ",
        "GET ",
        "POST ",
        "PUT ",
        "PATCH ",
        "DELETE ",
        "get ",
        "post ",
        "put ",
        "patch ",
        "delete ",
        "map ",
    ]
    .iter()
    .any(|marker| source.contains(marker))
}

fn extract_routes(
    pack: &FrameworkPack,
    path: &str,
    source: &str,
    documents: &[DocumentOutput],
    fastapi_context: Option<&FastApiRouteContext>,
    facts: &mut Vec<FrameworkFact>,
) {
    let symbol_index = build_framework_symbol_index(documents);
    extract_routes_with_index(pack, path, source, fastapi_context, &symbol_index, facts);
}

fn extract_routes_with_index(
    pack: &FrameworkPack,
    path: &str,
    source: &str,
    fastapi_context: Option<&FastApiRouteContext>,
    symbol_index: &FrameworkSymbolIndex,
    facts: &mut Vec<FrameworkFact>,
) {
    let lines: Vec<&str> = source.lines().collect();
    let mut annotation_prefix: Option<String> = None;
    for (index, line) in lines.iter().enumerate() {
        if pack.id == "fastapi"
            && (line.contains("APIRouter(") || line.contains(".include_router("))
        {
            continue;
        }
        if let Some(prefix) = route_prefix(line) {
            if !has_http_method_annotation(line) {
                annotation_prefix = Some(prefix);
                continue;
            }
        }
        let mut route_line = (*line).to_string();
        while first_route_path(&route_line).is_none()
            && route_line.contains('(')
            && route_line.matches('(').count() > route_line.matches(')').count()
        {
            let next = index + route_line.lines().count();
            let Some(next_line) = lines.get(next) else {
                break;
            };
            route_line.push('\n');
            route_line.push_str(next_line);
        }
        let Some(method) = route_method(&route_line) else {
            continue;
        };
        let Some((route_path, end)) = first_route_path(&route_line) else {
            continue;
        };
        let route_path = if pack.id == "fastapi" {
            combine_route_prefix(
                fastapi_context.and_then(|context| context.prefix_for(path, line)),
                &route_path,
            )
        } else {
            combine_route_prefix(annotation_prefix.as_deref(), &route_path)
        };
        let handler_name = if pack.id == "fastapi" {
            fastapi_handler_name(&lines, index, &route_line[end..])
        } else {
            config_route_handler(&route_line)
                .or_else(|| macro_registration_handler(&route_line))
                .or_else(|| annotation_handler_name(&route_line))
                .or_else(|| {
                    let rest = &route_line[end..];
                    (!rest.contains("def ")
                        && !rest.contains("function ")
                        && !rest.contains("fn ")
                        && !rest.contains("func "))
                    .then(|| registration_handler(rest))
                    .flatten()
                })
                .or_else(|| nearby_handler(&lines, index))
        };
        let handler = handler_name
            .as_deref()
            .and_then(|name| resolve_symbol_at_indexed(symbol_index, path, name, index))
            .or_else(|| {
                handler_name
                    .as_deref()
                    .and_then(|name| resolve_symbol_indexed(symbol_index, path, name))
            })
            .filter(|symbol| project_symbol_is_defined_indexed(symbol_index, symbol));
        let source_line = index + 1;
        let id = format!("route:{}:{}:{}:{}", pack.id, path, source_line, route_path);
        facts.push(FrameworkFact {
            id,
            kind: "HTTP_ROUTE".to_string(),
            framework: pack.id.clone(),
            symbol: handler,
            method: Some(method.to_string()),
            path: Some(route_path),
            source_file: path.to_string(),
            source_line,
            source_end_line: source_line,
            source_range: line_source_range(index, line),
            evidence: vec!["http_route_syntax".to_string()],
            properties: BTreeMap::new(),
        });
    }
    if let Some((route_path, method, handler_name, source_line)) =
        file_system_route(pack, path, source)
    {
        let handler = handler_name.and_then(|name| {
            resolve_symbol_at_indexed(symbol_index, path, &name, source_line.saturating_sub(1))
        });
        let id = format!("route:{}:{}:{}:{}", pack.id, path, source_line, route_path);
        if !facts.iter().any(|fact| fact.id == id) {
            facts.push(FrameworkFact {
                id,
                kind: "HTTP_ROUTE".to_string(),
                framework: pack.id.clone(),
                symbol: handler,
                method: Some(method),
                path: Some(route_path),
                source_file: path.to_string(),
                source_line,
                source_end_line: source_line,
                source_range: source_line_range(source, source_line),
                evidence: vec!["filesystem_route_convention".to_string()],
                properties: BTreeMap::new(),
            });
        }
    }
}

fn fastapi_handler_name(lines: &[&str], index: usize, trailing: &str) -> Option<String> {
    identifier_after(trailing, "def ").or_else(|| {
        lines
            .iter()
            .skip(index + 1)
            .take(12)
            .find_map(|line| identifier_after(line.trim_start(), "def "))
    })
}

fn line_source_range(line_number: usize, line: &str) -> Vec<i32> {
    vec![
        line_number as i32,
        0,
        line_number as i32,
        line.chars().count() as i32,
    ]
}

fn source_line_range(source: &str, source_line: usize) -> Vec<i32> {
    source
        .lines()
        .nth(source_line.saturating_sub(1))
        .map(|line| line_source_range(source_line.saturating_sub(1), line))
        .unwrap_or_else(|| line_source_range(source_line.saturating_sub(1), ""))
}

fn file_system_route(
    pack: &FrameworkPack,
    path: &str,
    source: &str,
) -> Option<(String, String, Option<String>, usize)> {
    let normalized = path.replace('\\', "/");
    let relative = match pack.id.as_str() {
        "nextjs" if normalized.contains("/pages/") || normalized.starts_with("pages/") => {
            after_directory(&normalized, "pages")?
        }
        "nextjs" if normalized.contains("/app/") || normalized.starts_with("app/") => {
            after_directory(&normalized, "app")?
        }
        "nuxt" if normalized.contains("/server/api/") || normalized.starts_with("server/api/") => {
            after_directory(&normalized, "server/api")?
        }
        "nuxt" if normalized.contains("/pages/") || normalized.starts_with("pages/") => {
            after_directory(&normalized, "pages")?
        }
        "sveltekit"
            if normalized.contains("/src/routes/") || normalized.starts_with("src/routes/") =>
        {
            after_directory(&normalized, "src/routes")?
        }
        "dart-frog" if normalized.contains("/routes/") || normalized.starts_with("routes/") => {
            after_directory(&normalized, "routes")?
        }
        _ => return None,
    };
    let mut segments = relative.split('/').collect::<Vec<_>>();
    let file = segments.pop()?.split('.').next()?;
    if matches!(file, "index" | "page" | "+page" | "+server" | "route") {
        // directory path already represents the route
    } else {
        segments.push(file);
    }
    let route = segments
        .into_iter()
        .filter(|segment| !segment.is_empty())
        .map(|segment| {
            if segment.starts_with("[...") && segment.ends_with(']') {
                format!("*{}", &segment[4..segment.len() - 1])
            } else if segment.starts_with('[') && segment.ends_with(']') {
                format!(":{}", &segment[1..segment.len() - 1])
            } else {
                segment.to_string()
            }
        })
        .collect::<Vec<_>>();
    let route_path = if route.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", route.join("/"))
    };
    let method = ["GET", "POST", "PUT", "PATCH", "DELETE"]
        .iter()
        .find(|method| source.contains(&format!("function {}", method)))
        .copied()
        .unwrap_or("ANY")
        .to_string();
    let handler = if source.contains("onRequest") {
        Some("onRequest".to_string())
    } else if source.contains("defineEventHandler(handler") {
        Some("handler".to_string())
    } else if source.contains("defineEventHandler") {
        Some("default".to_string())
    } else if source.contains("export default function ") {
        source
            .lines()
            .find_map(|line| identifier_after(line, "export default function "))
            .or_else(|| Some("default".to_string()))
    } else if pack.id == "sveltekit" {
        source
            .lines()
            .find_map(|line| assignment_target_before(line, "defineComponent("))
            .or_else(|| {
                source
                    .lines()
                    .find(|line| line.contains("export function load"))
                    .map(|_| "load".to_string())
            })
            .or_else(|| {
                ["GET", "POST", "PUT", "PATCH", "DELETE"]
                    .iter()
                    .find(|method| source.contains(&format!("function {method}")))
                    .map(|method| (*method).to_string())
            })
    } else {
        ["GET", "POST", "PUT", "PATCH", "DELETE"]
            .iter()
            .find(|method| source.contains(&format!("function {}", method)))
            .map(|method| (*method).to_string())
    };
    let source_line = source
        .lines()
        .position(|line| {
            line.contains("function GET")
                || line.contains("export default")
                || line.contains("onRequest")
        })
        .map(|line| line + 1)
        .unwrap_or(1);
    Some((route_path, method, handler, source_line))
}

fn after_directory<'a>(path: &'a str, directory: &str) -> Option<&'a str> {
    let prefix = format!("{directory}/");
    if let Some(relative) = path.strip_prefix(&prefix) {
        return Some(relative);
    }
    let marker = format!("/{directory}/");
    path.split_once(&marker).map(|(_, relative)| relative)
}

fn extract_generic_facts(
    pack: &FrameworkPack,
    path: &str,
    source: &str,
    documents: &[DocumentOutput],
    facts: &mut Vec<FrameworkFact>,
) {
    let symbol_index = build_framework_symbol_index(documents);
    extract_generic_facts_with_index(pack, path, source, &symbol_index, facts);
}

fn extract_generic_facts_with_index(
    pack: &FrameworkPack,
    path: &str,
    source: &str,
    symbol_index: &FrameworkSymbolIndex,
    facts: &mut Vec<FrameworkFact>,
) {
    let lines: Vec<&str> = source.lines().collect();
    for (index, line) in lines.iter().enumerate() {
        for output in &pack.rules {
            if matches!(output.as_str(), "HTTP_ROUTE" | "HANDLES") {
                continue;
            }
            let Some(evidence) = output_evidence(pack, output, line) else {
                continue;
            };
            let handler_name =
                fact_target_name(output, line).or_else(|| nearby_handler(&lines, index));
            let symbol = handler_name
                .and_then(|name| resolve_symbol_at_indexed(symbol_index, path, &name, index))
                .filter(|symbol| project_symbol_is_defined_indexed(symbol_index, symbol));
            facts.push(FrameworkFact {
                id: format!("fact:{}:{}:{}:{}", pack.id, output, path, index + 1),
                kind: output.clone(),
                framework: pack.id.clone(),
                symbol,
                method: None,
                path: None,
                source_file: path.to_string(),
                source_line: index + 1,
                source_end_line: index + 1,
                source_range: line_source_range(index, line),
                evidence: vec![evidence],
                properties: fact_properties(output, line),
            });
        }
    }
}

pub(crate) fn implementation_file_score(symbol: &str) -> u8 {
    let location = symbol.split('@').next().unwrap_or(symbol);
    let location = location.rsplit(['#', '/', '.']).next().unwrap_or(location);
    let extension = symbol
        .split('#')
        .next()
        .and_then(|value| value.rsplit_once('.'))
        .map(|(_, extension)| extension.to_ascii_lowercase());
    match extension.as_deref() {
        Some("c") | Some("cc") | Some("cpp") | Some("cxx") | Some("m") | Some("mm") => 2,
        Some("h") | Some("hh") | Some("hpp") | Some("hxx") => 1,
        _ if !location.is_empty() => 0,
        _ => 0,
    }
}

pub(crate) fn symbol_short_name(symbol: &str) -> &str {
    let symbol = symbol.split('@').next().unwrap_or(symbol);
    let symbol = symbol.trim_end_matches(['.', ':', '/']);
    let symbol = symbol.trim_end_matches('#');
    let symbol = symbol.rsplit(['#', '.', ':', '/']).next().unwrap_or(symbol);
    let symbol = symbol.split_whitespace().last().unwrap_or(symbol);
    symbol.split('(').next().unwrap_or(symbol)
}

#[cfg(test)]
pub(crate) fn symbol_matches_name(symbol: &str, name: &str) -> bool {
    let name = name
        .rsplit(['#', '.', ':', '/', '\\'])
        .next()
        .unwrap_or(name)
        .trim_matches(['"', '\'', '`']);
    symbol_short_name(symbol) == name
}

pub(crate) fn self_test(root: &Path) -> Result<usize, String> {
    let packs = load_packs(root)?;
    let mut passed = 0usize;
    for pack in &packs {
        if pack
            .fixture
            .expected_relations
            .iter()
            .any(|relation| !pack.outputs.iter().any(|output| output == relation))
        {
            return Err(format!("{} has an invalid fixture relation", pack.id));
        }
        let mut facts = Vec::new();
        for file in &pack.fixture.files {
            if !fixture_is_source_file(pack, &file.path) {
                continue;
            }
            if pack.rules.iter().any(|rule| rule == "HTTP_ROUTE") {
                extract_routes(pack, &file.path, &file.source, &[], None, &mut facts);
            }
            extract_generic_facts(pack, &file.path, &file.source, &[], &mut facts);
        }
        for rule in &pack.fixture.expected_facts {
            if !facts.iter().any(|fact| fact.kind == *rule) {
                return Err(format!("{} did not emit {}", pack.id, rule));
            }
        }
        if facts
            .iter()
            .map(|fact| &fact.id)
            .collect::<HashSet<_>>()
            .len()
            != facts.len()
        {
            return Err(format!("{} emitted duplicate fact IDs", pack.id));
        }
        passed += 1;
    }
    println!("framework-pack-self-test\t{passed}");
    Ok(passed)
}

fn fixture_is_source_file(pack: &FrameworkPack, path: &str) -> bool {
    LANGUAGES
        .iter()
        .find(|language| language.id == pack.language)
        .map(|language| {
            language
                .extensions
                .iter()
                .any(|extension| path.ends_with(extension))
        })
        .unwrap_or(false)
}
