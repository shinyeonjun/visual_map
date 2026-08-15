pub(crate) fn default_confirmed_status() -> String {
    "confirmed".to_string()
}

pub(crate) fn map_status(status: &str) -> String {
    match status {
        "confirmed" => "verified".to_string(),
        "ambiguous" => "shared".to_string(),
        _ => "candidate".to_string(),
    }
}

pub(crate) fn map_feature_status(status: &str) -> String {
    match status {
        "confirmed" => "verified".to_string(),
        _ => "candidate".to_string(),
    }
}

pub(crate) fn step_status(node_kind: &str, edge_status: Option<&str>) -> String {
    if node_kind == "dynamicBoundary" {
        return "candidate".to_string();
    }
    match edge_status {
        Some("confirmed") | None => "verified".to_string(),
        _ => "candidate".to_string(),
    }
}

pub(crate) fn is_boundary_flow_kind(kind: &str) -> bool {
    kind == "entry" || kind == "exit"
}
