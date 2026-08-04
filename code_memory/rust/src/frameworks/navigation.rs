fn route_surface(pack: &FrameworkPack, path: &str) -> String {
    let normalized = path.replace('\\', "/");
    match pack.id.as_str() {
        "nextjs"
            if filesystem_path_has_directory(&normalized, "pages")
                || filesystem_path_has_directory(&normalized, "app") =>
        {
            let file_is_route_handler = normalized
                .rsplit('/')
                .next()
                .is_some_and(|file| file.starts_with("route."));
            if file_is_route_handler
                || filesystem_path_has_directory(&normalized, "pages/api")
                || filesystem_path_has_directory(&normalized, "app/api")
            {
                "backend-api".to_string()
            } else {
                "ui-navigation".to_string()
            }
        }
        "nuxt" if filesystem_path_has_directory(&normalized, "pages") => {
            "ui-navigation".to_string()
        }
        "nuxt" if filesystem_path_has_directory(&normalized, "server") => "backend-api".to_string(),
        "sveltekit"
            if normalized.contains("/src/routes/") || normalized.starts_with("src/routes/") =>
        {
            if normalized
                .rsplit('/')
                .next()
                .is_some_and(|file| file.starts_with("+server."))
            {
                "backend-api".to_string()
            } else {
                "ui-navigation".to_string()
            }
        }
        _ => "backend-api".to_string(),
    }
}

fn extract_react_navigation_routes(
    language: &str,
    path: &str,
    source: &str,
    facts: &mut Vec<FrameworkFact>,
) {
    let comment_free = source_without_comments(source, language);
    let lines = comment_free.lines().collect::<Vec<_>>();
    for (index, line) in lines.iter().enumerate() {
        let route_source = lines[index..lines.len().min(index + 4)].join(" ");
        let route_tag = line.find("<Route").is_some_and(|start| {
            line[start + "<Route".len()..]
                .chars()
                .next()
                .is_none_or(|value| value.is_whitespace() || value == '>')
        });
        if !route_tag || !route_source.contains("path=") {
            continue;
        }
        let Some(path_value) = route_source
            .split_once("path=")
            .and_then(|(_, tail)| first_quoted_value(tail))
        else {
            continue;
        };
        let route_path = normalize_ui_route_path(&path_value);
        let source_line = index + 1;
        let id = format!("route:react:{}:{}:{}", path, source_line, route_path);
        if facts.iter().any(|fact| fact.id == id) {
            continue;
        }
        facts.push(FrameworkFact {
            id,
            kind: "HTTP_ROUTE".to_string(),
            framework: "react".to_string(),
            symbol: None,
            method: Some("ANY".to_string()),
            path: Some(route_path),
            source_file: path.to_string(),
            source_line,
            source_end_line: source_line,
            source_range: source_line_range(source, source_line),
            evidence: vec!["react-router-navigation".to_string()],
            properties: BTreeMap::from([("routeSurface".to_string(), "ui-navigation".to_string())]),
        });
    }
}

fn normalize_ui_route_path(path: &str) -> String {
    let path = path.trim();
    if path.is_empty() || path == "/" {
        "/".to_string()
    } else {
        format!("/{}", path.trim_start_matches('/'))
    }
}

fn filesystem_path_has_directory(path: &str, directory: &str) -> bool {
    path == directory
        || path.starts_with(&format!("{directory}/"))
        || path.contains(&format!("/{directory}/"))
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
    if pack.language == "ruby" && pack.id == "rack" {
        // Rack is often present in config.ru as the host for Rails/Sinatra.
        // That activation must not make Rack claim the application's route DSL.
        return source.contains("Rack::Builder")
            || source.lines().map(str::trim_start).any(|line| {
                line.strip_prefix("run ")
                    .is_some_and(|rest| !rest.trim_start().starts_with('#'))
                    || line
                        .strip_prefix("map ")
                        .is_some_and(|rest| rest.trim_start().starts_with(['"', '\'', '/']))
            });
    }
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

fn build_minimal_api_route_context(sources: &[(&str, &str)]) -> HashMap<String, String> {
    let has_endpoint_group_discovery = sources.iter().any(|(_, source)| {
        source.contains("GetProperty")
            && source.contains("IEndpointGroup")
            && source.contains("MapGroup(")
    });
    if !has_endpoint_group_discovery {
        return HashMap::new();
    }

    let default_template = sources.iter().find_map(|(_, source)| {
        source.lines().find_map(|line| {
            (line.contains("groupName") && line.contains("??"))
                .then(|| first_quoted_value(line))
                .flatten()
        })
    });
    let mut prefixes = HashMap::new();
    for &(path, source) in sources {
        if !path.ends_with(".cs") {
            continue;
        }
        let Some(class_name) = source.lines().find_map(|line| {
            (line.contains("class ") && line.contains("IEndpointGroup"))
                .then(|| declared_type_name(line))
                .flatten()
        }) else {
            continue;
        };
        let custom_prefix = source.lines().find_map(|line| {
            (line.contains("RoutePrefix") && line.contains("=>"))
                .then(|| first_quoted_value(line))
                .flatten()
        });
        let prefix = custom_prefix.or_else(|| {
            default_template
                .as_deref()
                .map(|template| template.replace("{groupName}", &class_name))
        });
        if let Some(prefix) = prefix {
            prefixes.insert(path.to_string(), prefix);
        }
    }
    prefixes
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
        "router.register(",
        "@api_view",
        "as_view(",
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

pub(crate) fn source_code_mask(source: &str, language: &str) -> String {
    source_mask(source, language, true)
}

fn source_path_is_test(path: &str) -> bool {
    let components = path
        .split(['/', '\\'])
        .map(|component| component.to_ascii_lowercase())
        .collect::<Vec<_>>();
    components.iter().any(|component| {
        matches!(
            component.as_str(),
            "test" | "tests" | "spec" | "specs" | "__tests__"
        ) || component.contains(".test.")
            || component.contains(".spec.")
            || component.ends_with("_test.go")
            || component.starts_with("test_")
    })
}

pub(crate) fn source_without_comments(source: &str, language: &str) -> String {
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
