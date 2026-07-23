use std::{collections::HashSet, fs, path::Path};

use super::model::{CodeHandle, CodeInventory, CodeInventoryItem};

const MAX_CSHARP_SOURCE_BYTES: u64 = 2 * 1024 * 1024;
const HTTP_METHODS: &[&str] = &[
    "Delete", "Get", "Head", "Options", "Patch", "Post", "Put", "Trace",
];

#[derive(Debug)]
struct RouteDeclaration {
    method: String,
    argument: String,
    line: u64,
}

pub(super) fn enrich_fastendpoints_routes(repo_path: &str, inventory: &mut CodeInventory) {
    let discovered = discover_fastendpoints_routes(repo_path, inventory);
    if discovered.is_empty() {
        return;
    }

    let mut route_ids = inventory
        .routes
        .iter()
        .map(|route| route.id.clone())
        .collect::<HashSet<_>>();
    let mut handles = inventory.handles.clone();
    for (route, handle) in discovered {
        if route_ids.insert(route.id.clone()) {
            inventory.routes.push(route);
            handles.push(handle);
        }
    }
    super::code::attach_route_handles(handles, inventory);
}

fn discover_fastendpoints_routes(
    repo_path: &str,
    inventory: &CodeInventory,
) -> Vec<(CodeInventoryItem, CodeHandle)> {
    let items = code_items(inventory);
    let configurations = items
        .iter()
        .copied()
        .filter(|item| item.engine_label == "Method" && item.name == "Configure")
        .collect::<Vec<_>>();
    let mut discovered = Vec::new();

    for configure in configurations {
        let Some(path) = configure
            .file_path
            .as_deref()
            .filter(|path| is_csharp_path(path))
        else {
            continue;
        };
        let Some(parent) = parent_type(configure) else {
            continue;
        };
        let Some(endpoint_type) = items.iter().copied().find(|item| {
            item.id == parent
                && is_class_like(item)
                && same_source_path(item.file_path.as_deref(), Some(path))
        }) else {
            continue;
        };
        let Some(source) = read_csharp_source(repo_path, path) else {
            continue;
        };
        if !is_fastendpoints_type(&source, endpoint_type, configure) {
            continue;
        }
        let Some(declaration) = route_declaration(&source, configure) else {
            continue;
        };
        let Some(route_path) =
            resolve_route_path(repo_path, inventory, path, &source, &declaration.argument)
        else {
            continue;
        };
        let handlers = items
            .iter()
            .copied()
            .filter(|item| {
                item.engine_label == "Method"
                    && matches!(item.name.as_str(), "ExecuteAsync" | "HandleAsync")
                    && same_source_path(item.file_path.as_deref(), Some(path))
                    && parent_type(item).as_deref() == Some(parent.as_str())
                    && item.line.is_some()
            })
            .collect::<Vec<_>>();
        if handlers.len() != 1 {
            continue;
        }
        let handler = handlers[0];
        let base_route_id = format!("__route__{}__{}", declaration.method, route_path);
        let route_id = super::code::route_binding_id(&base_route_id, &handler.id);
        let route = CodeInventoryItem {
            id: route_id.clone(),
            kind: "Route".to_string(),
            name: route_path.clone(),
            project: inventory.project.clone(),
            qualified_name: route_id.clone(),
            engine_label: "Route".to_string(),
            file_path: configure.file_path.clone(),
            line: Some(declaration.line),
            column: None,
            end_line: Some(declaration.line),
            end_column: None,
            detail: serde_json::json!({
                "framework": "FastEndpoints",
                "routeMethod": declaration.method,
                "routePath": route_path,
                "routePathSource": "fastendpoints-static-configure",
                "handlerQualifiedName": handler.qualified_name,
            }),
        };
        discovered.push((
            route,
            CodeHandle {
                handler: handler.id.clone(),
                route: route_id,
            },
        ));
    }

    discovered.sort_by(|left, right| left.0.id.cmp(&right.0.id));
    discovered
}

fn resolve_route_path(
    repo_path: &str,
    inventory: &CodeInventory,
    configure_path: &str,
    configure_source: &str,
    argument: &str,
) -> Option<String> {
    if let Some(path) = parse_route_literal(argument) {
        return Some(path);
    }

    let (type_name, field_name) = member_reference(argument)?;
    let configure_dir = source_parent(configure_path);
    let mut all = Vec::new();
    let mut nearby = Vec::new();
    for item in code_items(inventory)
        .into_iter()
        .filter(|item| is_class_like(item) && item.name == type_name)
    {
        let Some(path) = item
            .file_path
            .as_deref()
            .filter(|path| is_csharp_path(path))
        else {
            continue;
        };
        all.push(item);
        if source_parent(path) == configure_dir {
            nearby.push(item);
        }
    }
    let candidate = if nearby.len() == 1 {
        nearby[0]
    } else if nearby.is_empty() && all.len() == 1 {
        all[0]
    } else {
        return None;
    };
    let candidate_path = candidate.file_path.as_deref()?;
    let owned_source;
    let source = if same_source_path(Some(candidate_path), Some(configure_path)) {
        configure_source
    } else {
        owned_source = read_csharp_source(repo_path, candidate_path)?;
        &owned_source
    };
    type_const_string(source, candidate, field_name)
}

fn route_declaration(source: &str, configure: &CodeInventoryItem) -> Option<RouteDeclaration> {
    let (start, lines) = bounded_lines(source, configure)?;
    if lines
        .iter()
        .any(|line| line.contains("/*") || line.contains("*/"))
    {
        return None;
    }
    let mut declarations = Vec::new();

    for (offset, line) in lines.iter().enumerate() {
        let line = line.trim();
        if line.starts_with("//") {
            continue;
        }
        let Some(name_end) = line.bytes().position(|byte| !is_identifier_continue(byte)) else {
            continue;
        };
        let method = &line[..name_end];
        if !HTTP_METHODS.contains(&method) {
            continue;
        }
        let remainder = line[name_end..].trim();
        if !remainder.starts_with('(') || !remainder.ends_with(");") {
            continue;
        }
        let argument = remainder[1..remainder.len() - 2].trim();
        if argument.is_empty() {
            continue;
        }
        declarations.push(RouteDeclaration {
            method: method.to_ascii_uppercase(),
            argument: argument.to_string(),
            line: start + offset as u64,
        });
    }

    (declarations.len() == 1).then(|| declarations.remove(0))
}

fn is_fastendpoints_type(
    source: &str,
    endpoint_type: &CodeInventoryItem,
    configure: &CodeInventoryItem,
) -> bool {
    let Some(start) = endpoint_type.line else {
        return false;
    };
    let Some(configure_line) = configure.line else {
        return false;
    };
    let end = configure_line.saturating_sub(1);
    let Some(header) = bounded_text(source, start, end) else {
        return false;
    };
    let header = header.split('{').next().unwrap_or_default();
    if header.contains("//") || header.contains("/*") || header.contains("*/") {
        return false;
    }
    let Some(inheritance) = header.split_once(':').map(|(_, value)| value) else {
        return false;
    };

    inheritance
        .split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
        .any(|token| matches!(token, "Endpoint" | "EndpointWithoutRequest"))
}

fn type_const_string(
    source: &str,
    type_item: &CodeInventoryItem,
    field_name: &str,
) -> Option<String> {
    let (_, lines) = bounded_lines(source, type_item)?;
    if lines
        .iter()
        .any(|line| line.contains("/*") || line.contains("*/"))
    {
        return None;
    }
    let mut values = Vec::new();

    for line in lines {
        if line.trim_start().starts_with("//") {
            continue;
        }
        let Some(const_index) = line.find("const") else {
            continue;
        };
        let declaration = &line[const_index..];
        let tokens = identifier_spans(declaration);
        let Some(window) = tokens.windows(3).find(|window| {
            &declaration[window[0].0..window[0].1] == "const"
                && &declaration[window[1].0..window[1].1] == "string"
                && &declaration[window[2].0..window[2].1] == field_name
        }) else {
            continue;
        };
        let bytes = declaration.as_bytes();
        let equals = skip_whitespace(bytes, window[2].1);
        if bytes.get(equals) != Some(&b'=') {
            continue;
        }
        let literal_start = skip_whitespace(bytes, equals + 1);
        let Some((value, literal_end)) = parse_normal_string(declaration, literal_start) else {
            continue;
        };
        let semicolon = skip_whitespace(bytes, literal_end);
        if bytes.get(semicolon) == Some(&b';')
            && declaration[semicolon + 1..].trim().is_empty()
            && is_route_path(&value)
        {
            values.push(value);
        }
    }

    (values.len() == 1).then(|| values.remove(0))
}

fn read_csharp_source(repo_path: &str, source_path: &str) -> Option<String> {
    let resolved = crate::source::resolve_repo_source(repo_path, source_path).ok()?;
    let metadata = resolved.metadata().ok()?;
    if metadata.len() > MAX_CSHARP_SOURCE_BYTES {
        return None;
    }
    fs::read_to_string(resolved).ok()
}

fn bounded_lines<'a>(source: &'a str, item: &CodeInventoryItem) -> Option<(u64, Vec<&'a str>)> {
    let start = item.line?;
    let end = item.end_line.filter(|end| *end >= start)?;
    let lines = source.lines().collect::<Vec<_>>();
    let start_index = usize::try_from(start.checked_sub(1)?).ok()?;
    let end_index = usize::try_from(end).ok()?.min(lines.len());
    (start_index < end_index).then(|| (start, lines[start_index..end_index].to_vec()))
}

fn bounded_text(source: &str, start: u64, end: u64) -> Option<String> {
    if end < start {
        return None;
    }
    let lines = source.lines().collect::<Vec<_>>();
    let start_index = usize::try_from(start.checked_sub(1)?).ok()?;
    let end_index = usize::try_from(end).ok()?.min(lines.len());
    (start_index < end_index).then(|| lines[start_index..end_index].join("\n"))
}

fn parse_route_literal(argument: &str) -> Option<String> {
    let argument = argument.trim();
    let (value, end) = parse_normal_string(argument, 0)?;
    (skip_whitespace(argument.as_bytes(), end) == argument.len() && is_route_path(&value))
        .then_some(value)
}

fn parse_normal_string(source: &str, start: usize) -> Option<(String, usize)> {
    let bytes = source.as_bytes();
    if bytes.get(start) != Some(&b'"') {
        return None;
    }
    let mut value = String::new();
    let mut cursor = start + 1;

    while cursor < bytes.len() {
        match bytes[cursor] {
            b'"' => return Some((value, cursor + 1)),
            b'\\' => {
                let escaped = *bytes.get(cursor + 1)?;
                match escaped {
                    b'"' => value.push('"'),
                    b'\'' => value.push('\''),
                    b'\\' => value.push('\\'),
                    b'n' => value.push('\n'),
                    b'r' => value.push('\r'),
                    b't' => value.push('\t'),
                    _ => return None,
                }
                cursor += 2;
            }
            byte if byte.is_ascii() => {
                value.push(byte as char);
                cursor += 1;
            }
            _ => {
                let character = source[cursor..].chars().next()?;
                value.push(character);
                cursor += character.len_utf8();
            }
        }
    }
    None
}

fn member_reference(argument: &str) -> Option<(&str, &str)> {
    let parts = argument.trim().split('.').collect::<Vec<_>>();
    if parts.len() < 2 || parts.iter().any(|part| !is_identifier(part)) {
        return None;
    }
    Some((parts[parts.len() - 2], parts[parts.len() - 1]))
}

fn code_items(inventory: &CodeInventory) -> Vec<&CodeInventoryItem> {
    inventory
        .handlers
        .iter()
        .chain(inventory.services.iter())
        .chain(inventory.repositories.iter())
        .chain(inventory.functions.iter())
        .chain(inventory.classes.iter())
        .chain(inventory.modules.iter())
        .chain(inventory.unknown.iter())
        .collect()
}

fn parent_type(item: &CodeInventoryItem) -> Option<String> {
    [
        "parent_class",
        "parentClass",
        "parent_qualified_name",
        "parentQualifiedName",
    ]
    .into_iter()
    .find_map(|key| {
        item.detail
            .get(key)
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    })
}

fn identifier_spans(source: &str) -> Vec<(usize, usize)> {
    let bytes = source.as_bytes();
    let mut spans = Vec::new();
    let mut cursor = 0;
    while cursor < bytes.len() {
        if !is_identifier_start(bytes[cursor]) {
            cursor += 1;
            continue;
        }
        let start = cursor;
        cursor += 1;
        while cursor < bytes.len() && is_identifier_continue(bytes[cursor]) {
            cursor += 1;
        }
        spans.push((start, cursor));
    }
    spans
}

fn same_source_path(left: Option<&str>, right: Option<&str>) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => normalize_source_path(left) == normalize_source_path(right),
        _ => false,
    }
}

fn source_parent(path: &str) -> String {
    Path::new(&normalize_source_path(path))
        .parent()
        .map(|path| normalize_source_path(&path.to_string_lossy()))
        .unwrap_or_default()
}

fn normalize_source_path(path: &str) -> String {
    path.replace('\\', "/")
}

fn skip_whitespace(bytes: &[u8], from: usize) -> usize {
    let mut cursor = from;
    while bytes
        .get(cursor)
        .is_some_and(|byte| byte.is_ascii_whitespace())
    {
        cursor += 1;
    }
    cursor
}

fn is_class_like(item: &CodeInventoryItem) -> bool {
    matches!(item.engine_label.as_str(), "Class" | "Record" | "Struct")
}

fn is_csharp_path(path: &str) -> bool {
    path.to_ascii_lowercase().ends_with(".cs")
}

fn is_route_path(path: &str) -> bool {
    path.starts_with('/') && !path.chars().any(char::is_control)
}

fn is_identifier(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.first().is_some_and(|byte| is_identifier_start(*byte))
        && bytes
            .get(1..)
            .is_some_and(|rest| rest.iter().all(|byte| is_identifier_continue(*byte)))
}

fn is_identifier_start(byte: u8) -> bool {
    byte == b'_' || byte.is_ascii_alphabetic()
}

fn is_identifier_continue(byte: u8) -> bool {
    is_identifier_start(byte) || byte.is_ascii_digit()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::model::CodeInventorySummary;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn item(
        name: &str,
        id: &str,
        label: &str,
        path: &str,
        lines: (u64, u64),
        parent: Option<&str>,
    ) -> CodeInventoryItem {
        CodeInventoryItem {
            id: id.to_string(),
            kind: label.to_string(),
            name: name.to_string(),
            project: "test".to_string(),
            qualified_name: id.to_string(),
            engine_label: label.to_string(),
            file_path: Some(path.to_string()),
            line: Some(lines.0),
            column: None,
            end_line: Some(lines.1),
            end_column: None,
            detail: parent
                .map(|parent| serde_json::json!({ "parent_class": parent }))
                .unwrap_or_else(|| serde_json::json!({})),
        }
    }

    fn inventory(
        functions: Vec<CodeInventoryItem>,
        classes: Vec<CodeInventoryItem>,
    ) -> CodeInventory {
        CodeInventory {
            project: "test".to_string(),
            routes: Vec::new(),
            services: Vec::new(),
            files: Vec::new(),
            handlers: Vec::new(),
            repositories: Vec::new(),
            functions,
            classes,
            modules: Vec::new(),
            unknown: Vec::new(),
            summary: CodeInventorySummary {
                routes: 0,
                handlers: 0,
                services: 0,
                repositories: 0,
                functions: 0,
                classes: 0,
                modules: 0,
                files: 0,
                unknown: 0,
            },
            architecture: None,
            calls: Vec::new(),
            handles: Vec::new(),
            partial: false,
        }
    }

    fn temp_root(label: &str) -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "backend-map-fastendpoints-{label}-{}-{nonce}",
            std::process::id()
        ))
    }

    #[test]
    fn links_literal_and_indexed_constant_routes_to_exact_handlers() {
        let root = temp_root("confirmed");
        let list = root.join("Endpoints/List.cs");
        let get = root.join("Contributors/GetById.cs");
        let request = root.join("Contributors/GetById.Request.cs");
        fs::create_dir_all(list.parent().unwrap()).unwrap();
        fs::create_dir_all(get.parent().unwrap()).unwrap();
        fs::write(
            &list,
            r#"public class List : EndpointWithoutRequest
{
  public override void Configure()
  {
    Get("/Contributors");
  }
  public override Task HandleAsync(CancellationToken ct) => Task.CompletedTask;
}
"#,
        )
        .unwrap();
        fs::write(
            &get,
            r#"public class GetById : Endpoint<GetByIdRequest>
{
  public override void Configure()
  {
    Get(GetByIdRequest.Route);
  }
  public override Task ExecuteAsync(GetByIdRequest request, CancellationToken ct)
    => Task.CompletedTask;
}
"#,
        )
        .unwrap();
        fs::write(
            &request,
            r#"public class GetByIdRequest
{
  public const string Route = "/Contributors/{ContributorId:int}";
}
"#,
        )
        .unwrap();
        let list_parent = "app.Endpoints.List";
        let get_parent = "app.Contributors.GetById";
        let mut inventory = inventory(
            vec![
                item(
                    "Configure",
                    "app.Endpoints.List.Configure",
                    "Method",
                    "Endpoints/List.cs",
                    (3, 6),
                    Some(list_parent),
                ),
                item(
                    "HandleAsync",
                    "app.Endpoints.List.HandleAsync",
                    "Method",
                    "Endpoints/List.cs",
                    (7, 7),
                    Some(list_parent),
                ),
                item(
                    "Configure",
                    "app.Contributors.GetById.Configure",
                    "Method",
                    "Contributors/GetById.cs",
                    (3, 6),
                    Some(get_parent),
                ),
                item(
                    "ExecuteAsync",
                    "app.Contributors.GetById.ExecuteAsync",
                    "Method",
                    "Contributors/GetById.cs",
                    (7, 8),
                    Some(get_parent),
                ),
            ],
            vec![
                item(
                    "List",
                    list_parent,
                    "Class",
                    "Endpoints/List.cs",
                    (1, 8),
                    None,
                ),
                item(
                    "GetById",
                    get_parent,
                    "Class",
                    "Contributors/GetById.cs",
                    (1, 9),
                    None,
                ),
                item(
                    "GetByIdRequest",
                    "app.Contributors.GetByIdRequest",
                    "Class",
                    "Contributors/GetById.Request.cs",
                    (1, 4),
                    None,
                ),
            ],
        );

        enrich_fastendpoints_routes(root.to_str().unwrap(), &mut inventory);

        assert_eq!(
            inventory
                .routes
                .iter()
                .map(|route| (
                    route.detail["routeMethod"].as_str().unwrap(),
                    route.name.as_str()
                ))
                .collect::<Vec<_>>(),
            vec![
                ("GET", "/Contributors"),
                ("GET", "/Contributors/{ContributorId:int}")
            ]
        );
        assert_eq!(inventory.handlers.len(), 2);
        assert_eq!(inventory.handles.len(), 2);
        assert!(inventory
            .routes
            .iter()
            .all(|route| { route.detail["routePathSource"] == "fastendpoints-static-configure" }));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn fails_closed_for_dynamic_non_endpoint_and_ambiguous_declarations() {
        let root = temp_root("uncertain");
        let dynamic = root.join("Endpoints/Dynamic.cs");
        let configuration = root.join("Data/EntityConfiguration.cs");
        fs::create_dir_all(dynamic.parent().unwrap()).unwrap();
        fs::create_dir_all(configuration.parent().unwrap()).unwrap();
        fs::write(
            &dynamic,
            r#"public class Dynamic : EndpointWithoutRequest
{
  public override void Configure()
  {
    Get($"/{nameof(Item)}s");
    Post("/also-ambiguous");
  }
  public override Task ExecuteAsync(CancellationToken ct) => Task.CompletedTask;
  public override Task HandleAsync(CancellationToken ct) => Task.CompletedTask;
}
"#,
        )
        .unwrap();
        fs::write(
            &configuration,
            r#"public class EntityConfiguration : IEntityTypeConfiguration<Entity>
{
  public void Configure()
  {
    Get("/not-an-api");
  }
  public Task ExecuteAsync() => Task.CompletedTask;
}
"#,
        )
        .unwrap();
        let dynamic_parent = "app.Endpoints.Dynamic";
        let config_parent = "app.Data.EntityConfiguration";
        let mut inventory = inventory(
            vec![
                item(
                    "Configure",
                    "app.Endpoints.Dynamic.Configure",
                    "Method",
                    "Endpoints/Dynamic.cs",
                    (3, 7),
                    Some(dynamic_parent),
                ),
                item(
                    "ExecuteAsync",
                    "app.Endpoints.Dynamic.ExecuteAsync",
                    "Method",
                    "Endpoints/Dynamic.cs",
                    (8, 8),
                    Some(dynamic_parent),
                ),
                item(
                    "HandleAsync",
                    "app.Endpoints.Dynamic.HandleAsync",
                    "Method",
                    "Endpoints/Dynamic.cs",
                    (9, 9),
                    Some(dynamic_parent),
                ),
                item(
                    "Configure",
                    "app.Data.EntityConfiguration.Configure",
                    "Method",
                    "Data/EntityConfiguration.cs",
                    (3, 6),
                    Some(config_parent),
                ),
                item(
                    "ExecuteAsync",
                    "app.Data.EntityConfiguration.ExecuteAsync",
                    "Method",
                    "Data/EntityConfiguration.cs",
                    (7, 7),
                    Some(config_parent),
                ),
            ],
            vec![
                item(
                    "Dynamic",
                    dynamic_parent,
                    "Class",
                    "Endpoints/Dynamic.cs",
                    (1, 10),
                    None,
                ),
                item(
                    "EntityConfiguration",
                    config_parent,
                    "Class",
                    "Data/EntityConfiguration.cs",
                    (1, 8),
                    None,
                ),
            ],
        );

        enrich_fastendpoints_routes(root.to_str().unwrap(), &mut inventory);

        assert!(inventory.routes.is_empty());
        assert!(inventory.handles.is_empty());
        fs::remove_dir_all(root).unwrap();
    }
}
