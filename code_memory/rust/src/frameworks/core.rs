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
        // Shared headers (`.h`, `.inc`) can belong to both C and C++.
        // Keep provider ownership singular, but let framework discovery see
        // the header from every matching language so declarations are not
        // silently hidden by catalog order.
        for language in LANGUAGES
            .iter()
            .filter(|item| path_matches_language(path, item.extensions))
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
        let mut matched_metadata_roots = HashSet::new();
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
                    matched_metadata_roots.insert(metadata_scope(relative));
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
                    let in_metadata_scope = matched_metadata_roots.is_empty()
                        || matched_metadata_roots
                            .iter()
                            .any(|root| path_is_in_scope(path, root));
                    (has_route_syntax_candidate(source, &pack.language)
                        || file_system_route(&pack, path, source).is_some())
                        && in_metadata_scope
                        || source_signal_files.contains(path)
                } else {
                    !restrict_to_signal_files || source_signal_files.contains(*path)
                }
            })
            .collect();

        let mut facts = Vec::new();
        let route_context = (pack.id == "fastapi" || pack.id == "minimal-api").then(|| {
            let mut context = FastApiRouteContext::default();
            if pack.id == "fastapi" {
                context.prefixes = build_fastapi_route_context(&sources).prefixes;
            }
            if pack.id == "minimal-api" {
                context.minimal_prefixes = build_minimal_api_route_context(&sources);
            }
            context
        });
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
                    route_context.as_ref(),
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
            if pack.id == "react" {
                extract_react_navigation_routes(&pack.language, path, source, &mut facts);
            }
        }
        for fact in &mut facts {
            if fact.kind == "HTTP_ROUTE" {
                fact.properties
                    .entry("routeSurface".to_string())
                    .or_insert_with(|| route_surface(&pack, &fact.source_file));
                fact.properties
                    .entry("runtime_reachability".to_string())
                    .or_insert_with(|| "not-assessed".to_string());
            }
            if source_path_is_test(&fact.source_file) {
                fact.properties
                    .entry("source_scope".to_string())
                    .or_insert_with(|| "test".to_string());
                fact.properties
                    .entry("isTest".to_string())
                    .or_insert_with(|| "true".to_string());
            }
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

