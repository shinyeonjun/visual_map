use super::build_map;
use crate::models::SemanticDomain;
use std::collections::HashSet;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

struct CleanFixture {
    root: PathBuf,
}

impl CleanFixture {
    fn new(suffix: &str) -> Self {
        let root = std::env::temp_dir().join(format!("visual-map-clean-{suffix}"));
        fs::create_dir_all(root.join("domains/parts")).expect("fixture dir");
        fs::create_dir_all(root.join("features/parts")).expect("fixture dir");
        fs::create_dir_all(root.join("relations/parts")).expect("fixture dir");
        fs::create_dir_all(root.join("metadata")).expect("fixture dir");
        Self { root }
    }

    fn write(&self, relative: &str, contents: &str) {
        let path = self.root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("fixture parent dir");
        }
        let mut file = fs::File::create(path).expect("fixture file");
        file.write_all(contents.as_bytes()).expect("fixture write");
    }
}

impl Drop for CleanFixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn projects_domains_features_and_relations_from_clean_bundle() {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
        .to_string();
    let fixture = CleanFixture::new(&suffix);
    fixture.write(
        "manifest.json",
        r#"{
          "metadata": { "coveragePath": "metadata/coverage.json" },
          "datasets": [
            { "name": "domains", "parts": [{ "path": "domains/parts/part-0000.json" }] },
            { "name": "features", "parts": [{ "path": "features/parts/part-0000.json" }] },
            { "name": "relations", "parts": [{ "path": "relations/parts/part-0000.json" }] }
          ]
        }"#,
    );
    fixture.write(
        "domains/parts/part-0000.json",
        r#"[{
          "id": "domain-a",
          "candidateKey": "order",
          "label": "Order",
          "status": "confirmed",
          "confidenceScore": 82,
          "unitIds": ["unit-1"],
          "entrypointIds": ["entry-1"],
          "featureIds": ["feature-1"],
          "evidence": [{ "kind": "unit", "value": "OrderService" }]
        }]"#,
    );
    fixture.write(
        "features/parts/part-0000.json",
        r#"[{
          "id": "feature-1",
          "label": "Create Order",
          "kind": "endpoint",
          "entrypointIds": ["entry-1"]
        }]"#,
    );
    fixture.write(
        "relations/parts/part-0000.json",
        r#"[{
          "sourceDomainId": "domain-a",
          "targetDomainId": "domain-b"
        }]"#,
    );
    fixture.write(
        "metadata/coverage.json",
        r#"{
          "totalFiles": 3,
          "totalUnits": 4,
          "totalFeatures": 1,
          "totalExecutionFlows": 2,
          "totalResources": 1
        }"#,
    );

    let semantic = vec![SemanticDomain {
        domain_id: "domain-a".into(),
        source_domain_ids: vec!["domain-a".into()],
        name: "\u{C8FC}\u{BB38}".into(),
        summary: Some("\u{C8FC}\u{BB38} \u{CC98}\u{B9AC}".into()),
    }];
    let (domains, stats) = build_map(&fixture.root, &semantic).expect("map build failed");
    assert_eq!(domains.len(), 1);
    assert_eq!(domains[0].name, "\u{C8FC}\u{BB38}");
    assert_eq!(domains[0].summary, "\u{C8FC}\u{BB38} \u{CC98}\u{B9AC}");
    assert_eq!(domains[0].status, "verified");
    assert_eq!(domains[0].confidence, 82);
    assert_eq!(domains[0].units, 1);
    assert_eq!(domains[0].features, 1);
    assert_eq!(domains[0].entrypoints, 1);
    assert_eq!(domains[0].dependencies, vec!["domain-b".to_string()]);
    assert_eq!(domains[0].feature_items[0].name, "Create Order");
    assert_eq!(stats.files, 3);
}

#[test]
fn builds_map_from_static_labels_when_semantic_is_empty() {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
        .to_string();
    let fixture = CleanFixture::new(&format!("empty-semantic-{suffix}"));
    fixture.write(
        "manifest.json",
        r#"{
          "metadata": { "coveragePath": "metadata/coverage.json" },
          "datasets": [
            { "name": "domains", "parts": [{ "path": "domains/parts/part-0000.json" }] },
            { "name": "features", "parts": [{ "path": "features/parts/part-0000.json" }] },
            { "name": "relations", "parts": [{ "path": "relations/parts/part-0000.json" }] }
          ]
        }"#,
    );
    fixture.write(
        "domains/parts/part-0000.json",
        r#"[{
          "id": "domain-a",
          "candidateKey": "order",
          "label": "Order",
          "status": "confirmed",
          "confidenceScore": 82,
          "unitIds": ["unit-1"],
          "entrypointIds": ["entry-1"],
          "featureIds": ["feature-1"],
          "evidence": []
        }]"#,
    );
    fixture.write(
        "features/parts/part-0000.json",
        r#"[{
          "id": "feature-1",
          "label": "Create Order",
          "kind": "endpoint",
          "entrypointIds": ["entry-1"]
        }]"#,
    );
    fixture.write("relations/parts/part-0000.json", r#"[]"#);
    fixture.write(
        "metadata/coverage.json",
        r#"{
          "totalFiles": 1,
          "totalUnits": 1,
          "totalFeatures": 1,
          "totalExecutionFlows": 0,
          "totalResources": 0
        }"#,
    );

    let (domains, _) = build_map(&fixture.root, &[]).expect("map build failed");
    assert_eq!(domains.len(), 1);
    assert_eq!(domains[0].name, "Order");
    assert_eq!(domains[0].feature_items[0].name, "Create Order");
}

#[test]
fn projects_linear_flow_as_graph_nodes_and_edges() {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
        .to_string();
    let fixture = CleanFixture::new(&format!("flows-{suffix}"));
    fixture.write(
        "manifest.json",
        r#"{
          "metadata": { "coveragePath": "metadata/coverage.json" },
          "datasets": [
            { "name": "domains", "parts": [{ "path": "domains/parts/part-0000.json" }] },
            { "name": "features", "parts": [{ "path": "features/parts/part-0000.json" }] },
            { "name": "relations", "parts": [{ "path": "relations/parts/part-0000.json" }] },
            { "name": "flows", "parts": [{ "path": "flows/parts/part-0000.json" }] },
            { "name": "units", "parts": [{ "path": "units/parts/part-0000.json" }] }
          ]
        }"#,
    );
    fixture.write(
        "domains/parts/part-0000.json",
        r#"[{
          "id": "domain-a",
          "candidateKey": "order",
          "label": "Order",
          "status": "confirmed",
          "confidenceScore": 82,
          "unitIds": ["unit-1"],
          "entrypointIds": ["entry-1"],
          "featureIds": ["feature-1"],
          "evidence": []
        }]"#,
    );
    fixture.write(
        "features/parts/part-0000.json",
        r#"[{
          "id": "feature-1",
          "label": "Create Order",
          "kind": "endpoint",
          "entrypointIds": ["entry-1"],
          "flowIds": ["flow-1"]
        }]"#,
    );
    fixture.write("relations/parts/part-0000.json", r#"[]"#);
    fixture.write(
        "flows/parts/part-0000.json",
        r#"[{
          "id": "flow-1",
          "ownerUnitId": "unit-1",
          "entryNodeId": "node-entry",
          "exitNodeId": "node-exit",
          "nodes": [
            { "id": "node-entry", "kind": "entry", "label": "entry" },
            { "id": "node-call", "kind": "call", "label": "saveOrder()" },
            { "id": "node-return", "kind": "return", "label": "return" },
            { "id": "node-exit", "kind": "exit", "label": "exit" }
          ],
          "edges": [
            { "sourceNodeId": "node-entry", "targetNodeId": "node-call" },
            { "sourceNodeId": "node-call", "targetNodeId": "node-return" },
            { "sourceNodeId": "node-return", "targetNodeId": "node-exit" }
          ],
          "dynamicBoundaryIds": []
        }]"#,
    );
    fixture.write(
        "units/parts/part-0000.json",
        r#"[{
          "id": "unit-1",
          "name": "createOrder",
          "qualifiedName": "OrderService.createOrder"
        }]"#,
    );
    fixture.write(
        "metadata/coverage.json",
        r#"{
          "totalFiles": 1,
          "totalUnits": 1,
          "totalFeatures": 1,
          "totalExecutionFlows": 1,
          "totalResources": 0
        }"#,
    );

    let (domains, _) = build_map(&fixture.root, &[]).expect("map build failed");
    let flows = &domains[0].feature_items[0].flows;
    assert_eq!(flows.len(), 1);
    assert_eq!(flows[0].owner, "OrderService.createOrder");
    assert_eq!(flows[0].nodes.len(), 2);
    assert_eq!(flows[0].nodes[0].label, "saveOrder()");
    assert_eq!(flows[0].nodes[0].kind, "call");
    assert_eq!(flows[0].nodes[1].kind, "return");
    assert_eq!(flows[0].edges.len(), 2);
    assert!(flows[0].edges.iter().all(|edge| edge.kind == "sequential"));
    assert_eq!(flows[0].status, "verified");
    assert_eq!(flows[0].nodes[0].status, "verified");
}

#[test]
fn marks_candidate_features_and_flow_nodes() {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
        .to_string();
    let fixture = CleanFixture::new(&format!("candidate-{suffix}"));
    fixture.write(
        "manifest.json",
        r#"{
          "metadata": { "coveragePath": "metadata/coverage.json" },
          "datasets": [
            { "name": "domains", "parts": [{ "path": "domains/parts/part-0000.json" }] },
            { "name": "features", "parts": [{ "path": "features/parts/part-0000.json" }] },
            { "name": "relations", "parts": [{ "path": "relations/parts/part-0000.json" }] },
            { "name": "flows", "parts": [{ "path": "flows/parts/part-0000.json" }] },
            { "name": "units", "parts": [{ "path": "units/parts/part-0000.json" }] }
          ]
        }"#,
    );
    fixture.write(
        "domains/parts/part-0000.json",
        r#"[{
          "id": "domain-a",
          "candidateKey": "order",
          "label": "Order",
          "status": "candidate",
          "confidenceScore": 42,
          "unitIds": ["unit-1"],
          "entrypointIds": ["entry-1"],
          "featureIds": ["feature-1"],
          "evidence": []
        }]"#,
    );
    fixture.write(
        "features/parts/part-0000.json",
        r#"[{
          "id": "feature-1",
          "label": "Create Order",
          "kind": "endpoint",
          "status": "candidate",
          "entrypointIds": ["entry-1"],
          "flowIds": ["flow-1"]
        }]"#,
    );
    fixture.write("relations/parts/part-0000.json", r#"[]"#);
    fixture.write(
        "flows/parts/part-0000.json",
        r#"[{
          "id": "flow-1",
          "ownerUnitId": "unit-1",
          "entryNodeId": "node-entry",
          "exitNodeId": "node-exit",
          "nodes": [
            { "id": "node-entry", "kind": "entry", "label": "entry" },
            { "id": "node-call", "kind": "dynamicBoundary", "label": "dispatch()" },
            { "id": "node-exit", "kind": "exit", "label": "exit" }
          ],
          "edges": [
            { "sourceNodeId": "node-entry", "targetNodeId": "node-call", "status": "dynamic" },
            { "sourceNodeId": "node-call", "targetNodeId": "node-exit" }
          ],
          "dynamicBoundaryIds": ["dyn-1"]
        }]"#,
    );
    fixture.write(
        "units/parts/part-0000.json",
        r#"[{
          "id": "unit-1",
          "name": "createOrder",
          "qualifiedName": "OrderService.createOrder"
        }]"#,
    );
    fixture.write(
        "metadata/coverage.json",
        r#"{
          "totalFiles": 1,
          "totalUnits": 1,
          "totalFeatures": 1,
          "totalExecutionFlows": 1,
          "totalResources": 0
        }"#,
    );

    let (domains, _) = build_map(&fixture.root, &[]).expect("map build failed");
    assert_eq!(domains[0].status, "candidate");
    assert_eq!(domains[0].feature_items[0].status, "candidate");
    assert_eq!(domains[0].feature_items[0].flows[0].status, "candidate");
    assert_eq!(domains[0].feature_items[0].flows[0].nodes[0].status, "candidate");
}

#[test]
fn merges_canonical_domains_into_single_card() {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
        .to_string();
    let fixture = CleanFixture::new(&format!("canonical-{suffix}"));
    fixture.write(
        "manifest.json",
        r#"{
          "metadata": { "coveragePath": "metadata/coverage.json" },
          "datasets": [
            { "name": "domains", "parts": [{ "path": "domains/parts/part-0000.json" }] },
            { "name": "features", "parts": [{ "path": "features/parts/part-0000.json" }] },
            { "name": "relations", "parts": [{ "path": "relations/parts/part-0000.json" }] }
          ]
        }"#,
    );
    fixture.write(
        "domains/parts/part-0000.json",
        r#"[
          {
            "id": "domain-a",
            "candidateKey": "auth",
            "label": "Auth",
            "status": "confirmed",
            "confidenceScore": 70,
            "unitIds": ["unit-1"],
            "entrypointIds": ["entry-1"],
            "featureIds": ["feature-1"],
            "evidence": []
          },
          {
            "id": "domain-b",
            "candidateKey": "session",
            "label": "Session",
            "status": "confirmed",
            "confidenceScore": 80,
            "unitIds": ["unit-2"],
            "entrypointIds": ["entry-2"],
            "featureIds": ["feature-2"],
            "evidence": []
          }
        ]"#,
    );
    fixture.write(
        "features/parts/part-0000.json",
        r#"[
          {
            "id": "feature-1",
            "label": "Login",
            "kind": "endpoint",
            "entrypointIds": ["entry-1"]
          },
          {
            "id": "feature-2",
            "label": "Session",
            "kind": "endpoint",
            "entrypointIds": ["entry-2"]
          }
        ]"#,
    );
    fixture.write(
        "relations/parts/part-0000.json",
        r#"[{
          "sourceDomainId": "domain-a",
          "targetDomainId": "domain-b"
        }]"#,
    );
    fixture.write(
        "metadata/coverage.json",
        r#"{
          "totalFiles": 2,
          "totalUnits": 2,
          "totalFeatures": 2,
          "totalExecutionFlows": 0,
          "totalResources": 0
        }"#,
    );

    let semantic = vec![SemanticDomain {
        domain_id: "domain-b".into(),
        source_domain_ids: vec!["domain-a".into(), "domain-b".into()],
        name: "\u{C778}\u{C99D}\u{ACFC} \u{C138}\u{C158}".into(),
        summary: Some("\u{C0AC}\u{C6A9}\u{C790} \u{C778}\u{C99D}".into()),
    }];
    let (domains, _) = build_map(&fixture.root, &semantic).expect("map build failed");
    assert_eq!(domains.len(), 1);
    assert_eq!(domains[0].domain_id, "domain-b");
    assert_eq!(domains[0].name, "\u{C778}\u{C99D}\u{ACFC} \u{C138}\u{C158}");
    assert_eq!(domains[0].units, 2);
    assert_eq!(domains[0].features, 2);
    assert!(domains[0].dependencies.is_empty());
}

fn write_branching_flow_fixture(fixture: &CleanFixture) {
    fixture.write(
        "manifest.json",
        r#"{
          "metadata": { "coveragePath": "metadata/coverage.json" },
          "datasets": [
            { "name": "domains", "parts": [{ "path": "domains/parts/part-0000.json" }] },
            { "name": "features", "parts": [{ "path": "features/parts/part-0000.json" }] },
            { "name": "relations", "parts": [{ "path": "relations/parts/part-0000.json" }] },
            { "name": "flows", "parts": [{ "path": "flows/parts/part-0000.json" }] },
            { "name": "units", "parts": [{ "path": "units/parts/part-0000.json" }] }
          ]
        }"#,
    );
    fixture.write(
        "domains/parts/part-0000.json",
        r#"[{
          "id": "domain-a",
          "candidateKey": "payment",
          "label": "Payment",
          "status": "confirmed",
          "confidenceScore": 90,
          "unitIds": ["unit-1"],
          "entrypointIds": ["entry-1"],
          "featureIds": ["feature-1"],
          "evidence": []
        }]"#,
    );
    fixture.write(
        "features/parts/part-0000.json",
        r#"[{
          "id": "feature-1",
          "label": "Charge",
          "kind": "endpoint",
          "entrypointIds": ["entry-1"],
          "flowIds": ["flow-1"]
        }]"#,
    );
    fixture.write("relations/parts/part-0000.json", r#"[]"#);
    fixture.write(
        "units/parts/part-0000.json",
        r#"[{
          "id": "unit-1",
          "name": "charge",
          "qualifiedName": "PaymentService.charge"
        }]"#,
    );
    fixture.write(
        "metadata/coverage.json",
        r#"{
          "totalFiles": 1,
          "totalUnits": 1,
          "totalFeatures": 1,
          "totalExecutionFlows": 1,
          "totalResources": 0
        }"#,
    );
}

#[test]
fn flow_projection_preserves_if_else_edge_kinds() {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
        .to_string();
    let fixture = CleanFixture::new(&format!("flow-branch-{suffix}"));
    write_branching_flow_fixture(&fixture);
    fixture.write(
        "flows/parts/part-0000.json",
        r#"[{
          "id": "flow-1",
          "ownerUnitId": "unit-1",
          "entryNodeId": "node-entry",
          "exitNodeId": "node-exit",
          "nodes": [
            { "id": "node-entry", "kind": "entry", "label": "entry" },
            { "id": "node-condition", "kind": "condition", "label": "isPaid()" },
            { "id": "node-paid", "kind": "call", "label": "processPaid()" },
            { "id": "node-failed", "kind": "call", "label": "processFailed()" },
            { "id": "node-return", "kind": "return", "label": "return" },
            { "id": "node-exit", "kind": "exit", "label": "exit" }
          ],
          "edges": [
            { "sourceNodeId": "node-entry", "targetNodeId": "node-condition", "kind": "sequential" },
            { "sourceNodeId": "node-condition", "targetNodeId": "node-paid", "kind": "trueBranch", "label": "paid" },
            { "sourceNodeId": "node-condition", "targetNodeId": "node-failed", "kind": "falseBranch" },
            { "sourceNodeId": "node-paid", "targetNodeId": "node-return", "kind": "sequential" },
            { "sourceNodeId": "node-failed", "targetNodeId": "node-return", "kind": "sequential" },
            { "sourceNodeId": "node-return", "targetNodeId": "node-exit", "kind": "return" }
          ],
          "dynamicBoundaryIds": []
        }]"#,
    );

    let (domains, _) = build_map(&fixture.root, &[]).expect("map build failed");
    let flow = &domains[0].feature_items[0].flows[0];
    let edge_kinds: HashSet<_> = flow.edges.iter().map(|edge| edge.kind.as_str()).collect();

    assert!(edge_kinds.contains("trueBranch"));
    assert!(edge_kinds.contains("falseBranch"));
    assert!(!flow.edges.iter().any(|edge| {
        edge.source_node_id == "node-paid" && edge.target_node_id == "node-failed"
    }));
    assert_eq!(
        flow.edges
            .iter()
            .find(|edge| edge.kind == "trueBranch")
            .and_then(|edge| edge.label.as_deref()),
        Some("paid")
    );
}

#[test]
fn flow_projection_preserves_loop_edge_kinds() {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
        .to_string();
    let fixture = CleanFixture::new(&format!("flow-loop-{suffix}"));
    write_branching_flow_fixture(&fixture);
    fixture.write(
        "flows/parts/part-0000.json",
        r#"[{
          "id": "flow-1",
          "ownerUnitId": "unit-1",
          "entryNodeId": "node-entry",
          "exitNodeId": "node-exit",
          "nodes": [
            { "id": "node-entry", "kind": "entry", "label": "entry" },
            { "id": "node-loop", "kind": "loop", "label": "while items" },
            { "id": "node-body", "kind": "call", "label": "handle()" },
            { "id": "node-after", "kind": "return", "label": "return" },
            { "id": "node-exit", "kind": "exit", "label": "exit" }
          ],
          "edges": [
            { "sourceNodeId": "node-entry", "targetNodeId": "node-loop", "kind": "sequential" },
            { "sourceNodeId": "node-loop", "targetNodeId": "node-body", "kind": "loopBody" },
            { "sourceNodeId": "node-body", "targetNodeId": "node-loop", "kind": "loopBack" },
            { "sourceNodeId": "node-loop", "targetNodeId": "node-after", "kind": "sequential" },
            { "sourceNodeId": "node-after", "targetNodeId": "node-exit", "kind": "return" }
          ],
          "dynamicBoundaryIds": []
        }]"#,
    );

    let (domains, _) = build_map(&fixture.root, &[]).expect("map build failed");
    let flow = &domains[0].feature_items[0].flows[0];
    assert!(flow.edges.iter().any(|edge| edge.kind == "loopBody"));
    assert!(flow.edges.iter().any(|edge| edge.kind == "loopBack"));
}

#[test]
fn flow_projection_preserves_exception_edge_kind() {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
        .to_string();
    let fixture = CleanFixture::new(&format!("flow-exception-{suffix}"));
    write_branching_flow_fixture(&fixture);
    fixture.write(
        "flows/parts/part-0000.json",
        r#"[{
          "id": "flow-1",
          "ownerUnitId": "unit-1",
          "entryNodeId": "node-entry",
          "exitNodeId": "node-exit",
          "nodes": [
            { "id": "node-entry", "kind": "entry", "label": "entry" },
            { "id": "node-try", "kind": "call", "label": "save()" },
            { "id": "node-catch", "kind": "catch", "label": "catch" },
            { "id": "node-return", "kind": "return", "label": "return" },
            { "id": "node-exit", "kind": "exit", "label": "exit" }
          ],
          "edges": [
            { "sourceNodeId": "node-entry", "targetNodeId": "node-try", "kind": "sequential" },
            { "sourceNodeId": "node-try", "targetNodeId": "node-catch", "kind": "exception" },
            { "sourceNodeId": "node-catch", "targetNodeId": "node-return", "kind": "sequential" },
            { "sourceNodeId": "node-return", "targetNodeId": "node-exit", "kind": "return" }
          ],
          "dynamicBoundaryIds": []
        }]"#,
    );

    let (domains, _) = build_map(&fixture.root, &[]).expect("map build failed");
    let flow = &domains[0].feature_items[0].flows[0];
    assert!(flow.edges.iter().any(|edge| edge.kind == "exception"));
}

#[test]
fn flow_projection_preserves_early_return_and_control_flow_nodes() {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
        .to_string();
    let fixture = CleanFixture::new(&format!("flow-control-{suffix}"));
    write_branching_flow_fixture(&fixture);
    fixture.write(
        "flows/parts/part-0000.json",
        r#"[{
          "id": "flow-1",
          "ownerUnitId": "unit-1",
          "entryNodeId": "node-entry",
          "exitNodeId": "node-exit",
          "nodes": [
            { "id": "node-entry", "kind": "entry", "label": "entry" },
            { "id": "node-guard", "kind": "condition", "label": "if invalid" },
            { "id": "node-return", "kind": "return", "label": "return early" },
            { "id": "node-loop", "kind": "loop", "label": "for item" },
            { "id": "node-break", "kind": "break", "label": "break" },
            { "id": "node-continue", "kind": "continue", "label": "continue" },
            { "id": "node-exit", "kind": "exit", "label": "exit" }
          ],
          "edges": [
            { "sourceNodeId": "node-entry", "targetNodeId": "node-guard", "kind": "sequential" },
            { "sourceNodeId": "node-guard", "targetNodeId": "node-return", "kind": "trueBranch" },
            { "sourceNodeId": "node-guard", "targetNodeId": "node-loop", "kind": "falseBranch" },
            { "sourceNodeId": "node-loop", "targetNodeId": "node-continue", "kind": "loopBody" },
            { "sourceNodeId": "node-loop", "targetNodeId": "node-break", "kind": "falseBranch" },
            { "sourceNodeId": "node-return", "targetNodeId": "node-exit", "kind": "return" }
          ],
          "dynamicBoundaryIds": []
        }]"#,
    );

    let (domains, _) = build_map(&fixture.root, &[]).expect("map build failed");
    let kinds: HashSet<_> = domains[0].feature_items[0].flows[0]
        .nodes
        .iter()
        .map(|node| node.kind.as_str())
        .collect();
    assert!(kinds.contains("return"));
    assert!(kinds.contains("break"));
    assert!(kinds.contains("continue"));
}
