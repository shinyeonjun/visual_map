use std::collections::{BTreeMap, HashMap, HashSet};
use std::env;
use std::fs;
use std::path::Path;
use std::time::Instant;

use crate::{DocumentOutput, LANGUAGES};
mod facts;
mod fastapi;
mod javascript_routes;
mod loader;
mod schema;
pub(crate) use facts::*;
pub(crate) use fastapi::*;
pub(crate) use javascript_routes::*;
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
    let java_webflux_modules = java_modules_with_markers(
        project_sources,
        &metadata_sources,
        &[
            "spring-boot-starter-webflux",
            "spring-cloud-starter-gateway-server-webflux",
            "org.springframework.web.reactive",
            "reactor.core.publisher",
            "RouterFunction",
        ],
    );
    let java_mvc_modules = java_modules_with_markers(
        project_sources,
        &metadata_sources,
        &["spring-boot-starter-webmvc", "spring-boot-starter-web"],
    );
    let java_quarkus_modules = java_modules_with_markers(
        project_sources,
        &metadata_sources,
        &["io.quarkus", "quarkus-"],
    );
    let mut source_signal_cache = HashMap::<(String, String), bool>::new();
    let mut metadata_signal_cache = HashMap::<(String, String), bool>::new();
    let mut source_code_masks = HashMap::<String, String>::new();
    let mut source_comment_masks = HashMap::<String, String>::new();
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
            let code = source_code_masks
                .entry(relative.to_string())
                .or_insert_with(|| source_code_mask(text, &pack.language));
            let comments = source_comment_masks
                .entry(relative.to_string())
                .or_insert_with(|| source_without_comments(text, &pack.language));
            for signal in &pack.signals {
                let signal_source = if signal_uses_string_literal(signal) {
                    comments.as_str()
                } else {
                    code.as_str()
                };
                if cached_source_signal_match(
                    &mut source_signal_cache,
                    relative,
                    signal_source,
                    signal,
                ) {
                    matched_signals.insert(signal.clone());
                    matched_files.insert(relative.to_string());
                }
            }
        }
        for (relative, text) in &metadata_sources {
            if !metadata_matches_language(relative, &pack.language) {
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
                let code = source_code_masks
                    .entry(relative.to_string())
                    .or_insert_with(|| source_code_mask(text, &pack.language));
                let comments = source_comment_masks
                    .entry(relative.to_string())
                    .or_insert_with(|| source_without_comments(text, &pack.language));
                for signal in &pack.signals {
                    let signal_source = if signal_uses_string_literal(signal) {
                        comments.as_str()
                    } else {
                        code.as_str()
                    };
                    if cached_source_signal_match(
                        &mut source_signal_cache,
                        relative,
                        signal_source,
                        signal,
                    ) {
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

        // Two behavioral signals can be shared by competing frameworks (for
        // example Express and Koa both use app/router methods). When a pack
        // declares an import/package identity, require that evidence as well.
        if matched_signals.len() < 2 || !framework_identity_confirmed(&pack, &matched_signals) {
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
                    has_route_syntax_candidate(source, &pack.language)
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
        let javascript_route_context =
            (pack.id == "express").then(|| JavascriptRouteContext::build(&sources));
        for &(path, source) in &candidate_sources {
            let source_owns_registration_routes = !matches!(
                (pack.language.as_str(), pack.adapter.as_str()),
                ("javascript" | "typescript", "registration-routing")
            ) || matched_files.contains(path);
            if pack.rules.iter().any(|value| value == "HTTP_ROUTE")
                && source_owns_registration_routes
                && pack_owns_routes(
                    &pack,
                    path,
                    source,
                    &java_webflux_modules,
                    &java_mvc_modules,
                    &java_quarkus_modules,
                )
            {
                let first_route_fact = facts.len();
                extract_routes_with_index(
                    &pack,
                    path,
                    source,
                    fastapi_context.as_ref(),
                    &symbol_index,
                    &mut facts,
                );
                if let Some(context) = javascript_route_context.as_ref() {
                    for fact in &mut facts[first_route_fact..] {
                        let Some(local_path) = fact.path.clone() else {
                            continue;
                        };
                        let Some(mounted_path) = context.mounted_path(path, &local_path) else {
                            continue;
                        };
                        if mounted_path == local_path {
                            continue;
                        }
                        fact.properties
                            .insert("localRoutePath".to_string(), local_path);
                        fact.properties
                            .insert("mountedRoutePath".to_string(), mounted_path.clone());
                        fact.properties.insert(
                            "routePathSource".to_string(),
                            "javascript-static-mount".to_string(),
                        );
                        fact.path = Some(mounted_path.clone());
                        fact.id = format!(
                            "route:{}:{}:{}:{}:{}",
                            pack.id,
                            path,
                            fact.source_line,
                            fact.method.as_deref().unwrap_or("ANY"),
                            mounted_path
                        );
                    }
                }
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

    dedupe_java_facts(&mut frameworks, &mut relations);

    Ok(Analysis {
        frameworks,
        relations,
    })
}

fn dedupe_java_facts(frameworks: &mut [FrameworkOutput], relations: &mut Vec<FrameworkRelation>) {
    let java_framework_ids = frameworks
        .iter()
        .filter(|framework| framework.language == "java")
        .map(|framework| framework.id.clone())
        .collect::<HashSet<_>>();
    let mut seen_facts = HashSet::<(String, String, usize, String)>::new();
    let mut kept_fact_ids = HashSet::<String>::new();

    for framework in frameworks {
        if framework.language != "java" {
            continue;
        }
        framework.facts.retain(|fact| {
            let target = if fact.kind == "HTTP_ROUTE" {
                format!(
                    "{}:{}",
                    fact.method.clone().unwrap_or_default(),
                    fact.path.clone().unwrap_or_default()
                )
            } else {
                fact.properties
                    .get("target")
                    .cloned()
                    .or_else(|| fact.path.clone())
                    .unwrap_or_default()
            };
            let source_line = if matches!(fact.kind.as_str(), "SERVICE" | "COMPONENT") {
                0
            } else {
                fact.source_line
            };
            let key = (
                fact.kind.clone(),
                fact.source_file.clone(),
                source_line,
                target,
            );
            if seen_facts.insert(key) {
                kept_fact_ids.insert(fact.id.clone());
                true
            } else {
                false
            }
        });
    }

    let mut seen_handles = HashSet::<(String, String, String, String, Vec<i32>)>::new();
    relations.retain(|relation| {
        if !java_framework_ids.contains(&relation.framework) {
            return true;
        }
        kept_fact_ids.contains(&relation.to)
            && seen_handles.insert((
                relation.from.clone(),
                relation.to.clone(),
                relation.kind.clone(),
                relation.path.clone(),
                relation.range.clone(),
            ))
    });
}

fn java_modules_with_markers(
    sources: &[(String, String)],
    metadata: &[(String, String)],
    markers: &[&str],
) -> HashSet<String> {
    sources
        .iter()
        .chain(metadata.iter())
        .filter(|(_, source)| markers.iter().any(|marker| source.contains(marker)))
        .map(|(path, _)| java_module_root(path))
        .collect()
}

fn java_module_root(path: &str) -> String {
    path.strip_prefix("src/")
        .map(|_| String::new())
        .or_else(|| path.split_once("/src/").map(|(root, _)| root.to_string()))
        .or_else(|| path.rsplit_once('/').map(|(root, _)| root.to_string()))
        .unwrap_or_default()
}

fn pack_owns_routes(
    pack: &FrameworkPack,
    path: &str,
    source: &str,
    webflux_modules: &HashSet<String>,
    mvc_modules: &HashSet<String>,
    quarkus_modules: &HashSet<String>,
) -> bool {
    if pack.language == "csharp" {
        let legacy_web_api = source.contains("System.Web.Http")
            || source.contains("IHttpActionResult")
            || source.contains("HttpResponseMessage");
        let attribute_route = [
            "[HttpGet",
            "[HttpPost",
            "[HttpPut",
            "[HttpPatch",
            "[HttpDelete",
            "[HttpHead",
            "[HttpOptions",
        ]
        .iter()
        .any(|marker| source.contains(marker));
        return match pack.id.as_str() {
            // ASP.NET Core is the shared host/component model. Attribute MVC
            // and Minimal API are the concrete route owners.
            "aspnet-core" => false,
            "aspnet-mvc" => !legacy_web_api && attribute_route,
            "aspnet-web-api" => legacy_web_api && attribute_route,
            "minimal-api" => ["MapGet(", "MapPost(", "MapPut(", "MapPatch(", "MapDelete("]
                .iter()
                .any(|marker| source.contains(marker)),
            _ => true,
        };
    }
    if pack.language != "java" {
        return true;
    }
    let module = java_module_root(path);
    let source_is_reactive = [
        "org.springframework.web.reactive",
        "reactor.core.publisher",
        "RouterFunction",
        "ServerResponse",
    ]
    .iter()
    .any(|marker| source.contains(marker));
    let module_is_webflux = webflux_modules.contains(&module);
    let module_is_mvc = mvc_modules.contains(&module);
    let module_is_quarkus = quarkus_modules.contains(&module);
    let use_webflux = source_is_reactive || module_is_webflux && !module_is_mvc;
    let source_is_jax_rs = source.contains("jakarta.ws.rs")
        || source.contains("@Path(")
            && [
                "@GET", "@POST", "@PUT", "@PATCH", "@DELETE", "@HEAD", "@OPTIONS",
            ]
            .iter()
            .any(|annotation| source.contains(annotation));

    match pack.id.as_str() {
        // Spring and Spring Boot describe the component model. The concrete
        // web stack owns route facts and their provenance.
        "spring" | "spring-boot" => false,
        "spring-webflux" => use_webflux,
        "spring-mvc" => !use_webflux,
        "quarkus" => module_is_quarkus && source_is_jax_rs,
        "jakarta-ee" => !module_is_quarkus && source_is_jax_rs,
        _ => true,
    }
}

fn has_route_syntax_candidate(source: &str, language: &str) -> bool {
    let source = source_code_mask(source, language);
    [
        ".route(",
        ".get(",
        ".post(",
        ".put(",
        ".patch(",
        ".delete(",
        ".head(",
        ".options(",
        "get(",
        "post(",
        "put(",
        "patch(",
        "delete(",
        "head(",
        "options(",
        "::get(",
        "::post(",
        "::put(",
        "::patch(",
        "::delete(",
        "::head(",
        "::options(",
        ".Get(",
        ".Post(",
        ".Put(",
        ".Patch(",
        ".Delete(",
        ".Head(",
        ".Options(",
        "GET(",
        "POST(",
        "PUT(",
        "PATCH(",
        "DELETE(",
        "HEAD(",
        "OPTIONS(",
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
        "HttpHead(",
        "HttpOptions(",
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
        "@Head(",
        "@Options(",
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
        "#[head(",
        "[Head(",
        "#[options(",
        "[Options(",
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
        "HEAD ",
        "OPTIONS ",
        "get ",
        "post ",
        "put ",
        "patch ",
        "delete ",
        "head ",
        "options ",
        "map ",
    ]
    .iter()
    .any(|marker| source.contains(marker))
}

fn source_code_mask(source: &str, language: &str) -> String {
    source_mask(source, language, true)
}

fn source_without_comments(source: &str, language: &str) -> String {
    source_mask(source, language, false)
}

fn signal_uses_string_literal(signal: &str) -> bool {
    signal
        .split_once(':')
        .is_some_and(|(prefix, _)| matches!(prefix, "import" | "require" | "include" | "package"))
}

fn source_mask(source: &str, language: &str, mask_strings: bool) -> String {
    let chars = source.chars().collect::<Vec<_>>();
    let mut masked = String::with_capacity(source.len());
    let mut index = 0;
    let mut block_comment = false;
    let mut html_comment = false;
    let mut quote: Option<(char, bool)> = None;
    let hash_comments = matches!(language, "python" | "ruby" | "php");

    while index < chars.len() {
        let current = chars[index];
        if block_comment {
            if current == '*' && chars.get(index + 1) == Some(&'/') {
                masked.push(' ');
                masked.push(' ');
                index += 2;
                block_comment = false;
            } else {
                masked.push(if matches!(current, '\r' | '\n') {
                    current
                } else {
                    ' '
                });
                index += 1;
            }
            continue;
        }
        if html_comment {
            if chars.get(index..index + 3) == Some(&['-', '-', '>']) {
                masked.push_str("   ");
                index += 3;
                html_comment = false;
            } else {
                masked.push(if matches!(current, '\r' | '\n') {
                    current
                } else {
                    ' '
                });
                index += 1;
            }
            continue;
        }
        if let Some((delimiter, triple)) = quote {
            if triple && chars.get(index..index + 3) == Some(&[delimiter, delimiter, delimiter]) {
                if mask_strings {
                    masked.push_str("   ");
                } else {
                    masked.extend([delimiter, delimiter, delimiter]);
                }
                index += 3;
                quote = None;
            } else if !triple && current == '\\' && index + 1 < chars.len() {
                if mask_strings {
                    masked.push(' ');
                    masked.push(if matches!(chars[index + 1], '\r' | '\n') {
                        chars[index + 1]
                    } else {
                        ' '
                    });
                } else {
                    masked.push(current);
                    masked.push(chars[index + 1]);
                }
                index += 2;
            } else if !triple && current == delimiter {
                masked.push(if mask_strings { ' ' } else { current });
                index += 1;
                quote = None;
            } else if !triple && matches!(current, '\r' | '\n') && delimiter != '`' {
                masked.push(current);
                index += 1;
                quote = None;
            } else {
                masked.push(if !mask_strings || matches!(current, '\r' | '\n') {
                    current
                } else {
                    ' '
                });
                index += 1;
            }
            continue;
        }

        if chars.get(index..index + 4) == Some(&['<', '!', '-', '-']) {
            masked.push_str("    ");
            index += 4;
            html_comment = true;
        } else if current == '/' && chars.get(index + 1) == Some(&'*') {
            masked.push_str("  ");
            index += 2;
            block_comment = true;
        } else if current == '/' && chars.get(index + 1) == Some(&'/') {
            while index < chars.len() && !matches!(chars[index], '\r' | '\n') {
                masked.push(' ');
                index += 1;
            }
        } else if hash_comments
            && current == '#'
            && !(language == "php" && chars.get(index + 1) == Some(&'['))
        {
            while index < chars.len() && !matches!(chars[index], '\r' | '\n') {
                masked.push(' ');
                index += 1;
            }
        } else if matches!(current, '"' | '`') || (current == '\'' && language != "rust") {
            let triple =
                chars.get(index + 1) == Some(&current) && chars.get(index + 2) == Some(&current);
            if triple {
                if mask_strings {
                    masked.push_str("   ");
                } else {
                    masked.extend([current, current, current]);
                }
                index += 3;
            } else {
                masked.push(if mask_strings { ' ' } else { current });
                index += 1;
            }
            quote = Some((current, triple));
        } else {
            masked.push(current);
            index += 1;
        }
    }
    masked
}

fn declares_type(line: &str) -> bool {
    let tokens = line
        .split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    tokens.windows(2).any(|tokens| {
        matches!(tokens[0], "class" | "interface" | "record" | "struct")
            && tokens[1]
                .chars()
                .next()
                .is_some_and(|character| character.is_ascii_alphabetic() || character == '_')
    })
}

fn has_route_prefix_syntax(line: &str) -> bool {
    line.contains("@RequestMapping")
        || line.contains("@Controller(")
        || line.contains("@Path(")
        || (line.trim_start().starts_with("[Route(") && !line.contains("#[Route("))
        || line.trim_start().starts_with("[RoutePrefix(")
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
    let code = source_code_mask(source, &pack.language);
    let code_lines: Vec<&str> = code.lines().collect();
    let mut annotation_prefix: Option<String> = None;
    let mut annotation_type: Option<String> = None;
    let mut pending_prefix: Option<String> = None;
    let mut skip_until = 0usize;
    for (index, line) in lines.iter().enumerate() {
        if index < skip_until {
            continue;
        }
        let code_line = code_lines.get(index).copied().unwrap_or_default();
        if pack.id == "fastapi"
            && (code_line.contains("APIRouter(") || code_line.contains(".include_router("))
        {
            continue;
        }
        if declares_type(code_line) {
            annotation_prefix = pending_prefix.take();
            annotation_type = declared_type_name(code_line);
        }
        if has_route_prefix_syntax(code_line) {
            let Some(prefix) = route_prefix(line) else {
                continue;
            };
            let request_mapping_has_method = code_line.contains("@RequestMapping")
                && request_mapping_method(code_line).is_some();
            if !has_http_method_annotation(code_line) && !request_mapping_has_method {
                pending_prefix = Some(prefix);
                continue;
            }
        }
        if matches!(
            (pack.language.as_str(), pack.adapter.as_str()),
            ("javascript" | "typescript", "registration-routing")
        ) && code_line.contains(".route(")
        {
            let mut chain = (*line).to_string();
            for next_line in lines.iter().skip(index + 1).take(31) {
                if chain.contains(';') {
                    break;
                }
                chain.push('\n');
                chain.push_str(next_line);
            }
            let chain_calls = javascript_chained_route_calls(&chain);
            if !chain_calls.is_empty() {
                let consumed = chain.lines().count();
                skip_until = index + consumed;
                let Some((route_path, _)) = first_route_path(&chain) else {
                    continue;
                };
                for (method, handler_name, line_offset) in chain_calls {
                    let source_line = index + line_offset + 1;
                    let handler = handler_name
                        .as_deref()
                        .and_then(|name| {
                            resolve_symbol_in_file_indexed(
                                symbol_index,
                                path,
                                name,
                                source_line.saturating_sub(1),
                            )
                        })
                        .and_then(|symbol| {
                            project_definition_for_symbol_indexed(symbol_index, &symbol)
                        });
                    let route_path = combine_route_prefix(None, &route_path);
                    facts.push(FrameworkFact {
                        id: format!(
                            "route:{}:{}:{}:{}:{}",
                            pack.id, path, source_line, method, route_path
                        ),
                        kind: "HTTP_ROUTE".to_string(),
                        framework: pack.id.clone(),
                        symbol: handler,
                        method: Some(method),
                        path: Some(route_path),
                        source_file: path.to_string(),
                        source_line,
                        source_end_line: source_line,
                        source_range: source_line_range(source, source_line),
                        evidence: vec!["http_route_chain_syntax".to_string()],
                        properties: BTreeMap::new(),
                    });
                }
                pending_prefix = None;
                continue;
            }
        }
        let mut route_line = (*line).to_string();
        let mut route_code = code_line.to_string();
        if joins_following_route_annotations(pack, code_line) {
            for (next, next_line) in lines
                .iter()
                .enumerate()
                .take((index + 8).min(lines.len()))
                .skip(index + 1)
            {
                let next_code = code_lines.get(next).copied().unwrap_or_default();
                let trimmed = next_code.trim_start();
                if !(trimmed.starts_with('@') || trimmed.starts_with('[')) {
                    break;
                }
                route_line.push('\n');
                route_line.push_str(next_line);
                route_code.push('\n');
                route_code.push_str(next_code);
            }
        }
        while route_code.contains('(')
            && route_code.matches('(').count() > route_code.matches(')').count()
            && route_line.lines().count() < 64
        {
            let next = index + route_line.lines().count();
            let Some(next_line) = lines.get(next) else {
                break;
            };
            let next_code_line = code_lines.get(next).copied().unwrap_or_default();
            route_line.push('\n');
            route_line.push_str(next_line);
            route_code.push('\n');
            route_code.push_str(next_code_line);
        }
        skip_until = index + route_line.lines().count();
        let Some(method) =
            configured_route_method(&route_line).or_else(|| route_method(&route_code))
        else {
            continue;
        };
        let Some((route_paths, end)) = framework_route_paths(pack, &route_line, &route_code) else {
            continue;
        };
        let functional_lambda = if pack.language != "java" {
            false
        } else if route_code.contains("->") || route_code.contains("=>") {
            true
        } else {
            let mut found = false;
            for line in code_lines.iter().skip(index).take(8) {
                if line.contains("->") || line.contains("=>") {
                    found = true;
                    break;
                }
                let trimmed = line.trim_start();
                if trimmed.contains("public ")
                    || trimmed.contains("protected ")
                    || trimmed.contains("private ")
                {
                    break;
                }
            }
            found
        };
        let handler_name = if functional_lambda {
            (pack.language == "java")
                .then(|| enclosing_java_method(&lines, index))
                .flatten()
        } else if pack.id == "fastapi" {
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
            .and_then(|name| {
                if matches!(
                    (pack.language.as_str(), pack.adapter.as_str()),
                    ("javascript" | "typescript", "registration-routing")
                ) {
                    resolve_symbol_in_file_indexed(symbol_index, path, name, index)
                } else {
                    resolve_symbol_at_indexed(symbol_index, path, name, index)
                        .or_else(|| resolve_symbol_indexed(symbol_index, path, name))
                }
            })
            .and_then(|symbol| project_definition_for_symbol_indexed(symbol_index, &symbol));
        let source_line = index + 1;
        for route_path in route_paths {
            let route_path = if pack.id == "fastapi" {
                combine_route_prefix(
                    fastapi_context.and_then(|context| context.prefix_for(path, line)),
                    &route_path,
                )
            } else {
                combine_route_prefix(
                    pending_prefix.as_deref().or(annotation_prefix.as_deref()),
                    &route_path,
                )
            };
            let route_path = if pack.language == "csharp" {
                csharp_route_tokens(
                    &route_path,
                    annotation_type.as_deref(),
                    handler_name.as_deref(),
                )
            } else {
                route_path
            };
            let id = format!("route:{}:{}:{}:{}", pack.id, path, source_line, route_path);
            facts.push(FrameworkFact {
                id,
                kind: "HTTP_ROUTE".to_string(),
                framework: pack.id.clone(),
                symbol: handler.clone(),
                method: Some(method.to_string()),
                path: Some(route_path),
                source_file: path.to_string(),
                source_line,
                source_end_line: source_line + route_line.lines().count().saturating_sub(1),
                source_range: source_range_for_text(index, &route_line),
                evidence: vec!["http_route_syntax".to_string()],
                properties: BTreeMap::new(),
            });
        }
        pending_prefix = None;
    }
    if let Some((route_path, method, handler_name, source_line)) =
        file_system_route(pack, path, source)
    {
        let handler = handler_name
            .and_then(|name| {
                resolve_symbol_at_indexed(symbol_index, path, &name, source_line.saturating_sub(1))
            })
            .and_then(|symbol| project_definition_for_symbol_indexed(symbol_index, &symbol));
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

fn java_mapping_annotation(source: &str) -> bool {
    [
        "@RequestMapping",
        "@GetMapping",
        "@PostMapping",
        "@PutMapping",
        "@PatchMapping",
        "@DeleteMapping",
    ]
    .iter()
    .any(|annotation| source.contains(annotation))
}

fn joins_following_route_annotations(pack: &FrameworkPack, line: &str) -> bool {
    (pack.language == "java"
        && [
            "@GET", "@POST", "@PUT", "@PATCH", "@DELETE", "@HEAD", "@OPTIONS",
        ]
        .iter()
        .any(|marker| line.trim_start().starts_with(marker)))
        || (pack.language == "csharp" && line.trim_start().starts_with("[Http"))
}

fn framework_route_paths(
    pack: &FrameworkPack,
    route_line: &str,
    route_code: &str,
) -> Option<(Vec<String>, usize)> {
    if pack.language == "java" && java_mapping_annotation(route_code) {
        let paths = java_route_paths(route_line);
        let paths = if paths.is_empty() {
            vec![String::new()]
        } else {
            paths
        };
        let end = first_route_path(route_line)
            .map(|(_, end)| end)
            .or_else(|| route_line.find(')').map(|end| end + 1))
            .unwrap_or(route_line.len());
        return Some((paths, end));
    }

    if pack.language == "csharp" {
        let method_paths = annotation_route_paths(
            route_line,
            &[
                "[HttpGet",
                "[HttpPost",
                "[HttpPut",
                "[HttpPatch",
                "[HttpDelete",
                "[HttpHead",
                "[HttpOptions",
            ],
        );
        if let Some((paths, end)) = method_paths {
            if paths.iter().any(|path| !path.is_empty()) {
                return Some((paths, end));
            }
            if let Some(route) = annotation_route_paths(route_line, &["[Route"]) {
                return Some(route);
            }
            return Some((paths, end));
        }
    }

    let annotation_markers: &[&str] = if pack.id == "nestjs" {
        &[
            "@Get", "@Post", "@Put", "@Patch", "@Delete", "@Head", "@Options",
        ]
    } else if pack.id == "django" {
        &["re_path", "path"]
    } else if pack.language == "java" && matches!(pack.id.as_str(), "jakarta-ee" | "quarkus") {
        if let Some(result) = annotation_route_paths(route_line, &["@Path"]) {
            return Some(result);
        }
        return Some((vec![String::new()], route_line.len()));
    } else if pack.language == "java" && pack.id == "micronaut" {
        &[
            "@Get", "@Post", "@Put", "@Patch", "@Delete", "@Head", "@Options",
        ]
    } else {
        &[]
    };
    if !annotation_markers.is_empty() {
        if let Some(paths) = annotation_route_paths(route_line, annotation_markers) {
            return Some(paths);
        }
    }

    let (route_path, end) = first_route_path(route_line)?;
    let paths = route_paths(route_line);
    Some((
        if paths.is_empty() {
            vec![route_path]
        } else {
            paths
        },
        end,
    ))
}

fn declared_type_name(line: &str) -> Option<String> {
    ["class ", "record ", "struct ", "interface "]
        .iter()
        .find_map(|keyword| identifier_after(line, keyword))
}

fn csharp_route_tokens(path: &str, type_name: Option<&str>, action: Option<&str>) -> String {
    let mut path = path.to_string();
    if let Some(controller) = type_name {
        let controller = controller.strip_suffix("Controller").unwrap_or(controller);
        path = path.replace("[controller]", controller);
    }
    if let Some(action) = action {
        path = path.replace("[action]", action);
    }
    path
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

fn source_range_for_text(start_line: usize, text: &str) -> Vec<i32> {
    let lines = text.lines().collect::<Vec<_>>();
    let end_line = start_line + lines.len().saturating_sub(1);
    vec![
        start_line as i32,
        0,
        end_line as i32,
        lines
            .last()
            .map(|line| line.chars().count())
            .unwrap_or_default() as i32,
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
    let file_name = segments.pop()?;
    let mut file = file_name
        .rsplit_once('.')
        .map(|(stem, _)| stem)
        .unwrap_or(file_name);
    let file_method = if pack.id == "nuxt" {
        file.rsplit_once('.').and_then(|(stem, suffix)| {
            let method = filesystem_http_method(suffix)?;
            file = stem;
            Some(method)
        })
    } else {
        None
    };
    if matches!(file, "index" | "page" | "+page" | "+server" | "route") {
        // directory path already represents the route
    } else {
        segments.push(file);
    }
    let route = segments
        .into_iter()
        .filter(|segment| !segment.is_empty())
        .filter_map(|segment| filesystem_route_segment(pack.id.as_str(), segment))
        .collect::<Vec<_>>();
    let route_path = if route.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", route.join("/"))
    };
    let exported_method = exported_http_handler(source);
    let method = file_method
        .or_else(|| exported_method.map(|(method, _)| method))
        .unwrap_or("ANY")
        .to_string();
    let handler = if source.contains("onRequest") {
        Some("onRequest".to_string())
    } else if source.contains("defineEventHandler(handler") {
        Some("handler".to_string())
    } else if source.contains("defineEventHandler") {
        Some("default".to_string())
    } else if let Some((method, _)) = exported_method {
        Some(method.to_string())
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
    let source_line = exported_method
        .map(|(_, line)| line)
        .or_else(|| {
            source
                .lines()
                .position(|line| {
                    line.contains("function GET")
                        || line.contains("export default")
                        || line.contains("onRequest")
                })
                .map(|line| line + 1)
        })
        .unwrap_or(1);
    Some((route_path, method, handler, source_line))
}

fn filesystem_http_method(value: &str) -> Option<&'static str> {
    match value.to_ascii_lowercase().as_str() {
        "get" => Some("GET"),
        "post" => Some("POST"),
        "put" => Some("PUT"),
        "patch" => Some("PATCH"),
        "delete" => Some("DELETE"),
        "head" => Some("HEAD"),
        "options" => Some("OPTIONS"),
        _ => None,
    }
}

fn filesystem_route_segment(pack: &str, segment: &str) -> Option<String> {
    if matches!(pack, "nextjs" | "sveltekit")
        && ((segment.starts_with('(') && segment.ends_with(')')) || segment.starts_with('@'))
    {
        return None;
    }
    if segment.starts_with("[[...") && segment.ends_with("]]") {
        return Some(format!("*{}", &segment[5..segment.len() - 2]));
    }
    if segment.starts_with("[...") && segment.ends_with(']') {
        return Some(format!("*{}", &segment[4..segment.len() - 1]));
    }
    if segment.starts_with('[') && segment.ends_with(']') {
        return Some(format!(":{}", &segment[1..segment.len() - 1]));
    }
    Some(segment.to_string())
}

fn exported_http_handler(source: &str) -> Option<(&'static str, usize)> {
    let methods = ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"];
    for (line, text) in source.lines().enumerate() {
        let trimmed = text.trim_start();
        for method in methods {
            let markers = [
                format!("export async function {method}"),
                format!("export function {method}"),
                format!("export const {method}"),
                format!("export let {method}"),
                format!("export var {method}"),
            ];
            if markers.iter().any(|marker| {
                trimmed.starts_with(marker)
                    && trimmed[marker.len()..]
                        .chars()
                        .next()
                        .is_none_or(|value| !value.is_ascii_alphanumeric() && value != '_')
            }) {
                return Some((method, line + 1));
            }
        }
    }
    None
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
            if pack.language == "java"
                && output == "DEPENDENCY"
                && java_constructor_is_injection(&lines, index, &pack.id)
            {
                let dependencies = java_constructor_dependency_types(&lines, index);
                if !dependencies.is_empty() {
                    let handler_name = nearby_handler(&lines, index);
                    let handler = handler_name
                        .as_deref()
                        .and_then(|name| resolve_symbol_at_indexed(symbol_index, path, name, index))
                        .filter(|symbol| project_symbol_is_defined_indexed(symbol_index, symbol));
                    for target in dependencies {
                        let mut properties = BTreeMap::new();
                        properties.insert("target".to_string(), target);
                        facts.push(FrameworkFact {
                            id: format!(
                                "fact:{}:{}:{}:{}:{}",
                                pack.id,
                                output,
                                path,
                                index + 1,
                                properties["target"]
                            ),
                            kind: output.clone(),
                            framework: pack.id.clone(),
                            symbol: handler.clone(),
                            method: None,
                            path: None,
                            source_file: path.to_string(),
                            source_line: index + 1,
                            source_end_line: index + 1,
                            source_range: line_source_range(index, line),
                            evidence: vec!["java_constructor_injection".to_string()],
                            properties,
                        });
                    }
                    continue;
                }
            }
            let Some(evidence) = output_evidence(pack, output, line) else {
                continue;
            };
            let dependency_context = (pack.language == "java" && output == "DEPENDENCY")
                .then(|| java_dependency_annotation_context(&lines, index))
                .flatten();
            let fact_line = dependency_context.as_deref().unwrap_or(line);
            let handler_name =
                if pack.language == "java" && matches!(output.as_str(), "SERVICE" | "COMPONENT") {
                    fact_target_name(output, line).or_else(|| java_nearby_type(&lines, index))
                } else {
                    fact_target_name(output, fact_line).or_else(|| nearby_handler(&lines, index))
                };
            let symbol = handler_name
                .as_ref()
                .and_then(|name| {
                    if pack.language == "java" && matches!(output.as_str(), "SERVICE" | "COMPONENT")
                    {
                        resolve_java_type_indexed(symbol_index, path, name)
                    } else if matches!(pack.language.as_str(), "javascript" | "typescript") {
                        resolve_symbol_on_line_indexed(symbol_index, path, name, index)
                    } else {
                        resolve_symbol_at_indexed(symbol_index, path, name, index)
                    }
                })
                .and_then(|symbol| project_definition_for_symbol_indexed(symbol_index, &symbol));
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

fn java_constructor_dependency_types(lines: &[&str], index: usize) -> Vec<String> {
    if !lines.get(index).is_some_and(|line| line.contains('(')) {
        return Vec::new();
    }
    let Some(signature) = lines
        .iter()
        .skip(index)
        .take(16)
        .scan(String::new(), |buffer, line| {
            if !buffer.is_empty() {
                buffer.push(' ');
            }
            buffer.push_str(line.trim());
            Some(buffer.clone())
        })
        .find(|value| value.contains(')'))
    else {
        return Vec::new();
    };
    let Some(open) = signature.find('(') else {
        return Vec::new();
    };
    let before_open = signature[..open].trim();
    let Some(constructor) = before_open
        .rsplit(|value: char| !value.is_ascii_alphanumeric() && value != '_')
        .next()
    else {
        return Vec::new();
    };
    if constructor.is_empty()
        || !constructor
            .chars()
            .next()
            .is_some_and(|value| value.is_ascii_uppercase())
        || java_enclosing_type(lines, index).as_deref() != Some(constructor)
        || !(before_open.contains("public ")
            || before_open.contains("protected ")
            || before_open.contains("private ")
            || before_open
                .split_whitespace()
                .filter(|token| *token != constructor)
                .all(|token| token.starts_with('@') || token.starts_with('<')))
    {
        return Vec::new();
    }
    let Some(close) = signature[open + 1..]
        .find(')')
        .map(|value| value + open + 1)
    else {
        return Vec::new();
    };
    signature[open + 1..close]
        .split(',')
        .filter_map(|parameter| {
            parameter
                .split_whitespace()
                .filter(|token| !token.starts_with('@'))
                .map(|token| {
                    token.trim_matches(|value: char| {
                        !value.is_ascii_alphanumeric() && value != '.' && value != '_'
                    })
                })
                .find(|token| {
                    token
                        .chars()
                        .next()
                        .is_some_and(|value| value.is_ascii_uppercase())
                })
                .map(|token| token.split('<').next().unwrap_or(token).to_string())
        })
        .collect()
}

fn java_constructor_is_injection(lines: &[&str], index: usize, framework: &str) -> bool {
    for (offset, line) in lines.iter().take(index + 1).rev().take(5).enumerate() {
        if line.contains("@Autowired") || line.contains("@Inject") {
            return true;
        }
        if offset > 0 && !line.trim().is_empty() {
            break;
        }
    }
    if !matches!(
        framework,
        "spring" | "spring-boot" | "spring-mvc" | "spring-webflux"
    ) {
        return false;
    }
    let Some(type_name) = java_enclosing_type(lines, index) else {
        return false;
    };
    let Some(type_index) =
        lines
            .iter()
            .enumerate()
            .take(index + 1)
            .rev()
            .find_map(|(line_index, line)| {
                (identifier_after(line, "class ").as_deref() == Some(type_name.as_str())
                    || identifier_after(line, "record ").as_deref() == Some(type_name.as_str()))
                .then_some(line_index)
            })
    else {
        return false;
    };
    for line in lines.iter().take(type_index).rev().take(8) {
        if line.trim().is_empty() {
            continue;
        }
        if [
            "@Component",
            "@Service",
            "@Repository",
            "@Controller",
            "@RestController",
            "@Configuration",
        ]
        .iter()
        .any(|annotation| line.contains(annotation))
        {
            return true;
        }
        break;
    }
    false
}

fn java_dependency_annotation_context(lines: &[&str], index: usize) -> Option<String> {
    let line = *lines.get(index)?;
    if !(line.contains("@Autowired") || line.contains("@Inject")) || line.contains('(') {
        return None;
    }
    let mut context = line.trim().to_string();
    for next in lines.iter().skip(index + 1).take(4) {
        let trimmed = next.trim();
        if next.contains('(') && !trimmed.starts_with('@') {
            return None;
        }
        context.push(' ');
        context.push_str(trimmed);
        if next.contains(';') {
            return Some(context);
        }
    }
    None
}

fn java_nearby_type(lines: &[&str], index: usize) -> Option<String> {
    lines.iter().skip(index).take(8).find_map(|line| {
        ["class ", "record ", "interface ", "enum "]
            .iter()
            .find_map(|keyword| identifier_after(line, keyword))
    })
}

fn java_enclosing_type(lines: &[&str], index: usize) -> Option<String> {
    lines
        .iter()
        .take(index + 1)
        .rev()
        .take(200)
        .find_map(|line| {
            identifier_after(line, "class ").or_else(|| identifier_after(line, "record "))
        })
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
    let property_descriptor = symbol.ends_with(':');
    let symbol = symbol.trim_end_matches(['.', ':', '/']);
    let symbol = symbol.trim_end_matches('#');
    let symbol = symbol.rsplit(['#', '.', ':', '/']).next().unwrap_or(symbol);
    let symbol = symbol.split('(').next().unwrap_or(symbol);
    let symbol = symbol.split_whitespace().last().unwrap_or(symbol);
    if property_descriptor {
        symbol.trim_end_matches(char::is_numeric)
    } else {
        symbol
    }
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
