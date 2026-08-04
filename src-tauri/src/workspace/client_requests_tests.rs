mod tests {
    use super::*;
    use crate::workspace::model::{CodeInventoryItem, CodeInventorySummary};

    fn inventory(path: &str) -> CodeInventory {
        let item = CodeInventoryItem {
            id: "code:login".to_string(),
            kind: "function".to_string(),
            name: "login".to_string(),
            project: String::new(),
            qualified_name: "login".to_string(),
            engine_label: "Function".to_string(),
            file_path: Some(path.to_string()),
            line: Some(1),
            column: None,
            end_line: Some(20),
            end_column: None,
            detail: serde_json::json!({}),
        };
        CodeInventory {
            project: "test".to_string(),
            routes: Vec::new(),
            services: Vec::new(),
            files: Vec::new(),
            handlers: Vec::new(),
            repositories: Vec::new(),
            functions: vec![item],
            classes: Vec::new(),
            modules: Vec::new(),
            unknown: Vec::new(),
            summary: CodeInventorySummary {
                routes: 0,
                handlers: 0,
                services: 0,
                repositories: 0,
                functions: 1,
                classes: 0,
                modules: 0,
                files: 0,
                unknown: 0,
            },
            architecture: None,
            evidence: None,
            calls: Vec::new(),
            handles: Vec::new(),
            relation_gaps: Vec::new(),
            client_requests: Vec::new(),
            partial: false,
        }
    }

    #[test]
    fn extracts_static_requests_across_common_language_clients() {
        let root =
            std::env::temp_dir().join(format!("visual-map-client-request-{}", std::process::id()));
        let _ = fs::create_dir_all(&root);
        let source = r#"const API = "/api";
axios.post(API + "/token");
const example = "axios.post('/fake-string')";
const multiline = "axios.get(\n  '/fake-multiline')";
// axios.get("/fake")
requests.get("/health");
RestTemplate.getForObject("/owners", Owner.class);
client.GetAsync("/orders");
http.NewRequest("POST", "/transfers", body);
reqwest::get("/rust");
Http::post("/php");
Faraday.get("/ruby");
http.post("/dart");
"#;
        fs::write(root.join("main.ts"), source).unwrap();
        let inventory = inventory("main.ts");
        let requests = extract_client_requests(root.to_str().unwrap(), &inventory)
            .unwrap()
            .requests;
        assert!(requests
            .iter()
            .any(|item| item.method.as_deref() == Some("POST")
                && item.path.as_deref() == Some("/api/token")));
        assert_eq!(
            requests
                .iter()
                .filter(|item| item.path.as_deref() == Some("/api/token"))
                .count(),
            1
        );
        assert_eq!(
            requests
                .iter()
                .find(|item| item.path.as_deref() == Some("/api/token"))
                .map(|item| item.line),
            Some(2)
        );
        assert!(requests
            .iter()
            .any(|item| item.client == "requests" && item.path.as_deref() == Some("/health")));
        assert!(requests
            .iter()
            .any(|item| item.client == "go-http" && item.path.as_deref() == Some("/transfers")));
        assert!(!requests
            .iter()
            .any(|item| item.path.as_deref() == Some("/fake")));
        assert!(!requests
            .iter()
            .any(|item| item.path.as_deref() == Some("/fake-string")));
        assert!(!requests
            .iter()
            .any(|item| item.path.as_deref() == Some("/fake-multiline")));
        fs::create_dir_all(root.join("tests")).unwrap();
        fs::write(
            root.join("tests/client.test.ts"),
            "axios.post(\"/test-only\")\n",
        )
        .unwrap();
        let requests = extract_client_requests(root.to_str().unwrap(), &inventory)
            .unwrap()
            .requests;
        let test_only = requests
            .iter()
            .find(|item| item.path.as_deref() == Some("/test-only"))
            .expect("test-only request");
        assert_eq!(test_only.resolution, "excluded");
        assert!(test_only
            .evidence
            .iter()
            .any(|evidence| evidence == "excluded:test-only"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn normalizes_http_and_https_urls_without_panicking() {
        assert_eq!(
            normalize_url_path("http://localhost:8000/api/items"),
            Some("/api/items".to_string())
        );
        assert_eq!(
            normalize_url_path("https://example.com/api/items?limit=1"),
            Some("/api/items".to_string())
        );
        assert_eq!(normalize_url_path("http://localhost:8000"), None);
    }

    #[test]
    fn keeps_one_common_request_contract_across_active_language_extensions() {
        let root = std::env::temp_dir().join(format!(
            "visual-map-client-request-languages-{}",
            std::process::id()
        ));
        let _ = fs::create_dir_all(&root);
        let fixtures = [
            (
                "main.c",
                "curl_easy_setopt(curl, CURLOPT_URL, \"/c\"); curl_easy_setopt(curl, CURLOPT_POST, 1L);",
                "POST",
                "/c",
                "candidate",
            ),
            ("main.cpp", "cpr::Get(\"/cpp\");", "GET", "/cpp", "static-confirmed"),
            ("main.cs", "client.GetAsync(\"/csharp\");", "GET", "/csharp", "candidate"),
            ("main.dart", "http.get(\"/dart\");", "GET", "/dart", "candidate"),
            (
                "main.go",
                "http.NewRequest(\"POST\", \"/go\", nil);",
                "POST",
                "/go",
                "static-confirmed",
            ),
            (
                "Main.java",
                "RestTemplate.getForObject(\"/java\", String.class);",
                "GET",
                "/java",
                "static-confirmed",
            ),
            ("main.js", "fetch(\"/javascript\");", "GET", "/javascript", "static-confirmed"),
            ("main.php", "Http::post(\"/php\");", "POST", "/php", "static-confirmed"),
            (
                "main.py",
                "requests.get(\"/python\");",
                "GET",
                "/python",
                "static-confirmed",
            ),
            ("main.rb", "Faraday.post(\"/ruby\");", "POST", "/ruby", "static-confirmed"),
            ("main.rs", "reqwest::get(\"/rust\");", "GET", "/rust", "static-confirmed"),
            (
                "main.ts",
                "axios.post(\"/typescript\");",
                "POST",
                "/typescript",
                "static-confirmed",
            ),
        ];
        for (file, source, _, _, _) in fixtures {
            fs::write(root.join(file), source).unwrap();
        }

        let requests = extract_client_requests(root.to_str().unwrap(), &inventory("main.js"))
            .unwrap()
            .requests;
        assert_eq!(requests.len(), fixtures.len());
        for (_, _, method, path, resolution) in fixtures {
            let request = requests
                .iter()
                .find(|request| request.path.as_deref() == Some(path))
                .expect("language fixture request");
            assert_eq!(request.method.as_deref(), Some(method));
            assert_eq!(request.resolution, resolution);
        }
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn leaves_dynamic_urls_unknown_instead_of_guessing() {
        let root = std::env::temp_dir().join(format!(
            "visual-map-client-request-dynamic-{}",
            std::process::id()
        ));
        let _ = fs::create_dir_all(&root);
        fs::write(
            root.join("main.py"),
            "requests.get(os.getenv(\"API_URL\") + path)\n",
        )
        .unwrap();
        let requests = extract_client_requests(root.to_str().unwrap(), &inventory("main.py"))
            .unwrap()
            .requests;
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].resolution, "unknown");
        assert_eq!(requests[0].path, None);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn folds_templates_and_axios_instance_base_urls() {
        let root = std::env::temp_dir().join(format!(
            "visual-map-client-request-template-{}",
            std::process::id()
        ));
        let _ = fs::create_dir_all(&root);
        fs::write(
            root.join("main.ts"),
            "const API = '/api';\nconst path = `${API}/orders`;\nfetch(path);\nconst api = axios.create({ baseURL: API });\napi.get('/users');\napi.post(`/orders`);\n",
        )
        .unwrap();
        let requests = extract_client_requests(root.to_str().unwrap(), &inventory("main.ts"))
            .unwrap()
            .requests;
        assert!(requests
            .iter()
            .any(|request| request.path.as_deref() == Some("/api/orders")));
        assert!(requests
            .iter()
            .any(|request| request.path.as_deref() == Some("/api/users")
                && request.method.as_deref() == Some("GET")));
        assert!(requests
            .iter()
            .any(|request| request.path.as_deref() == Some("/api/orders")
                && request.method.as_deref() == Some("POST")));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn reports_source_files_skipped_by_the_scan_budget() {
        let root = std::env::temp_dir().join(format!(
            "visual-map-client-request-budget-{}",
            std::process::id()
        ));
        let _ = fs::create_dir_all(&root);
        fs::write(
            root.join("large.ts"),
            vec![b'x'; (MAX_FILE_BYTES + 1) as usize],
        )
        .unwrap();

        let report =
            extract_client_requests(root.to_str().unwrap(), &inventory("large.ts")).unwrap();
        assert!(report.requests.is_empty());
        assert_eq!(report.skipped_files, 1);
        assert_eq!(report.skipped_bytes, MAX_FILE_BYTES + 1);
        assert!(report.truncated);
        let _ = fs::remove_dir_all(root);
    }
}
