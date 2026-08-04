mod tests {
    use super::*;
    use crate::workspace::model::{CodeCall, CodeHandle, CodeInventorySummary};

    fn sources(entries: &[(&str, &str)]) -> BTreeMap<String, String> {
        entries
            .iter()
            .map(|(path, source)| ((*path).to_string(), (*source).to_string()))
            .collect()
    }

    fn inventory_for_route(path: &str, line: u64) -> CodeInventory {
        let handler_id = "app.routes.session.lifecycle.list_sessions".to_string();
        let route_id = "__route__GET__/#handler=list_sessions".to_string();
        let handler = CodeInventoryItem {
            id: handler_id.clone(),
            kind: "Function".to_string(),
            name: "list_sessions".to_string(),
            project: "test".to_string(),
            qualified_name: handler_id.clone(),
            engine_label: "Function".to_string(),
            file_path: Some(path.to_string()),
            line: Some(line),
            column: None,
            end_line: None,
            end_column: None,
            detail: serde_json::json!({
                "route_path": "/",
                "route_method": "GET"
            }),
        };
        let route = CodeInventoryItem {
            id: route_id.clone(),
            kind: "Route".to_string(),
            name: "/".to_string(),
            project: "test".to_string(),
            qualified_name: route_id.clone(),
            engine_label: "Route".to_string(),
            file_path: Some(path.to_string()),
            line: Some(line),
            column: None,
            end_line: None,
            end_column: None,
            detail: serde_json::json!({}),
        };
        CodeInventory {
            project: "test".to_string(),
            routes: vec![route],
            services: Vec::new(),
            files: Vec::new(),
            handlers: vec![handler],
            repositories: Vec::new(),
            functions: Vec::new(),
            classes: Vec::new(),
            modules: Vec::new(),
            unknown: Vec::new(),
            summary: CodeInventorySummary {
                routes: 1,
                handlers: 1,
                services: 0,
                repositories: 0,
                functions: 0,
                classes: 0,
                modules: 0,
                files: 0,
                unknown: 0,
            },
            architecture: None,
            evidence: None,
            calls: Vec::new(),
            handles: vec![CodeHandle {
                handler: handler_id,
                route: route_id,
            }],
            relation_gaps: Vec::new(),
            client_requests: Vec::new(),
            partial: false,
        }
    }

    fn file_item(path: &str) -> CodeInventoryItem {
        CodeInventoryItem {
            id: path.to_string(),
            kind: "File".to_string(),
            name: path.to_string(),
            project: "test".to_string(),
            qualified_name: path.to_string(),
            engine_label: "File".to_string(),
            file_path: Some(path.to_string()),
            line: None,
            column: None,
            end_line: None,
            end_column: None,
            detail: serde_json::json!({}),
        }
    }

    fn function_item(id: &str, name: &str, path: &str) -> CodeInventoryItem {
        CodeInventoryItem {
            id: id.to_string(),
            kind: "Function".to_string(),
            name: name.to_string(),
            project: "test".to_string(),
            qualified_name: id.to_string(),
            engine_label: "Function".to_string(),
            file_path: Some(path.to_string()),
            line: Some(1),
            column: None,
            end_line: Some(2),
            end_column: None,
            detail: serde_json::json!({}),
        }
    }

    #[test]
    fn resolves_nested_fastapi_router_prefixes_and_reexports() {
        let sources = sources(&[
            (
                "src/app/routes/session/lifecycle.py",
                r#"
from fastapi import APIRouter

router = APIRouter()

@router.get("/")
def list_sessions():
    pass
"#,
            ),
            (
                "src/app/routes/session/__init__.py",
                "from .lifecycle import router\n",
            ),
            (
                "src/app/routes/root.py",
                r#"
from fastapi import APIRouter
from .session import (
    router as session_router,
)

router = APIRouter(prefix="/api/v1")
router.include_router(session_router, prefix="/sessions")
"#,
            ),
            (
                "src/app/main.py",
                r#"
from fastapi import FastAPI
from .routes.root import router as root_router
app = FastAPI()
app.include_router(root_router)
"#,
            ),
        ]);
        let graph = FastApiGraph::from_sources(&sources);

        assert_eq!(
            graph.mounted_route_path("src/app/routes/session/lifecycle.py", 7, "GET", "/"),
            Some(MountedRoutePath {
                local: "/".to_string(),
                mounted: "/api/v1/sessions/".to_string(),
            })
        );
    }

    #[test]
    fn enriches_only_a_uniquely_proven_mounted_route() {
        let sources = sources(&[
            (
                "app/routes/session/lifecycle.py",
                r#"
from fastapi import APIRouter
router = APIRouter()

@router.get("/")
def list_sessions():
    pass
"#,
            ),
            (
                "app/routes/session/router.py",
                r#"
from fastapi import APIRouter
from app.routes.session.lifecycle import router as lifecycle_router
router = APIRouter(prefix="/api/v1/sessions")
router.include_router(lifecycle_router)
"#,
            ),
            (
                "app/route_groups/control.py",
                r#"
from fastapi import FastAPI
from app.routes.session.router import router as session_router
def include_control_routes(app: FastAPI):
    app.include_router(session_router)
"#,
            ),
        ]);
        let mut inventory = inventory_for_route("app/routes/session/lifecycle.py", 6);

        enrich_fastapi_route_paths_from_sources(&sources, &mut inventory);

        let route = &inventory.routes[0];
        assert_eq!(route.name, "/api/v1/sessions/");
        assert_eq!(route.detail["localRoutePath"], "/");
        assert_eq!(route.detail["mountedRoutePath"], "/api/v1/sessions/");
        assert_eq!(route.detail["routePathSource"], "fastapi-static-mount");
    }

    #[test]
    fn confirms_only_unambiguous_unshadowed_fastapi_import_calls() {
        let route_path = "backend/app/api/routes/login.py";
        let sources = sources(&[(
            route_path,
            r#"
from fastapi import APIRouter
from app import crud, shadowed
from app.core import security
router = APIRouter()

@router.post("/login")
def list_sessions(shadowed):
    user = crud.authenticate()
    shadowed.run()
    return security.create_access_token(user.id)
"#,
        )]);
        let graph = FastApiGraph::from_sources(&sources);
        let mut inventory = inventory_for_route(route_path, 8);
        let caller = inventory.handlers[0].id.clone();
        inventory.functions = vec![
            function_item(
                "backend.app.crud.authenticate",
                "authenticate",
                "backend/app/crud.py",
            ),
            function_item(
                "backend.app.core.security.create_access_token",
                "create_access_token",
                "backend/app/core/security.py",
            ),
            function_item("backend.app.shadowed.run", "run", "backend/app/shadowed.py"),
        ];
        inventory.calls = vec![
            CodeCall {
                from: caller.clone(),
                to: "backend.app.crud.authenticate".to_string(),
                confidence: Some(38),
                strategy: Some("unique_name".to_string()),
                expression: Some("crud.authenticate".to_string()),
                path: None,
                range: Vec::new(),
            },
            CodeCall {
                from: caller.clone(),
                to: "backend.app.core.security.create_access_token".to_string(),
                confidence: Some(38),
                strategy: Some("unique_name".to_string()),
                expression: Some("security.create_access_token".to_string()),
                path: None,
                range: Vec::new(),
            },
            CodeCall {
                from: caller.clone(),
                to: "backend.app.shadowed.run".to_string(),
                confidence: Some(38),
                strategy: Some("unique_name".to_string()),
                expression: Some("shadowed.run".to_string()),
                path: None,
                range: Vec::new(),
            },
            CodeCall {
                from: caller,
                to: "backend.app.crud.authenticate".to_string(),
                confidence: Some(38),
                strategy: Some("unique_name".to_string()),
                expression: Some("crud.users.authenticate".to_string()),
                path: None,
                range: Vec::new(),
            },
        ];

        enrich_fastapi_import_calls(&graph, &mut inventory);

        assert_eq!(
            inventory
                .calls
                .iter()
                .map(|call| (
                    call.expression.as_deref().unwrap(),
                    call.confidence,
                    call.strategy.as_deref().unwrap()
                ))
                .collect::<Vec<_>>(),
            vec![
                ("crud.authenticate", Some(95), "python_static_import"),
                (
                    "security.create_access_token",
                    Some(95),
                    "python_static_import"
                ),
                ("shadowed.run", Some(38), "unique_name"),
                ("crud.users.authenticate", Some(38), "unique_name"),
            ]
        );
    }

    #[test]
    fn preserves_an_empty_decorator_path_after_engine_root_normalization() {
        let sources = sources(&[
            (
                "app/routes/sessions.py",
                r#"
from fastapi import APIRouter
router = APIRouter(prefix="/api/v1/sessions")

@router.post("")
def create_session():
    pass
"#,
            ),
            (
                "app/main.py",
                r#"
from fastapi import FastAPI
from app.routes.sessions import router as session_router
app = FastAPI()
app.include_router(session_router)
"#,
            ),
        ]);
        let mut inventory = inventory_for_route("app/routes/sessions.py", 6);
        inventory.handlers[0].detail["route_method"] = serde_json::json!("POST");

        enrich_fastapi_route_paths_from_sources(&sources, &mut inventory);

        let route = &inventory.routes[0];
        assert_eq!(route.name, "/api/v1/sessions");
        assert_eq!(route.detail["localRoutePath"], "");
    }

    #[test]
    fn reads_indexed_python_files_within_the_repository_boundary() {
        let root = std::env::temp_dir().join(format!(
            "backend-map-fastapi-routes-{}-{}",
            std::process::id(),
            crate::workspace::store::timestamp()
        ));
        let lifecycle = root.join("app/routes/session/lifecycle.py");
        let router = root.join("app/routes/session/router.py");
        let control = root.join("app/route_groups/control.py");
        for parent in [
            lifecycle.parent().unwrap(),
            router.parent().unwrap(),
            control.parent().unwrap(),
        ] {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(
            &lifecycle,
            "from fastapi import APIRouter\nrouter = APIRouter()\n\n@router.get(\"/\")\ndef list_sessions():\n    pass\n",
        )
        .unwrap();
        fs::write(
            &router,
            "from fastapi import APIRouter\nfrom app.routes.session.lifecycle import router as lifecycle_router\nrouter = APIRouter(prefix=\"/api/v1/sessions\")\nrouter.include_router(lifecycle_router)\n",
        )
        .unwrap();
        fs::write(
            &control,
            "from fastapi import FastAPI\nfrom app.routes.session.router import router as session_router\ndef include_control_routes(app: FastAPI):\n    app.include_router(session_router)\n",
        )
        .unwrap();

        let lifecycle_path = lifecycle.display().to_string();
        let mut inventory = inventory_for_route(&lifecycle_path, 5);
        inventory.files = vec![
            file_item(&lifecycle_path),
            file_item("app/routes/session/router.py"),
            file_item("app/route_groups/control.py"),
        ];

        enrich_fastapi_evidence(root.to_str().unwrap(), &mut inventory);

        assert_eq!(inventory.routes[0].name, "/api/v1/sessions/");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn does_not_present_an_unmounted_router_prefix_as_a_full_route() {
        let sources = sources(&[(
            "app/routes.py",
            r#"
from fastapi import APIRouter
router = APIRouter(prefix="/internal")
@router.get("/items")
def items():
    pass
"#,
        )]);
        let graph = FastApiGraph::from_sources(&sources);

        assert_eq!(
            graph.mounted_route_path("app/routes.py", 5, "GET", "/items"),
            None
        );
    }

    #[test]
    fn leaves_multiply_mounted_routes_unresolved() {
        let sources = sources(&[
            (
                "app/child.py",
                r#"
from fastapi import APIRouter
router = APIRouter()
@router.get("/items")
def items():
    pass
"#,
            ),
            (
                "app/parents.py",
                r#"
from fastapi import APIRouter
from app.child import router as child_router
v1 = APIRouter(prefix="/v1")
v2 = APIRouter(prefix="/v2")
v1.include_router(child_router)
v2.include_router(child_router)
"#,
            ),
            (
                "app/main.py",
                r#"
from fastapi import FastAPI
from app.parents import v1, v2
app = FastAPI()
app.include_router(v1)
app.include_router(v2)
"#,
            ),
        ]);
        let graph = FastApiGraph::from_sources(&sources);

        assert_eq!(
            graph.mounted_route_path("app/child.py", 5, "GET", "/items"),
            None
        );
    }

    #[test]
    fn leaves_dynamic_prefixes_unresolved() {
        let sources = sources(&[
            (
                "app/child.py",
                r#"
from fastapi import APIRouter
router = APIRouter()
@router.get("/items")
def items():
    pass
"#,
            ),
            (
                "app/root.py",
                r#"
from fastapi import APIRouter
from app.child import router as child_router
router = APIRouter(prefix=API_PREFIX)
router.include_router(child_router)
"#,
            ),
            (
                "app/main.py",
                r#"
from fastapi import FastAPI
from app.root import router
app = FastAPI()
app.include_router(router)
"#,
            ),
        ]);
        let graph = FastApiGraph::from_sources(&sources);

        assert_eq!(
            graph.mounted_route_path("app/child.py", 5, "GET", "/items"),
            None
        );
    }

    #[test]
    fn joins_paths_without_losing_a_route_trailing_slash() {
        assert_eq!(join_url_path("/api/v1/sessions", "/"), "/api/v1/sessions/");
        assert_eq!(
            join_url_path("/api/v1/sessions/", "/{session_id}"),
            "/api/v1/sessions/{session_id}"
        );
    }
}
