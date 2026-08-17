use code_analysis_engine::{analyze, AnalysisRequest};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn temporary_project(name: &str) -> PathBuf {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("visual-map-{name}-{suffix}"));
    fs::create_dir_all(&path).expect("임시 프로젝트 디렉터리를 만들어야 한다");
    path
}

#[test]
fn polyglot_http_match는_폴더명이_아니라_계약_키로_도메인을_만든다() {
    let root = temporary_project("polyglot-http");
    fs::create_dir_all(root.join("web")).expect("web 디렉터리를 만들어야 한다");
    fs::create_dir_all(root.join("svc")).expect("svc 디렉터리를 만들어야 한다");
    fs::write(
        root.join("web/client.ts"),
        r#"
export async function loadInvoices() {
  return fetch("/billing/invoices");
}
"#,
    )
    .expect("클라이언트 fetch를 써야 한다");
    fs::write(
        root.join("svc/api.py"),
        r#"from fastapi import FastAPI

app = FastAPI()

@app.post("/billing/invoices")
def create_invoice():
    return {}
"#,
    )
    .expect("서버 route를 써야 한다");

    let overview = analyze(AnalysisRequest::new(&root))
        .expect("분석이 성공해야 한다")
        .overview
        .expect("Overview가 생성되어야 한다");
    let keys: Vec<_> = overview
        .domains
        .iter()
        .map(|domain| domain.key.as_str())
        .collect();

    assert!(
        keys.iter().any(|key| *key == "billing"),
        "billing 계약 도메인이 없어: {keys:?}"
    );
    assert!(
        !keys.iter().any(|key| *key == "web" || *key == "svc"),
        "폴더명이 도메인 키가 되면 안 된다: {keys:?}"
    );
    assert!(
        overview
            .relations
            .iter()
            .any(|relation| relation.kind == "http")
            || overview
                .domains
                .iter()
                .any(|domain| domain.key == "billing" && !domain.feature_ids.is_empty()),
        "billing 도메인이 fetch와 route를 묶거나 http 관계가 있어야 한다"
    );

    fs::remove_dir_all(root).expect("임시 프로젝트를 정리해야 한다");
}

#[test]
fn event_only_overlay는_비즈니스_도메인_카드를_만들지_않는다() {
    let root = temporary_project("event-overlay");
    fs::create_dir_all(root.join("src/overlay")).expect("overlay 디렉터리를 만들어야 한다");
    fs::write(
        root.join("src/overlay/ui.ts"),
        r#"
export function bind() {
  window.addEventListener("click", () => {});
}
"#,
    )
    .expect("이벤트 전용 overlay를 써야 한다");

    let overview = analyze(AnalysisRequest::new(&root))
        .expect("분석이 성공해야 한다")
        .overview
        .expect("Overview가 생성되어야 한다");

    assert!(
        overview.domains.is_empty(),
        "HTTP 계약 없는 overlay는 도메인 카드가 없어야 한다: {:?}",
        overview
            .domains
            .iter()
            .map(|domain| domain.key.as_str())
            .collect::<Vec<_>>()
    );

    fs::remove_dir_all(root).expect("임시 프로젝트를 정리해야 한다");
}

#[test]
fn cli_script_excluded는_scripts를_도메인으로_올리지_않는다() {
    let root = temporary_project("cli-script");
    fs::create_dir_all(root.join("scripts")).expect("scripts 디렉터리를 만들어야 한다");
    fs::create_dir_all(root.join("src/api")).expect("api 디렉터리를 만들어야 한다");
    fs::write(
        root.join("package.json"),
        r#"{"dependencies":{"express":"5.0.0"}}"#,
    )
    .expect("package manifest를 써야 한다");
    fs::write(
        root.join("scripts/seed.py"),
        r#"
import argparse

def main():
    parser = argparse.ArgumentParser()
    parser.parse_args()
"#,
    )
    .expect("스크립트 CLI를 써야 한다");
    fs::write(
        root.join("src/api/users.ts"),
        r#"
import express from "express";
const router = express();
router.get("/users", listUsers);
export function listUsers() {
  return [];
}
"#,
    )
    .expect("users route를 써야 한다");

    let overview = analyze(AnalysisRequest::new(&root))
        .expect("분석이 성공해야 한다")
        .overview
        .expect("Overview가 생성되어야 한다");
    let keys: Vec<_> = overview
        .domains
        .iter()
        .map(|domain| domain.key.as_str())
        .collect();

    assert!(
        keys.iter().any(|key| *key == "users"),
        "users 계약 도메인이 있어야 한다: {keys:?}"
    );
    assert!(
        !keys.iter().any(|key| *key == "scripts"),
        "scripts 폴더가 도메인 키가 되면 안 된다: {keys:?}"
    );

    fs::remove_dir_all(root).expect("임시 프로젝트를 정리해야 한다");
}

#[test]
fn 로그인과_일정_기능은_자신이_속한_도메인_묶음에_붙는다() {
    let root = temporary_project("login-schedule-domains");
    fs::create_dir_all(root.join("web")).expect("web 디렉터리를 만들어야 한다");
    fs::create_dir_all(root.join("server/auth")).expect("auth 디렉터리를 만들어야 한다");
    fs::create_dir_all(root.join("server/schedules")).expect("schedules 디렉터리를 만들어야 한다");
    fs::write(
        root.join("package.json"),
        r#"{"dependencies":{"express":"5.0.0"}}"#,
    )
    .expect("package manifest를 써야 한다");
    fs::write(
        root.join("web/auth.ts"),
        r#"
export async function login(email: string, password: string) {
  return fetch("/auth/login", {
    method: "POST",
    body: JSON.stringify({ email, password }),
  });
}
"#,
    )
    .expect("로그인 클라이언트를 써야 한다");
    fs::write(
        root.join("web/schedules.ts"),
        r#"
export async function createSchedule(title: string) {
  return fetch("/schedules", {
    method: "POST",
    body: JSON.stringify({ title }),
  });
}
"#,
    )
    .expect("일정 등록 클라이언트를 써야 한다");
    fs::write(
        root.join("server/auth/routes.ts"),
        r#"
import express from "express";
const router = express();
router.post("/auth/login", login);
export function login(request: Request) {
  return { ok: true };
}
"#,
    )
    .expect("로그인 route를 써야 한다");
    fs::write(
        root.join("server/schedules/routes.ts"),
        r#"
import express from "express";
const router = express();
router.post("/schedules", createSchedule);
export function createSchedule(request: Request) {
  return { id: "schedule-1" };
}
"#,
    )
    .expect("일정 등록 route를 써야 한다");

    let overview = analyze(AnalysisRequest::new(&root))
        .expect("분석이 성공해야 한다")
        .overview
        .expect("Overview가 생성되어야 한다");

    let login_feature = overview
        .features
        .iter()
        .find(|feature| {
            feature.entrypoint_ids.iter().any(|entrypoint_id| {
                overview.entrypoints.iter().any(|entrypoint| {
                    entrypoint.id == *entrypoint_id
                        && entrypoint.path.as_deref() == Some("/auth/login")
                })
            })
        })
        .expect("POST /auth/login 기능이 있어야 한다");
    let schedule_feature = overview
        .features
        .iter()
        .find(|feature| {
            feature.entrypoint_ids.iter().any(|entrypoint_id| {
                overview.entrypoints.iter().any(|entrypoint| {
                    entrypoint.id == *entrypoint_id
                        && entrypoint.path.as_deref() == Some("/schedules")
                })
            })
        })
        .expect("POST /schedules 기능이 있어야 한다");

    assert!(
        !login_feature.domain_ids.is_empty(),
        "로그인 기능은 도메인에 붙어야 한다: {:?}",
        login_feature.domain_ids
    );
    assert!(
        !schedule_feature.domain_ids.is_empty(),
        "일정 등록 기능은 도메인에 붙어야 한다: {:?}",
        schedule_feature.domain_ids
    );

    for feature in [login_feature, schedule_feature] {
        let assigned: std::collections::HashSet<_> = feature.domain_ids.iter().collect();
        let expected: std::collections::HashSet<_> = overview
            .domains
            .iter()
            .filter(|domain| {
                feature
                    .entrypoint_ids
                    .iter()
                    .any(|entrypoint_id| domain.entrypoint_ids.contains(entrypoint_id))
            })
            .map(|domain| &domain.id)
            .collect();
        assert!(
            !expected.is_empty(),
            "기능 진입점이 속한 도메인이 있어야 한다: {}",
            feature.label
        );
        assert!(
            expected.iter().all(|domain_id| assigned.contains(domain_id)),
            "기능은 자신의 진입점이 들어 있는 도메인에 붙어야 한다: {} {:?} expected={expected:?}",
            feature.label,
            feature.domain_ids
        );
    }

    let login_domains: std::collections::HashSet<_> = login_feature.domain_ids.iter().collect();
    let schedule_domains: std::collections::HashSet<_> =
        schedule_feature.domain_ids.iter().collect();
    assert!(
        login_domains.is_disjoint(&schedule_domains),
        "로그인과 일정 등록은 서로 다른 도메인에 있어야 한다: login={login_domains:?} schedule={schedule_domains:?}"
    );

    fs::remove_dir_all(root).expect("임시 프로젝트를 정리해야 한다");
}

#[test]
fn 서로_다른_계약_도메인은_개수_상한_때문에_한_카드로_합쳐지지_않는다() {
    let root = temporary_project("distinct-contract-domains");
    fs::create_dir_all(root.join("server")).expect("server 디렉터리를 만들어야 한다");
    fs::write(
        root.join("package.json"),
        r#"{"dependencies":{"express":"5.0.0"}}"#,
    )
    .expect("package manifest를 써야 한다");
    let routes = [
        ("/auth/login", "login"),
        ("/sessions", "createSession"),
        ("/billing/invoices", "createInvoice"),
        ("/users", "listUsers"),
        ("/reports/summary", "loadReportSummary"),
    ];
    for (path, handler) in routes {
        fs::write(
            root.join(format!("server/{handler}.ts")),
            format!(
                r#"
import express from "express";
const router = express();
router.post("{path}", {handler});
export function {handler}(request: Request) {{
  return {{ ok: true }};
}}
"#
            ),
        )
        .expect("route 파일을 써야 한다");
    }

    let overview = analyze(AnalysisRequest::new(&root))
        .expect("분석이 성공해야 한다")
        .overview
        .expect("Overview가 생성되어야 한다");
    let domain_keys: std::collections::HashSet<_> = overview
        .domains
        .iter()
        .map(|domain| domain.key.as_str())
        .collect();

    for expected in ["auth", "sessions", "billing", "users", "reports"] {
        assert!(
            domain_keys.contains(expected),
            "계약 접두 도메인이 있어야 한다: {expected} in {domain_keys:?}"
        );
    }

    for domain in &overview.domains {
        let entrypoint_paths: Vec<_> = domain
            .entrypoint_ids
            .iter()
            .filter_map(|entrypoint_id| {
                overview
                    .entrypoints
                    .iter()
                    .find(|entrypoint| &entrypoint.id == entrypoint_id)
                    .and_then(|entrypoint| entrypoint.path.clone())
            })
            .collect();
        let domain_keys_in_card: std::collections::HashSet<_> = entrypoint_paths
            .iter()
            .filter_map(|path| path.trim_start_matches('/').split('/').next())
            .collect();
        assert!(
            domain_keys_in_card.len() <= 1,
            "한 도메인 카드에 서로 다른 계약 접두가 섞이면 안 된다: key={} paths={entrypoint_paths:?}",
            domain.key
        );
    }

    fs::remove_dir_all(root).expect("임시 프로젝트를 정리해야 한다");
}

#[test]
fn 짧은_경로와_긴_경로는_같은_기능으로_묶인다() {
    let root = temporary_project("short-long-contract");
    fs::create_dir_all(root.join("server")).expect("server 디렉터리를 만들어야 한다");
    fs::write(
        root.join("package.json"),
        r#"{"dependencies":{"express":"5.0.0"}}"#,
    )
    .expect("package manifest를 써야 한다");
    fs::write(
        root.join("server/health.ts"),
        r#"
import express from "express";
const router = express();
router.get("/health", healthShort);
export function healthShort() {
  return { ok: true };
}
"#,
    )
    .expect("짧은 health route를 써야 한다");
    fs::write(
        root.join("server/api.ts"),
        r#"
import express from "express";
const router = express();
router.get("/api/v1/health", healthLong);
export function healthLong() {
  return { ok: true };
}
"#,
    )
    .expect("긴 health route를 써야 한다");

    let overview = analyze(AnalysisRequest::new(&root))
        .expect("분석이 성공해야 한다")
        .overview
        .expect("Overview가 생성되어야 한다");
    let health_features: Vec<_> = overview
        .features
        .iter()
        .filter(|feature| feature.label.contains("/health"))
        .collect();

    assert_eq!(
        health_features.len(),
        1,
        "짧은 경로와 api 접두 경로는 기능 하나여야 한다: {:?}",
        overview
            .features
            .iter()
            .map(|feature| feature.label.as_str())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        health_features[0].entrypoint_ids.len(),
        2,
        "두 route 진입점이 같은 기능에 남아야 한다"
    );

    fs::remove_dir_all(root).expect("임시 프로젝트를 정리해야 한다");
}

#[test]
fn 기능_실행_길은_진입점에서_이어진_flow만_포함한다() {
    let root = temporary_project("feature-scoped-flows");
    fs::create_dir_all(root.join("server")).expect("server 디렉터리를 만들어야 한다");
    fs::write(
        root.join("package.json"),
        r#"{"dependencies":{"express":"5.0.0"}}"#,
    )
    .expect("package manifest를 써야 한다");
    fs::write(
        root.join("server/routes.ts"),
        r#"
import express from "express";
const router = express();
router.get("/users", listUsers);
router.get("/admin/users", adminUsers);
export function listUsers() {
  return loadUsers();
}
export function adminUsers() {
  return purgeUsers();
}
export function loadUsers() {
  return [];
}
export function purgeUsers() {
  return [];
}
"#,
    )
    .expect("route 파일을 써야 한다");

    let overview = analyze(AnalysisRequest::new(&root))
        .expect("분석이 성공해야 한다")
        .overview
        .expect("Overview가 생성되어야 한다");
    let flow_owner_by_id: std::collections::HashMap<_, _> = overview
        .execution_flows
        .flows
        .iter()
        .map(|flow| (flow.id.as_str(), flow.owner_unit_id.as_str()))
        .collect();
    let unit_name_by_id: std::collections::HashMap<_, _> = overview
        .units
        .iter()
        .map(|unit| (unit.id.as_str(), unit.name.as_str()))
        .collect();

    let list_feature = overview
        .features
        .iter()
        .find(|feature| {
            feature.entrypoint_ids.iter().any(|entrypoint_id| {
                overview.entrypoints.iter().any(|entrypoint| {
                    entrypoint.id == *entrypoint_id
                        && entrypoint.path.as_deref() == Some("/users")
                })
            })
        })
        .expect("GET /users 기능이 있어야 한다");
    let admin_feature = overview
        .features
        .iter()
        .find(|feature| {
            feature.entrypoint_ids.iter().any(|entrypoint_id| {
                overview.entrypoints.iter().any(|entrypoint| {
                    entrypoint.id == *entrypoint_id
                        && entrypoint.path.as_deref() == Some("/admin/users")
                })
            })
        })
        .expect("GET /admin/users 기능이 있어야 한다");

    let flow_names = |feature: &code_analysis_engine::views::overview::model::FeatureGroup| {
        feature
            .flow_ids
            .iter()
            .filter_map(|flow_id| flow_owner_by_id.get(flow_id.as_str()).copied())
            .filter_map(|unit_id| unit_name_by_id.get(unit_id).copied())
            .collect::<std::collections::HashSet<_>>()
    };

    let list_flow_names = flow_names(list_feature);
    let admin_flow_names = flow_names(admin_feature);

    assert!(
        list_flow_names.contains("listUsers"),
        "listUsers 기능은 자신의 handler flow를 포함해야 한다: {list_flow_names:?}"
    );
    assert!(
        !list_flow_names.contains("adminUsers") && !list_flow_names.contains("purgeUsers"),
        "listUsers 기능에 admin 경로 flow가 섞이면 안 된다: {list_flow_names:?}"
    );
    assert!(
        admin_flow_names.contains("adminUsers"),
        "adminUsers 기능은 자신의 handler flow를 포함해야 한다: {admin_flow_names:?}"
    );
    assert!(
        !admin_flow_names.contains("listUsers") && !admin_flow_names.contains("loadUsers"),
        "adminUsers 기능에 listUsers 경로 flow가 섞이면 안 된다: {admin_flow_names:?}"
    );

    fs::remove_dir_all(root).expect("임시 프로젝트를 정리해야 한다");
}

#[test]
fn 같은_http_계약은_production과_archived를_기능_하나로_묶는다() {
    let root = temporary_project("contract-collapse");
    fs::create_dir_all(root.join("server")).expect("server 디렉터리를 만들어야 한다");
    fs::create_dir_all(root.join("legacy")).expect("legacy 디렉터리를 만들어야 한다");
    let route = r#"from fastapi import FastAPI

app = FastAPI()

@app.get("/health")
def health():
    return {}
"#;
    fs::write(root.join("server/app.py"), route).expect("현행 health route를 써야 한다");
    fs::write(root.join("legacy/app.py"), route).expect("레거시 health route를 써야 한다");

    let overview = analyze(AnalysisRequest::new(&root))
        .expect("분석이 성공해야 한다")
        .overview
        .expect("Overview가 생성되어야 한다");
    let health: Vec<_> = overview
        .features
        .iter()
        .filter(|feature| feature.label.contains("/health"))
        .collect();

    assert_eq!(
        health.len(),
        1,
        "같은 GET /health 계약은 기능 하나여야 한다: {:?}",
        overview
            .features
            .iter()
            .map(|feature| feature.label.as_str())
            .collect::<Vec<_>>()
    );
    assert_eq!(health[0].entrypoint_ids.len(), 2);

    fs::remove_dir_all(root).expect("임시 프로젝트를 정리해야 한다");
}

#[test]
fn 같은_핸들러의_마운트_경로는_기능과_도메인_하나로_묶인다() {
    let root = temporary_project("mounted-router-contract");
    fs::create_dir_all(root.join("server")).expect("server 디렉터리를 만들어야 한다");
    fs::write(
        root.join("server/app.py"),
        r#"from fastapi import APIRouter, FastAPI

router = APIRouter()

@router.post("/{session_id}/start")
def start_session(session_id: str):
    return {"ok": True}

app = FastAPI()
app.include_router(router, prefix="/api/v1/sessions")
"#,
    )
    .expect("마운트 route를 써야 한다");

    let overview = analyze(AnalysisRequest::new(&root))
        .expect("분석이 성공해야 한다")
        .overview
        .expect("Overview가 생성되어야 한다");

    let start_features: Vec<_> = overview
        .features
        .iter()
        .filter(|feature| {
            feature.entrypoint_ids.iter().any(|entrypoint_id| {
                overview.entrypoints.iter().any(|entrypoint| {
                    entrypoint.id == *entrypoint_id
                        && entrypoint
                            .path
                            .as_deref()
                            .is_some_and(|path| path.contains("/start"))
                })
            })
        })
        .collect();

    assert_eq!(
        start_features.len(),
        1,
        "마운트 전후 start route는 기능 하나여야 한다: {:?}",
        overview
            .features
            .iter()
            .map(|feature| feature.label.as_str())
            .collect::<Vec<_>>()
    );
    assert!(
        start_features[0].entrypoint_ids.len() >= 2,
        "두 route 진입점이 같은 기능에 남아야 한다"
    );
    assert!(
        overview
            .domains
            .iter()
            .any(|domain| domain.key == "sessions"),
        "start route는 sessions 도메인에 속해야 한다: {:?}",
        overview
            .domains
            .iter()
            .map(|domain| domain.key.as_str())
            .collect::<Vec<_>>()
    );

    fs::remove_dir_all(root).expect("임시 프로젝트를 정리해야 한다");
}

#[test]
fn tauri_command는_도메인_카드를_만든다() {
    let root = temporary_project("tauri-command-domain");
    fs::write(
        root.join("desktop.rs"),
        r#"
use tauri::command;

#[tauri::command]
fn analyze_project() {}

#[tauri::command]
fn list_projects() {}
"#,
    )
    .expect("Tauri command fixture를 써야 한다");

    let overview = analyze(AnalysisRequest::new(&root))
        .expect("분석이 성공해야 한다")
        .overview
        .expect("Overview가 생성되어야 한다");
    let keys: Vec<_> = overview
        .domains
        .iter()
        .map(|domain| domain.key.as_str())
        .collect();

    assert!(
        !overview.domains.is_empty(),
        "Tauri command만 있는 프로젝트도 도메인 카드가 있어야 한다: {keys:?}"
    );
    assert!(
        overview.entrypoints.iter().any(|entrypoint| {
            entrypoint.kind == code_analysis_engine::facts::EntrypointKind::Rpc
                && entrypoint.name == "analyze_project"
        }),
        "analyze_project command가 RPC 진입점이어야 한다"
    );
    assert!(
        overview
            .features
            .iter()
            .any(|feature| !feature.entrypoint_ids.is_empty()),
        "Tauri command가 기능으로 올라가야 한다"
    );

    fs::remove_dir_all(root).expect("임시 프로젝트를 정리해야 한다");
}
