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
    let path = std::env::temp_dir().join(format!("visual-map-framework-routes-{suffix}"));
    fs::create_dir_all(&path).expect("임시 프로젝트 디렉터리를 만들어야 한다");
    path
}

#[test]
fn 언어별_웹_문법을_공통_http_진입점으로_변환한다() {
    let root = temporary_project();
    fs::write(
        root.join("api.py"),
        r#"
from flask import Flask
app = Flask(__name__)
@app.route("/python")
def python_route(): pass
"#,
    )
    .expect("Python fixture를 써야 한다");
    fs::write(
        root.join("api.cs"),
        r#"
using Microsoft.AspNetCore;
[Route("api")]
class Api {
  [HttpGet("/csharp")]
  public void Get() {}
  void Map(WebApplication app) { app.MapGet("/minimal", Get); }
}
"#,
    )
    .expect("C# fixture를 써야 한다");
    fs::write(
        root.join("api.rs"),
        r#"
#[get("/rust")]
fn rust_route() {}
fn axum_route() { let app = Router::new().route("/axum", get(rust_route)); }
"#,
    )
    .expect("Rust fixture를 써야 한다");
    fs::write(
        root.join("actix.rs"),
        r#"
use actix_web::get;

#[get("/actix")]
async fn actix_route() {}

#[route("/actix-route", method = "GET")]
async fn actix_explicit_route() {}
"#,
    )
    .expect("Actix Web fixture를 써야 한다");
    fs::write(
        root.join("rocket.rs"),
        r#"
use rocket::get;

#[get("/rocket")]
fn rocket_route() {}
"#,
    )
    .expect("Rocket fixture를 써야 한다");
    fs::write(
        root.join("poem.rs"),
        r#"
use poem::handler;

#[handler]
async fn poem_handler() {}

fn poem_routes() {
  let route = Route::new().at("/poem", get(poem_handler));
  let _ = route;
}
"#,
    )
    .expect("Poem fixture를 써야 한다");
    fs::write(
        root.join("warp.rs"),
        r#"
use warp::Filter;

fn warp_routes() {
  let route = warp::path("warp").and(warp::get()).and_then(warp_handler);
  let _ = route;
}

async fn warp_handler() {}
"#,
    )
    .expect("Warp fixture를 써야 한다");
    fs::write(
        root.join("tauri.ts"),
        r#"
import { invoke } from "@tauri-apps/api/core";

export async function loadUser() {
  return invoke("load_user");
}

export async function loadDynamic(command: string) {
  return invoke(command);
}
"#,
    )
    .expect("Tauri fixture를 써야 한다");
    fs::write(
        root.join("angular.ts"),
        r#"
import { Component } from "@angular/core";

@Component({ selector: "app-root" })
export class AppComponent {}
"#,
    )
    .expect("Angular fixture를 써야 한다");
    fs::write(
        root.join("vue.ts"),
        r#"
import { defineComponent } from "vue";
export default defineComponent({ name: "UserCard" });
"#,
    )
    .expect("Vue fixture를 써야 한다");
    fs::write(
        root.join("widget.dart"),
        r#"
import "package:flutter/widgets.dart";
class HomePage extends StatelessWidget {}
"#,
    )
    .expect("Flutter fixture를 써야 한다");
    fs::write(
        root.join("blazor.cs"),
        r#"
using Microsoft.AspNetCore.Components;
class UserPanel : ComponentBase {}
"#,
    )
    .expect("Blazor fixture를 써야 한다");
    fs::write(
        root.join("maui.cs"),
        r#"
using Microsoft.Maui.Controls;
class MainPage : ContentPage {}
"#,
    )
    .expect("MAUI fixture를 써야 한다");
    fs::write(
        root.join("Api.java"),
        r#"
import org.springframework.web.bind.annotation.GetMapping;
@RequestMapping("/api")
class Api {
  @GetMapping(path = "/java")
  public void get() {}
}
"#,
    )
    .expect("Java fixture를 써야 한다");
    fs::write(
        root.join("api.go"),
        r#"
package main
import "net/http"
func main() { http.HandleFunc("/go", handler) }
"#,
    )
    .expect("Go fixture를 써야 한다");
    fs::write(
        root.join("gin.go"),
        r#"
package api
import "github.com/gin-gonic/gin"
func build() {
  router := gin.Default()
  v1 := router.Group("/v1")
  v1.GET("/users", listUsers)
}
func listUsers(c *gin.Context) {}
"#,
    )
    .expect("Gin fixture를 써야 한다");
    fs::write(
        root.join("express.ts"),
        r#"
import express from "express";
import { Router as createRouter } from "express";
const app = express();
app.get("/health", health);
app.ws("/socket", socketHandler);
    const forge = express();
    forge.get("/nebula", health);
const aliasedRouter = createRouter();
aliasedRouter.get("/aliased", health);
const catalog = { get: (_path: string, _handler: unknown) => undefined };
    catalog.get("/nebula", health);
const apiRouter = express.Router();
app.use("/api", apiRouter);
apiRouter.get("/mounted", health);
function health(request: Request, response: Response) {}
    function socketHandler(socket: WebSocket) {}

@DeleteDateColumn()
class SoftDeleteMarker {}
"#,
    )
    .expect("Express fixture를 써야 한다");
    fs::write(
        root.join("express-alias-only.ts"),
        r#"
import express from "express";
const nebula = express();
nebula.get("/alias-only", health);
function health(request: Request, response: Response) {}
"#,
    )
    .expect("Express alias-only fixture를 써야 한다");
    fs::write(
        root.join("mounted-router.ts"),
        r#"
import express from "express";
const mountedRouter = express.Router();
mountedRouter.get("/cross-file", health);
function health(request: Request, response: Response) {}
export default mountedRouter;
"#,
    )
    .expect("교차 파일 Express router fixture를 써야 한다");
    fs::write(
        root.join("mount-app.ts"),
        r#"
import express from "express";
import mountedRouter from "./mounted-router";
const mountApp = express();
mountApp.use("/mounted", mountedRouter);
"#,
    )
    .expect("교차 파일 Express mount fixture를 써야 한다");
    fs::write(
        root.join("nest.ts"),
        r#"
import { Controller, Get } from "@nestjs/common";
@Controller("users")
class UsersController {
  @Get(":id")
  getUser() {}
}
"#,
    )
    .expect("NestJS fixture를 써야 한다");
    fs::create_dir_all(root.join("app/api/health")).expect("Next route 디렉터리를 만들어야 한다");
    fs::write(
        root.join("app/api/health/route.ts"),
        r#"
import { NextResponse } from "next/server";
export async function GET() { return NextResponse.json({ ok: true }); }
"#,
    )
    .expect("Next route fixture를 써야 한다");
    fs::create_dir_all(root.join("pages/api")).expect("Next Pages API 디렉터리를 만들어야 한다");
    fs::write(
        root.join("pages/api/legacy.ts"),
        "export default function legacy(request: Request, response: Response) { response.end(); }\n",
    )
    .expect("Next Pages API fixture를 써야 한다");
    fs::write(
        root.join("shelf.dart"),
        r#"
import "package:shelf/shelf.dart";
void routes(Router router) { router.get("/dart", dartHandler); }
Response dartHandler(Request request) => Response.ok("ok");
"#,
    )
    .expect("Shelf fixture를 써야 한다");
    fs::create_dir_all(root.join("routes/users/[id]"))
        .expect("Dart Frog route 디렉터리를 만들어야 한다");
    fs::write(
        root.join("routes/users/[id]/index.dart"),
        r#"
import "package:dart_frog/dart_frog.dart";
Future<Response> onRequest(RequestContext context) async => Response(body: "ok");
"#,
    )
    .expect("Dart Frog fixture를 써야 한다");
    fs::create_dir_all(root.join("conf")).expect("Play route 디렉터리를 만들어야 한다");
    fs::write(
        root.join("conf/routes"),
        "GET     /play      controllers.Users.list()\nPOST    /play/orders controllers.Orders.create()\n",
    )
    .expect("Play route DSL fixture를 써야 한다");
    fs::write(root.join("pom.xml"), "<dependency>play.mvc</dependency>")
        .expect("Play manifest fixture를 써야 한다");

    let result = analyze(AnalysisRequest::new(&root)).expect("분석이 성공해야 한다");
    let overview = result.overview.expect("Overview가 생성되어야 한다");
    let paths: Vec<_> = overview
        .entrypoints
        .iter()
        .filter_map(|entrypoint| entrypoint.path.as_deref())
        .collect();
    for path in [
        "/python",
        "/api/csharp",
        "/minimal",
        "/rust",
        "/actix",
        "/actix-route",
        "/rocket",
        "/poem",
        "/warp",
        "/axum",
        "/go",
        "/api/java",
        "/health",
        "/nebula",
        "/aliased",
        "/alias-only",
        "/users/:id",
        "/play",
        "/play/orders",
        "/v1/users",
        "/api/health",
        "/api/legacy",
        "/dart",
        "/users/:id",
    ] {
        assert!(
            paths.contains(&path),
            "진입점이 없어: {path}, 실제={paths:?}"
        );
    }
    assert!(!overview.entrypoints.iter().any(|entrypoint| {
        entrypoint.path.as_deref() == Some("/nebula")
            && entrypoint
                .evidence
                .iter()
                .any(|evidence| evidence.value.contains("catalog.get"))
    }));

    assert!(overview.entrypoints.iter().any(|entrypoint| {
        entrypoint.framework_id.as_deref() == Some("rust.actix_web")
            && entrypoint.path.as_deref() == Some("/actix")
    }));
    assert!(
        overview.entrypoints.iter().any(|entrypoint| {
            entrypoint.framework_id.as_deref() == Some("rust.actix_web")
                && entrypoint.path.as_deref() == Some("/actix-route")
                && entrypoint.method.as_deref() == Some("GET")
        }),
        "Actix explicit route가 없어: {:?}",
        overview
            .entrypoints
            .iter()
            .filter(|entrypoint| entrypoint.path.as_deref() == Some("/actix-route"))
            .collect::<Vec<_>>()
    );
    assert!(overview.entrypoints.iter().any(|entrypoint| {
        entrypoint.framework_id.as_deref() == Some("rust.rocket")
            && entrypoint.path.as_deref() == Some("/rocket")
    }));
    assert!(overview.entrypoints.iter().any(|entrypoint| {
        entrypoint.framework_id.as_deref() == Some("rust.poem")
            && entrypoint.kind == code_analysis_engine::facts::EntrypointKind::Callback
            && entrypoint.name == "poem_handler"
    }));
    assert!(overview.entrypoints.iter().any(|entrypoint| {
        entrypoint.framework_id.as_deref() == Some("rust.warp")
            && entrypoint.path.as_deref() == Some("/warp")
            && entrypoint.method.as_deref() == Some("HTTP_FILTER")
    }));
    assert!(
        overview
            .detected_frameworks
            .iter()
            .any(|framework| framework.id == "java.play"),
        "Play 감지 결과가 없어: {:?}",
        overview.detected_frameworks
    );
    assert!(
        overview.entrypoints.iter().any(|entrypoint| {
            entrypoint.framework_id.as_deref() == Some("java.play")
                && entrypoint.path.as_deref() == Some("/play")
                && entrypoint.method.as_deref() == Some("GET")
        }),
        "Play entrypoint가 없어: {:?}",
        overview.entrypoints
    );
    assert!(!overview.entrypoints.iter().any(|entrypoint| {
        entrypoint.path.as_deref() == Some("/play")
            && entrypoint.framework_id.as_deref() == Some("csharp.aspnet_mvc")
    }));
    let wrong_rust_frameworks = overview
        .entrypoints
        .iter()
        .filter(|entrypoint| {
            entrypoint.path.as_deref() == Some("/rust")
                && matches!(
                    entrypoint.framework_id.as_deref(),
                    Some("rust.actix_web") | Some("rust.rocket")
                )
        })
        .map(|entrypoint| {
            (
                entrypoint.framework_id.clone(),
                entrypoint
                    .evidence
                    .first()
                    .map(|evidence| evidence.kind.clone()),
            )
        })
        .collect::<Vec<_>>();
    assert!(
        wrong_rust_frameworks.is_empty(),
        "잘못 분류된 /rust: {wrong_rust_frameworks:?}"
    );
    assert!(overview.units.iter().any(|unit| {
        unit.name == "AppComponent"
            && unit
                .modifiers
                .iter()
                .any(|modifier| modifier == "framework:angular-component")
    }));
    assert!(overview.units.iter().any(|unit| {
        unit.relative_path == "vue.ts"
            && unit
                .modifiers
                .iter()
                .any(|modifier| modifier == "framework:vue-component")
    }));
    assert!(overview.units.iter().any(|unit| {
        unit.name == "HomePage"
            && unit
                .modifiers
                .iter()
                .any(|modifier| modifier == "framework:flutter-widget")
    }));
    assert!(overview.units.iter().any(|unit| {
        unit.name == "UserPanel"
            && unit
                .modifiers
                .iter()
                .any(|modifier| modifier == "framework:blazor-component")
    }));
    assert!(overview.units.iter().any(|unit| {
        unit.name == "MainPage"
            && unit
                .modifiers
                .iter()
                .any(|modifier| modifier == "framework:maui-component")
    }));
    assert!(overview.entrypoints.iter().any(|entrypoint| {
        entrypoint.framework_id.as_deref() == Some("javascript.tauri")
            && entrypoint.kind == code_analysis_engine::facts::EntrypointKind::Event
            && entrypoint.path.as_deref() == Some("load_user")
    }));
    assert!(overview.entrypoints.iter().any(|entrypoint| {
        entrypoint.framework_id.as_deref() == Some("javascript.tauri")
            && entrypoint.name == "<dynamic>"
            && entrypoint.path.is_none()
    }));
    assert!(overview.entrypoints.iter().any(|entrypoint| {
        entrypoint.framework_id.as_deref() == Some("javascript.express")
            && entrypoint.kind == code_analysis_engine::facts::EntrypointKind::WebSocket
            && entrypoint.path.as_deref() == Some("/socket")
            && entrypoint.method.as_deref() == Some("WS")
    }));
    assert!(overview.entrypoints.iter().any(|entrypoint| {
        entrypoint.framework_id.as_deref() == Some("javascript.express")
            && entrypoint.kind == EntrypointKind::Http
            && entrypoint.path.as_deref() == Some("/api/mounted")
    }));
    assert!(
        overview.entrypoints.iter().any(|entrypoint| {
            entrypoint.framework_id.as_deref() == Some("javascript.express")
                && entrypoint.kind == EntrypointKind::Http
                && entrypoint.path.as_deref() == Some("/mounted/cross-file")
        }),
        "교차 파일 route가 없어: {:?}",
        overview
            .entrypoints
            .iter()
            .filter(|entrypoint| entrypoint.framework_id.as_deref() == Some("javascript.express"))
            .collect::<Vec<_>>()
    );
    assert!(!overview.entrypoints.iter().any(|entrypoint| {
        entrypoint.evidence.iter().any(|evidence| {
            evidence.kind == "routeSource" && evidence.value == "@DeleteDateColumn()"
        })
    }));

    fs::remove_dir_all(root).expect("임시 프로젝트를 정리해야 한다");
}

#[test]
fn go_rust_cpp_rpc_등록을_공통_rpc_진입점으로_변환한다() {
    let root = temporary_project();
    fs::write(
        root.join("go_rpc.go"),
        r#"
package api

import "google.golang.org/grpc"

type GreeterServer struct{}

func registerGo() {
  grpcServer := grpc.NewServer()
  pb.RegisterGreeterServer(grpcServer, &GreeterServer{})
}
"#,
    )
    .expect("Go gRPC fixture를 써야 한다");
    fs::write(
        root.join("rust_rpc.rs"),
        r#"
use tonic::transport::Server;

struct GreeterServer;

async fn register_rust(service: GreeterServer) {
  Server::builder()
    .add_service(GreeterServer::new(service))
    .serve(addr)
    .await;
}
"#,
    )
    .expect("Rust Tonic fixture를 써야 한다");
    fs::write(
        root.join("cpp_rpc.cpp"),
        r#"
#include <grpcpp/grpcpp.h>

class GreeterService {};

void register_cpp() {
  grpc::ServerBuilder builder;
  GreeterService service;
  builder.RegisterService(&service);
}
"#,
    )
    .expect("C++ gRPC fixture를 써야 한다");
    fs::write(
        root.join("desktop.rs"),
        r#"
use tauri::command;

#[tauri::command]
fn greet() {}

#[tokio::main]
async fn main() {}
"#,
    )
    .expect("Tauri/Tokio fixture를 써야 한다");
    fs::write(
        root.join("serverpod.dart"),
        r#"
import "package:serverpod/serverpod.dart";

class UserEndpoint extends Endpoint {
  Future<String> getUser(Session session, String id) async => id;
}
"#,
    )
    .expect("Serverpod fixture를 써야 한다");
    fs::write(
        root.join("cpp_web.cpp"),
        r#"
#include <crow.h>
#include <drogon/drogon.h>
class ApiController {
 public:
  void get() {}
};
void crow_handler() {}
CROW_ROUTE(app, "/crow")([] {});
CROW_ROUTE(app, "/crow-methods").methods(crow::HTTPMethod::GET, crow::HTTPMethod::POST)(crow_handler);
ADD_METHOD_TO(ApiController::get, "/drogon");
METHOD_ADD(ApiController::get, "/drogon-method", Post);
"#,
    )
    .expect("C++ 웹 fixture를 써야 한다");
    fs::write(
        root.join("cpp_web.h"),
        r#"
#include <drogon/HttpController.h>
class HeaderController {
 public:
  void show() {}
};
ADD_METHOD_TO(HeaderController::show, "/header", Get);
"#,
    )
    .expect("C++ header 웹 fixture를 써야 한다");
    fs::write(
        root.join("jaxrs.java"),
        r#"
import jakarta.ws.rs.GET;
import jakarta.ws.rs.Path;

@Path("/api")
class JaxResource {
  @GET
  @Path("/users")
  void users() {}
}
"#,
    )
    .expect("JAX-RS fixture를 써야 한다");
    fs::write(
        root.join("micronaut.java"),
        r#"
import io.micronaut.http.annotation.Controller;
import io.micronaut.http.annotation.Get;

@Controller("/orders")
class OrdersController {
  @Get("/list")
  void list() {}
}
"#,
    )
    .expect("Micronaut fixture를 써야 한다");

    let result = analyze(AnalysisRequest::new(&root)).expect("분석이 성공해야 한다");
    let overview = result.overview.expect("Overview가 생성되어야 한다");
    let rpc_entrypoints: Vec<_> = overview
        .entrypoints
        .iter()
        .filter(|entrypoint| entrypoint.kind == EntrypointKind::Rpc)
        .collect();

    assert!(rpc_entrypoints.iter().any(|entrypoint| {
        entrypoint.framework_id.as_deref() == Some("go.grpc")
            && entrypoint.name == "Greeter"
            && entrypoint.method.as_deref() == Some("RPC")
    }));
    assert!(rpc_entrypoints.iter().any(|entrypoint| {
        entrypoint.framework_id.as_deref() == Some("rust.tonic")
            && entrypoint.name == "Greeter"
            && entrypoint.path.is_none()
    }));
    assert!(rpc_entrypoints.iter().any(|entrypoint| {
        entrypoint.framework_id.as_deref() == Some("cpp.grpc")
            && entrypoint.name == "service"
            && entrypoint.path.is_none()
    }));
    assert!(overview.entrypoints.iter().any(|entrypoint| {
        entrypoint.framework_id.as_deref() == Some("rust.tauri")
            && entrypoint.kind == EntrypointKind::Rpc
            && entrypoint.name == "greet"
    }));
    assert!(overview.entrypoints.iter().any(|entrypoint| {
        entrypoint.framework_id.as_deref() == Some("rust.tokio")
            && entrypoint.kind == EntrypointKind::Main
            && entrypoint.name == "main"
    }));
    assert!(overview.entrypoints.iter().any(|entrypoint| {
        entrypoint.framework_id.as_deref() == Some("dart.serverpod")
            && entrypoint.kind == EntrypointKind::Rpc
            && entrypoint.path.as_deref() == Some("/getUser")
    }));
    assert!(overview
        .entrypoints
        .iter()
        .any(|entrypoint| entrypoint.path.as_deref() == Some("/crow")));
    let crow_methods = overview
        .entrypoints
        .iter()
        .filter(|entrypoint| {
            entrypoint.framework_id.as_deref() == Some("cpp.crow")
                && entrypoint.path.as_deref() == Some("/crow-methods")
        })
        .filter_map(|entrypoint| entrypoint.method.as_deref())
        .collect::<Vec<_>>();
    assert_eq!(
        crow_methods,
        vec!["GET", "POST"],
        "crow entries: {overview:?}"
    );
    assert!(overview
        .entrypoints
        .iter()
        .any(|entrypoint| entrypoint.path.as_deref() == Some("/drogon")));
    assert!(overview.entrypoints.iter().any(|entrypoint| {
        entrypoint.framework_id.as_deref() == Some("cpp.drogon")
            && entrypoint.path.as_deref() == Some("/drogon")
            && entrypoint.method.as_deref() == Some("HTTP")
            && entrypoint.name == "HTTP /drogon"
    }));
    assert!(overview.entrypoints.iter().any(|entrypoint| {
        entrypoint.framework_id.as_deref() == Some("cpp.drogon")
            && entrypoint.path.as_deref() == Some("/drogon-method")
            && entrypoint.method.as_deref() == Some("POST")
            && entrypoint.name == "POST /drogon-method"
    }));
    let drogon_handler = overview
        .entrypoints
        .iter()
        .find(|entrypoint| {
            entrypoint.framework_id.as_deref() == Some("cpp.drogon")
                && entrypoint.path.as_deref() == Some("/drogon-method")
        })
        .expect("Drogon METHOD_ADD 진입점이 있어야 한다");
    assert_eq!(
        overview
            .units
            .iter()
            .find(|unit| unit.id == drogon_handler.unit_id)
            .map(|unit| unit.name.as_str()),
        Some("get")
    );
    let header_file = result
        .files
        .iter()
        .find(|file| file.relative_path == "cpp_web.h")
        .expect("C++ header가 스캔되어야 한다");
    assert_eq!(
        header_file.language,
        code_analysis_engine::model::Language::Cpp
    );
    assert!(overview.entrypoints.iter().any(|entrypoint| {
        entrypoint.framework_id.as_deref() == Some("cpp.drogon")
            && entrypoint.path.as_deref() == Some("/header")
            && entrypoint.method.as_deref() == Some("GET")
            && overview
                .units
                .iter()
                .find(|unit| unit.id == entrypoint.unit_id)
                .is_some_and(|unit| unit.name == "show")
    }));
    assert!(overview
        .entrypoints
        .iter()
        .any(|entrypoint| entrypoint.path.as_deref() == Some("/api/users")));
    assert!(overview
        .entrypoints
        .iter()
        .any(|entrypoint| entrypoint.path.as_deref() == Some("/orders/list")));

    fs::remove_dir_all(root).expect("임시 프로젝트를 정리해야 한다");
}

#[test]
fn aspnet_conventional_mvc는_controller_action_경로를_만든다() {
    let root = temporary_project();
    fs::write(
        root.join("CustomerController.cs"),
        r#"
using Microsoft.AspNetCore.Mvc;
public class CustomerController : Controller {
  [HttpGet]
  public IActionResult Login() { return View(); }
}
"#,
    )
    .expect("C# MVC fixture를 써야 한다");

    let overview = analyze(AnalysisRequest::new(&root))
        .expect("분석이 성공해야 한다")
        .overview
        .expect("Overview가 생성되어야 한다");

    assert!(overview.entrypoints.iter().any(|entrypoint| {
        entrypoint.path.as_deref() == Some("/Customer/Login")
            && entrypoint.method.as_deref() == Some("HTTPGET")
    }));

    fs::remove_dir_all(root).expect("임시 프로젝트를 정리해야 한다");
}

#[test]
fn nestjs_graphql_resolver는_api_prefix_계약을_만든다() {
    let root = temporary_project();
    fs::create_dir_all(root.join("api/resolvers/shop")).expect("resolver 디렉터리를 만들어야 한다");
    fs::write(
        root.join("api/resolvers/shop/shop-auth.resolver.ts"),
        r#"
import { Mutation, Resolver } from "@nestjs/graphql";

@Resolver()
export class ShopAuthResolver {
  @Mutation()
  async registerCustomerAccount() {
    return {};
  }
}
"#,
    )
    .expect("GraphQL fixture를 써야 한다");

    let overview = analyze(AnalysisRequest::new(&root))
        .expect("분석이 성공해야 한다")
        .overview
        .expect("Overview가 생성되어야 한다");

    assert!(overview.entrypoints.iter().any(|entrypoint| {
        entrypoint.path.as_deref() == Some("/shop-api/registerCustomerAccount")
            && entrypoint.method.as_deref() == Some("POST")
    }));
    assert!(overview
        .entrypoints
        .iter()
        .any(|entrypoint| entrypoint.path.as_deref() == Some("/shop-api")));

    fs::remove_dir_all(root).expect("임시 프로젝트를 정리해야 한다");
}
