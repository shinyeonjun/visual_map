use code_analysis_engine::config::AnalysisConfig;
use code_analysis_engine::facts::{
    CodeUnitKind, EntrypointKind, FactStore, ReferenceKind, ResourceKind,
};
use code_analysis_engine::languages::analyze_file;
use code_analysis_engine::model::{FileEntry, Language, ParseStatus};
use code_analysis_engine::{analyze, AnalysisRequest};
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

fn python_file() -> FileEntry {
    FileEntry {
        file_id: "file:app/api/routes.py".into(),
        relative_path: "app/api/routes.py".into(),
        language: Language::Python,
        size_bytes: 0,
        line_count: 40,
        modified_unix_ms: None,
        content_hash: None,
        is_test: false,
        parse_status: ParseStatus::NotAnalyzed,
    }
}

#[test]
fn python_import와_decorator_route를_ast_기준으로_정규화한다() {
    let source = r#"from __future__ import annotations
from app.models import User, Account as AccountModel
from .services import UserService as Service
import package.module as package_module

from fastapi import APIRouter
router = APIRouter()

class UserService:
    pass

@router.get("/users")
def list_users():
    return User

@router.post(
    "/users",
    response_model=User,
)
def create_user():
    service = Service()
    return service
"#;

    let bundle = analyze_file(&python_file(), source, &AnalysisConfig::default());

    assert_eq!(bundle.parse_status, ParseStatus::Parsed);
    assert!(bundle
        .references
        .iter()
        .any(|reference| reference.kind == ReferenceKind::Import
            && reference.target_name == "app.models.User"));
    assert!(bundle
        .references
        .iter()
        .any(|reference| reference.kind == ReferenceKind::Import
            && reference.target_name == "app.models.Account"));
    assert!(bundle
        .references
        .iter()
        .any(|reference| reference.kind == ReferenceKind::Import
            && reference.target_name == ".services.UserService"));
    assert!(bundle
        .references
        .iter()
        .any(|reference| reference.kind == ReferenceKind::Import
            && reference.target_name == "package.module"));

    assert_eq!(bundle.entrypoints.len(), 0);
    assert_eq!(bundle.decorators.len(), 2);
    assert!(bundle.decorators.iter().any(|decorator| {
        decorator.receiver.as_deref() == Some("router")
            && decorator.name == "get"
            && decorator.arguments.first().map(String::as_str) == Some("\"/users\"")
            && bundle
                .units
                .iter()
                .any(|unit| unit.id == decorator.unit_id && unit.name == "list_users")
    }));
    assert!(bundle.decorators.iter().any(|decorator| {
        decorator.receiver.as_deref() == Some("router")
            && decorator.name == "post"
            && decorator.arguments.first().map(String::as_str) == Some("\"/users\"")
            && bundle
                .units
                .iter()
                .any(|unit| unit.id == decorator.unit_id && unit.name == "create_user")
    }));
    assert!(bundle
        .call_sites
        .iter()
        .any(|call| call.callee == "APIRouter" && call.assigned_name.as_deref() == Some("router")));

    let mut store = FactStore::default();
    store.merge(bundle.clone());
    store.resolve_references();
    let service_id = store
        .units
        .iter()
        .find(|(_, unit)| unit.name == "UserService")
        .map(|(id, _)| id.as_str());
    assert!(store.references.iter().any(|reference| {
        reference.kind == ReferenceKind::Call
            && reference.target_name == "Service"
            && reference.target_unit_id.as_deref() == service_id
    }));

    assert!(bundle
        .resources
        .iter()
        .all(|resource| resource.kind != ResourceKind::Table));
}

#[test]
fn python_main_guard는_cli_진입점으로_보존된다() {
    let source = r#"
def main():
    return 0

if __name__ == "__main__":
    main()
"#;
    let bundle = analyze_file(&python_file(), source, &AnalysisConfig::default());
    assert!(bundle.entrypoints.iter().any(|entrypoint| {
        entrypoint.kind == EntrypointKind::Cli
            && entrypoint.name == "python-cli"
            && entrypoint.method.as_deref() == Some("CLI")
    }));
}

#[test]
fn python의_protocol_enum_init을_공통_유닛종류로_정규화한다() {
    let source = r#"from enum import Enum
from typing import Protocol

class UserPort(Protocol):
    def load(self): ...

class UserState(Enum):
    ACTIVE = "active"

class UserService:
    def __init__(self, repository):
        self.repository = repository
"#;
    let bundle = analyze_file(&python_file(), source, &AnalysisConfig::default());

    assert!(bundle
        .units
        .iter()
        .any(|unit| unit.name == "UserPort" && unit.kind == CodeUnitKind::Interface));
    assert!(bundle
        .units
        .iter()
        .any(|unit| unit.name == "UserState" && unit.kind == CodeUnitKind::Enum));
    assert!(bundle
        .units
        .iter()
        .any(|unit| unit.name == "__init__" && unit.kind == CodeUnitKind::Constructor));
}

#[test]
fn python_from_import이_교차_파일_구현_유닛으로_연결된다() {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("visual-map-python-import-{suffix}"));
    fs::create_dir_all(root.join("app")).expect("Python 패키지 디렉터리를 만들어야 한다");
    fs::write(root.join("app/__init__.py"), "").expect("패키지 초기화 파일을 써야 한다");
    fs::write(root.join("app/models.py"), "class User:\n    pass\n")
        .expect("모델 파일을 써야 한다");
    fs::write(root.join("app/other.py"), "class User:\n    pass\n")
        .expect("동명이인 모델 파일을 써야 한다");
    fs::write(
        root.join("app/routes.py"),
        "from app.models import User\n\ndef build():\n    return User()\n",
    )
    .expect("라우트 파일을 써야 한다");

    let result = analyze(AnalysisRequest::new(&root)).expect("분석이 성공해야 한다");
    let overview = result.overview.expect("Overview가 있어야 한다");
    let user_id = overview
        .units
        .iter()
        .find(|unit| unit.name == "User" && unit.relative_path == "app/models.py")
        .map(|unit| unit.id.clone())
        .expect("User 유닛이 있어야 한다");
    assert!(overview.static_graph.edges.iter().any(|reference| {
        reference.target_unit_id.as_deref() == Some(user_id.as_str())
            && reference.target_name == "User"
    }));

    fs::remove_dir_all(root).expect("임시 Python 프로젝트를 정리해야 한다");
}
