use crate::workspace::{CodeInventory, CodeInventoryItem};

use super::super::model::{Evidence, SnapshotLink};
use super::{detail_string, is_ui_route};

pub(super) fn build_client_request_links(code: &CodeInventory) -> Vec<SnapshotLink> {
    let routes = code
        .routes
        .iter()
        .filter(|route| !is_ui_route(route))
        .filter_map(|route| route_method_path(route).map(|(method, path)| (route, method, path)))
        .collect::<Vec<_>>();
    let mut links = Vec::new();
    for request in &code.client_requests {
        if request.resolution == "excluded" {
            continue;
        }
        let Some(caller) = request.caller_id.as_deref() else {
            continue;
        };
        let Some(path) = request.path.as_deref() else {
            continue;
        };
        let path = normalize_request_path(path);
        let mut matches = routes
            .iter()
            .filter_map(|(route, route_method, route_path)| {
                let method_matches = request.method.as_deref().is_none()
                    || route_method.as_deref() == request.method.as_deref()
                    || route_method.as_deref() == Some("ANY");
                method_matches.then(|| {
                    route_path_match_score(&path, route_path)
                        .map(|score| (route, route_method, route_path, score))
                })?
            })
            .collect::<Vec<_>>();
        if matches.is_empty() {
            continue;
        }
        let best_score = matches
            .iter()
            .map(|(_, _, _, score)| *score)
            .max()
            .unwrap_or(0);
        matches.retain(|(_, _, _, score)| *score == best_score);
        let exact = request.resolution == "static-confirmed"
            && request.method.is_some()
            && matches.len() == 1
            && matches[0].1.as_deref() == request.method.as_deref();
        for (route, route_method, route_path, _) in matches {
            let mut evidence = vec![
                Evidence {
                    kind: "client-request".to_string(),
                    text: format!("{} {}", request.method.as_deref().unwrap_or("ANY"), path),
                },
                Evidence {
                    kind: "client-source".to_string(),
                    text: format!("{}:{}", request.source_file, request.line),
                },
                Evidence {
                    kind: "client-resolution".to_string(),
                    text: request.resolution.clone(),
                },
                Evidence {
                    kind: "server-route".to_string(),
                    text: format!(
                        "{} {}",
                        route_method.as_deref().unwrap_or("ANY"),
                        route_path
                    ),
                },
            ];
            evidence.extend(request.evidence.iter().cloned().map(|text| Evidence {
                kind: "client-evidence".to_string(),
                text,
            }));
            links.push(SnapshotLink {
                id: format!("client-request-link:{}->{}", request.id, route.id),
                from: format!("code:{caller}"),
                to: format!("code:{}", route.id),
                kind: "client_request".to_string(),
                label: Some("REQUESTS".to_string()),
                truth_class: if exact { "confirmed" } else { "candidate" }.to_string(),
                direction: "outbound".to_string(),
                engine_edge_type: Some("CLIENT_REQUEST".to_string()),
                evidence,
            });
        }
    }
    links
}

fn route_method_path(route: &CodeInventoryItem) -> Option<(Option<String>, String)> {
    let name = route.name.trim();
    let mut parts = name.splitn(2, char::is_whitespace);
    let first = parts.next().unwrap_or_default();
    let (method, path) = if is_http_method(first) {
        (
            Some(first.to_ascii_uppercase()),
            parts.next().unwrap_or_default().trim(),
        )
    } else {
        (method_from_route_identity(&route.id), name)
    };
    let path = if path.starts_with('/') {
        path.to_string()
    } else {
        detail_string(
            &route.detail,
            &["mountedRoutePath", "routePath", "route_path", "path"],
        )
        .unwrap_or_else(|| path.to_string())
    };
    (!path.is_empty()).then_some((method, path))
}

fn method_from_route_identity(id: &str) -> Option<String> {
    let marker = id.split("__route__").nth(1)?;
    let method = marker.split(['_', '/']).next().unwrap_or_default();
    is_http_method(method).then(|| method.to_ascii_uppercase())
}

fn is_http_method(value: &str) -> bool {
    matches!(
        value.to_ascii_uppercase().as_str(),
        "GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "HEAD" | "OPTIONS" | "ANY"
    )
}

fn normalize_request_path(path: &str) -> String {
    let path = path.split(['?', '#']).next().unwrap_or(path).trim();
    let path = path.trim_end_matches('/');
    if path.is_empty() {
        "/".to_string()
    } else if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{path}")
    }
}

fn route_path_match_score(request_path: &str, route_path: &str) -> Option<usize> {
    let normalized_request_path = normalize_request_path(request_path);
    let normalized_route_path = normalize_request_path(route_path);
    let request_segments = normalized_request_path
        .trim_matches('/')
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();
    let route_segments = normalized_route_path
        .trim_matches('/')
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();
    if request_segments.len() != route_segments.len() {
        return None;
    }
    let mut static_segments = 0usize;
    for (request, route) in request_segments.iter().zip(route_segments) {
        let dynamic = (route.starts_with(':') && route.len() > 1)
            || (route.starts_with('{') && route.ends_with('}') && route.len() > 2)
            || (route.starts_with('<') && route.ends_with('>') && route.len() > 2);
        if dynamic {
            continue;
        }
        if request != &route {
            return None;
        }
        static_segments += 1;
    }
    Some(static_segments * 10 + usize::from(static_segments == request_segments.len()) * 1_000)
}

#[cfg(test)]
mod tests {
    use super::route_path_match_score;

    #[test]
    fn matches_dynamic_route_segments_without_guessing_segment_count() {
        assert!(route_path_match_score("/owners/42", "/owners/{ownerId}").is_some());
        assert!(route_path_match_score("/owners/42/pets", "/owners/{ownerId}").is_none());
        assert!(
            route_path_match_score("/owners/list", "/owners/list")
                > route_path_match_score("/owners/list", "/owners/{ownerId}")
        );
    }
}
