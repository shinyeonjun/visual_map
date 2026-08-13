use code_analysis_engine::facts::EntrypointKind;
use code_analysis_engine::{analyze, AnalysisRequest};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn temporary_project(name: &str) -> PathBuf {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("visual-map-python-accuracy-{name}-{suffix}"));
    fs::create_dir_all(&path).expect("임시 프로젝트를 만들어야 한다");
    path
}

#[test]
fn fastapi는_교차파일_router_prefix와_keyword_methods를_보존한다() {
    let root = temporary_project("fastapi");
    fs::write(
        root.join("routers.py"),
        r#"from fastapi import APIRouter

router = APIRouter(prefix="/users")

@router.api_route(path="/", methods=["GET", "POST"])
def list_users():
    pass

@router.websocket(path="/stream")
async def stream_users():
    pass
"#,
    )
    .expect("FastAPI router fixture를 써야 한다");
    fs::write(
        root.join("app.py"),
        r#"from fastapi import FastAPI
from routers import router

app = FastAPI()
app.include_router(router, prefix="/api")
"#,
    )
    .expect("FastAPI app fixture를 써야 한다");

    let overview = analyze(AnalysisRequest::new(&root))
        .expect("분석이 성공해야 한다")
        .overview
        .expect("Overview가 생성되어야 한다");
    assert!(overview.entrypoints.iter().any(|entry| {
        entry.path.as_deref() == Some("/api/users/")
            && entry.method.as_deref() == Some("GET,POST")
            && entry.kind == EntrypointKind::Http
    }));
    assert!(overview.entrypoints.iter().any(|entry| {
        entry.path.as_deref() == Some("/api/users/stream")
            && entry.kind == EntrypointKind::WebSocket
    }));

    fs::remove_dir_all(root).expect("임시 프로젝트를 정리해야 한다");
}

#[test]
fn fastapi는_import_alias를_사용한_router도_인식한다() {
    let root = temporary_project("fastapi-alias");
    fs::write(
        root.join("app.py"),
        r#"from fastapi import APIRouter as Router, FastAPI

app = FastAPI()
router = Router(prefix="/alias")

@router.get("/users")
def users():
    pass
"#,
    )
    .expect("FastAPI alias fixture를 써야 한다");

    let overview = analyze(AnalysisRequest::new(&root))
        .expect("분석이 성공해야 한다")
        .overview
        .expect("Overview가 생성되어야 한다");
    assert!(overview.entrypoints.iter().any(|entry| {
        entry.path.as_deref() == Some("/alias/users")
            && entry.framework_id.as_deref() == Some("python.fastapi")
    }));

    fs::remove_dir_all(root).expect("임시 프로젝트를 정리해야 한다");
}

#[test]
fn 동적인_fastapi_prefix는_경로를_확정하지_않고_경계를_표시한다() {
    let root = temporary_project("fastapi-dynamic-prefix");
    fs::write(
        root.join("app.py"),
        r#"from fastapi import APIRouter, FastAPI

app = FastAPI()
router = APIRouter(prefix=ROUTER_PREFIX)

@router.get("/users")
def users():
    pass

app.include_router(router, prefix=settings.API_V1_STR)
"#,
    )
    .expect("동적 FastAPI fixture를 써야 한다");

    let overview = analyze(AnalysisRequest::new(&root))
        .expect("분석이 성공해야 한다")
        .overview
        .expect("Overview가 생성되어야 한다");
    let entry = overview
        .entrypoints
        .iter()
        .find(|entry| !entry.unit_id.is_empty())
        .expect("동적 route도 진입점으로 보존되어야 한다");
    assert!(entry.path.is_none());
    assert!(entry.name.contains("<dynamic>"));

    fs::remove_dir_all(root).expect("임시 프로젝트를 정리해야 한다");
}

#[test]
fn flask는_교차파일_blueprint_prefix와_rule_keyword를_보존한다() {
    let root = temporary_project("flask");
    fs::write(
        root.join("blueprints.py"),
        r#"from flask import Blueprint

users = Blueprint("users", __name__, url_prefix="/users")

@users.route(rule="/", methods=["GET", "POST"])
def list_users():
    pass
"#,
    )
    .expect("Flask blueprint fixture를 써야 한다");
    fs::write(
        root.join("app.py"),
        r#"from flask import Flask
from blueprints import users

app = Flask(__name__)
app.register_blueprint(users, url_prefix="/api")
"#,
    )
    .expect("Flask app fixture를 써야 한다");

    let overview = analyze(AnalysisRequest::new(&root))
        .expect("분석이 성공해야 한다")
        .overview
        .expect("Overview가 생성되어야 한다");
    assert!(overview.entrypoints.iter().any(|entry| {
        entry.path.as_deref() == Some("/api/users/")
            && entry.method.as_deref() == Some("GET,POST")
            && entry.framework_id.as_deref() == Some("python.flask")
    }));

    fs::remove_dir_all(root).expect("임시 프로젝트를 정리해야 한다");
}

#[test]
fn starlette는_route와_websocketroute_생성자를_진입점으로_변환한다() {
    let root = temporary_project("starlette");
    fs::write(
        root.join("routes.py"),
        r#"from starlette.routing import Route, WebSocketRoute

async def home(request):
    pass

async def socket(websocket):
    pass

routes = [
    Route("/home", home),
    WebSocketRoute("/events", socket),
]
"#,
    )
    .expect("Starlette route fixture를 써야 한다");

    let overview = analyze(AnalysisRequest::new(&root))
        .expect("분석이 성공해야 한다")
        .overview
        .expect("Overview가 생성되어야 한다");
    assert!(overview.entrypoints.iter().any(|entry| {
        entry.path.as_deref() == Some("/home")
            && entry.kind == EntrypointKind::Http
            && overview
                .units
                .iter()
                .any(|unit| unit.id == entry.unit_id && unit.name == "home")
    }));
    assert!(overview.entrypoints.iter().any(|entry| {
        entry.path.as_deref() == Some("/events")
            && entry.kind == EntrypointKind::WebSocket
            && overview
                .units
                .iter()
                .any(|unit| unit.id == entry.unit_id && unit.name == "socket")
    }));

    fs::remove_dir_all(root).expect("임시 프로젝트를 정리해야 한다");
}

#[test]
fn django와_drf는_urlconf와_router_register를_진입점으로_변환한다() {
    let root = temporary_project("django-drf");
    fs::write(
        root.join("views.py"),
        r#"from rest_framework.viewsets import ViewSet
from models import User

def home(request):
    User.objects.filter(active=True)
    pass

class UserViewSet(ViewSet):
    def create(self, request):
        User.objects.create()
    pass
"#,
    )
    .expect("Django view fixture를 써야 한다");
    fs::write(
        root.join("models.py"),
        r#"from django.db import models

class User(models.Model):
    active = models.BooleanField(default=True)
"#,
    )
    .expect("Django model fixture를 써야 한다");
    fs::write(
        root.join("urls.py"),
        r#"from django.urls import path, re_path
from views import home

urlpatterns = [
    path("home/", home),
    re_path(r"^legacy/$", home),
]
"#,
    )
    .expect("Django URLConf fixture를 써야 한다");
    fs::write(
        root.join("api.py"),
        r#"from rest_framework.routers import DefaultRouter
from views import UserViewSet

router = DefaultRouter()
router.register("users", UserViewSet, basename="user")
"#,
    )
    .expect("DRF router fixture를 써야 한다");

    let overview = analyze(AnalysisRequest::new(&root))
        .expect("분석이 성공해야 한다")
        .overview
        .expect("Overview가 생성되어야 한다");
    assert!(overview.entrypoints.iter().any(|entry| {
        entry.path.as_deref() == Some("/home/")
            && entry.framework_id.as_deref() == Some("python.django")
    }));
    assert!(overview.entrypoints.iter().any(|entry| {
        entry.path.as_deref() == Some("/^legacy/$") && entry.method.as_deref() == Some("HTTP_REGEX")
    }));
    assert!(overview.entrypoints.iter().any(|entry| {
        entry.path.as_deref() == Some("/users")
            && entry.framework_id.as_deref() == Some("python.django_rest_framework")
    }));
    assert!(overview.resources.iter().any(|resource| {
        resource.name == "user" && resource.mode == code_analysis_engine::facts::AccessMode::Read
    }));
    assert!(overview.resources.iter().any(|resource| {
        resource.name == "user" && resource.mode == code_analysis_engine::facts::AccessMode::Write
    }));

    fs::remove_dir_all(root).expect("임시 프로젝트를 정리해야 한다");
}
