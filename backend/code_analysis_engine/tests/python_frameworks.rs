use code_analysis_engine::facts::EntrypointKind;
use code_analysis_engine::{analyze, AnalysisRequest};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn temporary_project() -> PathBuf {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("visual-map-python-frameworks-{suffix}"));
    fs::create_dir_all(&path).expect("임시 프로젝트를 만들어야 한다");
    path
}

#[test]
fn fastapi_adapter가_prefix_websocket_orm을_공통_facts로_변환한다() {
    let root = temporary_project();
    fs::write(
        root.join("app.py"),
        r#"from fastapi import APIRouter, FastAPI
from sqlmodel import Session, SQLModel

app = FastAPI()
router = APIRouter(prefix="/users")

class User(SQLModel, table=True):
    id: int

@router.get("/")
def list_users():
    session.get(User, 1)

@router.websocket("/stream")
async def stream_users():
    return None

app.include_router(router, prefix="/api")
"#,
    )
    .expect("Python fixture를 써야 한다");

    let result = analyze(AnalysisRequest::new(&root)).expect("분석이 성공해야 한다");
    let overview = result.overview.expect("Overview가 생성되어야 한다");
    let units = overview
        .units
        .iter()
        .map(|unit| (unit.id.as_str(), unit.name.as_str()))
        .collect::<std::collections::HashMap<_, _>>();

    assert!(overview.entrypoints.iter().any(|entrypoint| {
        entrypoint.path.as_deref() == Some("/api/users/")
            && entrypoint.kind == EntrypointKind::Http
            && units.get(entrypoint.unit_id.as_str()) == Some(&"list_users")
    }));
    assert!(overview.entrypoints.iter().any(|entrypoint| {
        entrypoint.path.as_deref() == Some("/api/users/stream")
            && entrypoint.kind == EntrypointKind::WebSocket
            && units.get(entrypoint.unit_id.as_str()) == Some(&"stream_users")
    }));
    assert!(overview.resources.iter().any(|resource| {
        resource.kind == code_analysis_engine::facts::ResourceKind::Table && resource.name == "user"
    }));
    assert!(overview.resources.iter().any(|resource| {
        resource.name == "user" && resource.mode == code_analysis_engine::facts::AccessMode::Read
    }));

    fs::remove_dir_all(root).expect("임시 프로젝트를 정리해야 한다");
}
