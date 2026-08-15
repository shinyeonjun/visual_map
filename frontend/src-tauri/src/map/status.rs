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

pub(crate) fn edge_status(status: Option<&str>) -> String {
    match status {
        Some("confirmed") | None => "verified".to_string(),
        Some("ambiguous") => "shared".to_string(),
        _ => "candidate".to_string(),
    }
}

pub(crate) fn node_status(flow: &super::clean::FlowJson, node: &super::clean::FlowNodeJson) -> String {
    if node.kind == "dynamicBoundary" {
        return "candidate".to_string();
    }

    let incoming = flow
        .edges
        .iter()
        .filter(|edge| edge.target_node_id == node.id);

    let mut status = "verified".to_string();
    for edge in incoming {
        let edge_state = edge_status(edge.status.as_deref());
        if edge_state == "candidate" {
            return "candidate".to_string();
        }
        if edge_state == "shared" {
            status = "shared".to_string();
        }
    }
    status
}

pub(crate) fn is_boundary_flow_kind(kind: &str) -> bool {
    kind == "entry" || kind == "exit"
}
