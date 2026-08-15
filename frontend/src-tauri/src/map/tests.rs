    use super::build_map;
    use crate::models::SemanticDomain;
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
            fs::create_dir_all(root.join("domains/parts")).expect("fixture 디렉터리를 만들어야 한다");
            fs::create_dir_all(root.join("features/parts")).expect("fixture 디렉터리를 만들어야 한다");
            fs::create_dir_all(root.join("relations/parts")).expect("fixture 디렉터리를 만들어야 한다");
            fs::create_dir_all(root.join("metadata")).expect("fixture 디렉터리를 만들어야 한다");
            Self { root }
        }

        fn write(&self, relative: &str, contents: &str) {
            let path = self.root.join(relative);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).expect("fixture 부모 디렉터리를 만들어야 한다");
            }
            let mut file = fs::File::create(path).expect("fixture를 써야 한다");
            file.write_all(contents.as_bytes())
                .expect("fixture를 써야 한다");
        }
    }

    impl Drop for CleanFixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn clean_bundle에서_도메인_관계와_기능을_투영한다() {
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
            name: "주문".into(),
            summary: Some("주문 처리".into()),
        }];
        let (domains, stats) =
            build_map(&fixture.root, &semantic).expect("지도를 만들어야 한다");
        assert_eq!(domains.len(), 1);
        assert_eq!(domains[0].name, "주문");
        assert_eq!(domains[0].summary, "주문 처리");
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
    fn semantic이_비어_있어도_정적_라벨로_지도를_만든다() {
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

        let (domains, _) = build_map(&fixture.root, &[]).expect("지도를 만들어야 한다");
        assert_eq!(domains.len(), 1);
        assert_eq!(domains[0].name, "Order");
        assert_eq!(domains[0].feature_items[0].name, "Create Order");
    }

    #[test]
    fn 기능에_연결된_flow_단계를_순서대로_투영한다() {
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

        let (domains, _) = build_map(&fixture.root, &[]).expect("지도를 만들어야 한다");
        let flows = &domains[0].feature_items[0].flows;
        assert_eq!(flows.len(), 1);
        assert_eq!(flows[0].owner, "OrderService.createOrder");
        assert_eq!(flows[0].steps.len(), 2);
        assert_eq!(flows[0].steps[0].label, "saveOrder()");
        assert_eq!(flows[0].steps[0].kind, "call");
        assert_eq!(flows[0].steps[1].kind, "return");
        assert_eq!(flows[0].status, "verified");
        assert_eq!(flows[0].steps[0].status, "verified");
    }

    #[test]
    fn 후보_기능과_미해결_flow_단계는_candidate로_표시한다() {
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

        let (domains, _) = build_map(&fixture.root, &[]).expect("지도를 만들어야 한다");
        assert_eq!(domains[0].status, "candidate");
        assert_eq!(domains[0].feature_items[0].status, "candidate");
        assert_eq!(domains[0].feature_items[0].flows[0].status, "candidate");
        assert_eq!(domains[0].feature_items[0].flows[0].steps[0].status, "candidate");
    }

    #[test]
    fn canonical_도메인은_정적_카드를_하나로_합친다() {
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
            name: "인증과 세션".into(),
            summary: Some("사용자 인증".into()),
        }];
        let (domains, _) = build_map(&fixture.root, &semantic).expect("지도를 만들어야 한다");
        assert_eq!(domains.len(), 1);
        assert_eq!(domains[0].domain_id, "domain-b");
        assert_eq!(domains[0].name, "인증과 세션");
        assert_eq!(domains[0].units, 2);
        assert_eq!(domains[0].features, 2);
        assert!(domains[0].dependencies.is_empty());
    }
