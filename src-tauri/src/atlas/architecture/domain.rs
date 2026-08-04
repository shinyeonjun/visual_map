fn atlas_truth_kind(link: &SnapshotLink, prefix: &str) -> Option<String> {
    match link.truth_class.as_str() {
        "confirmed" => Some(format!("{prefix}{}", link.kind)),
        "candidate" => Some(format!("candidate_{prefix}{}", link.kind)),
        "structural" | "" => Some(format!("structural_{prefix}{}", link.kind)),
        _ => None,
    }
}

fn atlas_projection_edge_rank(edge: &VisualEdge) -> u8 {
    if edge.kind.starts_with("candidate_") {
        2
    } else if edge.kind.starts_with("structural_") {
        1
    } else {
        0
    }
}

fn atlas_visible_endpoint<'a>(
    id: &'a str,
    item_by_id: &HashMap<&str, &'a InventoryItem>,
    visible_ids: &HashSet<&str>,
) -> Option<&'a str> {
    if visible_ids.contains(id) {
        return Some(id);
    }
    item_by_id
        .get(id)
        .and_then(|item| item.parent_id.as_deref())
        .filter(|parent| visible_ids.contains(parent))
}

fn route_domain(value: &str) -> Option<String> {
    let path = value
        .split_whitespace()
        .find(|part| part.starts_with('/'))
        .unwrap_or(value);
    path.trim_start_matches('/')
        .split('/')
        .find_map(text_domain)
}

fn path_domain(value: &str) -> Option<String> {
    let mut parts = value
        .split(['/', '\\'])
        .filter(|part| !part.is_empty() && *part != ".")
        .collect::<Vec<_>>();
    if parts.len() > 1 {
        // The final segment is normally a source file. Grouping by it merges every
        // `service.rs`/`index.ts` across the project instead of the owning folder.
        parts.pop();
    }
    parts.into_iter().rev().find_map(text_domain)
}

fn group_id_domain(value: &str) -> Option<String> {
    value
        .split(['/', '\\', '.', ':'])
        .filter(|part| !part.is_empty())
        .rev()
        .find_map(text_domain)
}

fn text_domain(value: &str) -> Option<String> {
    semantic_tokens(value).into_iter().next()
}

fn canonical_domain(value: &str) -> String {
    text_domain(value).unwrap_or_else(|| "other".to_string())
}

fn semantic_tokens(value: &str) -> Vec<String> {
    let mut words = String::with_capacity(value.len());
    let mut previous_lower = false;
    for character in value.chars() {
        if character.is_alphanumeric() {
            if character.is_uppercase() && previous_lower {
                words.push(' ');
            }
            for lower in character.to_lowercase() {
                words.push(lower);
            }
            previous_lower = character.is_lowercase();
        } else {
            words.push(' ');
            previous_lower = false;
        }
    }

    words
        .split_whitespace()
        .filter(|word| !is_generic_domain_word(word))
        .map(singular_domain)
        .collect()
}

fn singular_domain(value: &str) -> String {
    if value.len() > 3 && value.ends_with("ies") {
        return format!("{}y", &value[..value.len() - 3]);
    }
    for suffix in ["ches", "shes", "xes", "zes", "ses"] {
        if value.len() > suffix.len() && value.ends_with(suffix) {
            return value[..value.len() - 2].to_string();
        }
    }
    if value.len() > 2
        && value.ends_with('s')
        && !value.ends_with("ss")
        && !value.ends_with("us")
        && !value.ends_with("is")
    {
        return value[..value.len() - 1].to_string();
    }
    value.to_string()
}

fn is_generic_domain_word(value: &str) -> bool {
    value.len() < 2
        || value.chars().all(|character| character.is_ascii_digit())
        || (value.starts_with('v')
            && value[1..]
                .chars()
                .all(|character| character.is_ascii_digit()))
        || matches!(
            value,
            "api"
                | "app"
                | "apps"
                | "backend"
                | "code"
                | "common"
                | "controller"
                | "controllers"
                | "core"
                | "create"
                | "database"
                | "db"
                | "domain"
                | "domains"
                | "feature"
                | "features"
                | "find"
                | "get"
                | "handler"
                | "handlers"
                | "internal"
                | "id"
                | "java"
                | "kotlin"
                | "lib"
                | "libs"
                | "list"
                | "main"
                | "model"
                | "models"
                | "module"
                | "modules"
                | "package"
                | "packages"
                | "python"
                | "pkg"
                | "repository"
                | "repositories"
                | "repo"
                | "read"
                | "route"
                | "routes"
                | "schema"
                | "save"
                | "server"
                | "service"
                | "services"
                | "shared"
                | "source"
                | "src"
                | "test"
                | "tests"
                | "update"
                | "util"
                | "utils"
                | "write"
                | "delete"
                | "com"
                | "org"
                | "net"
                | "io"
                | "js"
                | "jsx"
                | "ts"
                | "tsx"
                | "rs"
                | "py"
                | "kt"
        )
}

fn is_default_schema(value: &str) -> bool {
    matches!(
        value.to_ascii_lowercase().as_str(),
        "public" | "dbo" | "main" | "default"
    )
}

fn slug(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

