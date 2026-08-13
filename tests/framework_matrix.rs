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
    let path = std::env::temp_dir().join(format!("visual-map-framework-matrix-{name}-{suffix}"));
    fs::create_dir_all(&path).expect("임시 프로젝트를 만들어야 한다");
    path
}

fn has_framework(
    overview: &code_analysis_engine::views::overview::OverviewResponse,
    id: &str,
) -> bool {
    overview
        .detected_frameworks
        .iter()
        .any(|framework| framework.id == id)
}

fn has_route(
    overview: &code_analysis_engine::views::overview::OverviewResponse,
    framework_id: &str,
    path: &str,
) -> bool {
    overview.entrypoints.iter().any(|entrypoint| {
        entrypoint.framework_id.as_deref() == Some(framework_id)
            && entrypoint.kind == EntrypointKind::Http
            && entrypoint.path.as_deref() == Some(path)
    })
}

#[test]
fn c_cpp_csharp_adapter가_컴포넌트와_minimal_route를_보존한다() {
    let root = temporary_project("native-dotnet");
    fs::write(
        root.join("widget.c"),
        r#"
#include <QtCore/QObject>
struct CWidget { QObject* object; };
"#,
    )
    .expect("C Qt fixture를 써야 한다");
    fs::write(
        root.join("native.cpp"),
        r#"
#include <afxwin.h>
#include "CoreMinimal.h"
class DesktopApp : public CWinApp {};
class GameActor : public AActor {};
"#,
    )
    .expect("MFC와 Unreal fixture를 써야 한다");
    fs::write(
        root.join("program.cs"),
        r#"
using Microsoft.AspNetCore;
using Microsoft.AspNetCore.Components;
using Microsoft.Maui.Controls;

class Program {
  void Map(WebApplication app) { app.MapGet("/minimal-matrix", Get); }
  void Get() {}
}
class UserPanel : ComponentBase {}
class MainPage : ContentPage {}
"#,
    )
    .expect("ASP.NET, Blazor, MAUI fixture를 써야 한다");

    let overview = analyze(AnalysisRequest::new(&root))
        .expect("native와 .NET 분석이 성공해야 한다")
        .overview
        .expect("Overview가 생성되어야 한다");

    for framework_id in [
        "c.qt",
        "cpp.mfc",
        "cpp.unreal_engine",
        "csharp.aspnet_core",
        "csharp.minimal_api",
        "csharp.blazor",
        "csharp.dotnet_maui",
    ] {
        assert!(
            has_framework(&overview, framework_id),
            "framework가 없어: {framework_id}"
        );
    }
    assert!(has_route(
        &overview,
        "csharp.minimal_api",
        "/minimal-matrix"
    ));
    for (unit_name, modifier) in [
        ("DesktopApp", "framework:mfc-component"),
        ("GameActor", "framework:unreal-component"),
        ("UserPanel", "framework:blazor-component"),
        ("MainPage", "framework:maui-component"),
    ] {
        assert!(
            overview.units.iter().any(|unit| {
                unit.name == unit_name && unit.modifiers.iter().any(|value| value == modifier)
            }),
            "component marker가 없어: {unit_name} / {modifier}, units={:?}",
            overview
                .units
                .iter()
                .filter(|unit| unit.name == unit_name)
                .collect::<Vec<_>>()
        );
    }

    fs::remove_dir_all(root).expect("임시 프로젝트를 정리해야 한다");
}

#[test]
fn go_adapter가_주요_router의_정적_route를_보존한다() {
    let root = temporary_project("go-routes");
    fs::write(
        root.join("routes.go"),
        r#"
package main

import (
  "net/http"
  "github.com/gin-gonic/gin"
  "github.com/labstack/echo/v4"
  "github.com/gofiber/fiber/v2"
  "github.com/go-chi/chi/v5"
  beego "github.com/beego/beego/v2/server/web"
)

func handler() {}
func setup() {
  ginRouter := gin.Default()
  ginRouter.GET("/gin-matrix", handler)
  echoRouter := echo.New()
  echoRouter.GET("/echo-matrix", handler)
  fiberApp := fiber.New()
  fiberApp.Get("/fiber-matrix", handler)
  chiRouter := chi.NewRouter()
  chiRouter.Get("/chi-matrix", handler)
  beego.Get("/beego-matrix", handler)
  http.HandleFunc("/net-http-matrix", handler)
}
"#,
    )
    .expect("Go route fixture를 써야 한다");

    let overview = analyze(AnalysisRequest::new(&root))
        .expect("Go 분석이 성공해야 한다")
        .overview
        .expect("Overview가 생성되어야 한다");

    for (framework_id, path) in [
        ("go.net_http", "/net-http-matrix"),
        ("go.gin", "/gin-matrix"),
        ("go.echo", "/echo-matrix"),
        ("go.fiber", "/fiber-matrix"),
        ("go.chi", "/chi-matrix"),
        ("go.beego", "/beego-matrix"),
    ] {
        assert!(
            has_framework(&overview, framework_id),
            "framework가 없어: {framework_id}"
        );
        assert!(
            has_route(&overview, framework_id, path),
            "route가 없어: {framework_id} {path}"
        );
    }

    fs::remove_dir_all(root).expect("임시 프로젝트를 정리해야 한다");
}

#[test]
fn ecma_file_route와_fastify_koa_adapter가_정적_route를_보존한다() {
    let root = temporary_project("ecma-routes");
    fs::create_dir_all(root.join("server/api")).expect("Nuxt route 디렉터리를 만들어야 한다");
    fs::create_dir_all(root.join("routes/items"))
        .expect("SvelteKit route 디렉터리를 만들어야 한다");
    fs::write(
        root.join("router.ts"),
        r#"
import fastify from "fastify";
import Koa from "koa";
import Router from "@koa/router";

function handler() {}
const server = fastify();
server.get("/fastify-matrix", handler);
const app = new Koa();
const router = new Router();
router.get("/koa-matrix", handler);
"#,
    )
    .expect("Fastify와 Koa fixture를 써야 한다");
    fs::write(
        root.join("server/api/users.ts"),
        r#"
import { defineEventHandler } from "nuxt";
export function GET() { return defineEventHandler(() => "ok"); }
"#,
    )
    .expect("Nuxt route fixture를 써야 한다");
    fs::write(
        root.join("routes/items/+server.ts"),
        r#"
import type { RequestHandler } from "@sveltejs/kit";
export const GET: RequestHandler = async () => new Response("ok");
"#,
    )
    .expect("SvelteKit route fixture를 써야 한다");

    let overview = analyze(AnalysisRequest::new(&root))
        .expect("ECMA 분석이 성공해야 한다")
        .overview
        .expect("Overview가 생성되어야 한다");

    for framework_id in [
        "javascript.fastify",
        "javascript.koa",
        "javascript.nuxt",
        "javascript.sveltekit",
    ] {
        assert!(
            has_framework(&overview, framework_id),
            "framework가 없어: {framework_id}"
        );
    }
    assert!(has_route(
        &overview,
        "javascript.fastify",
        "/fastify-matrix"
    ));
    assert!(has_route(&overview, "javascript.koa", "/koa-matrix"));
    assert!(has_route(&overview, "javascript.nuxt", "/users"));
    assert!(has_route(&overview, "javascript.sveltekit", "/items"));

    fs::remove_dir_all(root).expect("임시 프로젝트를 정리해야 한다");
}

#[test]
fn java_python_dart_rust_adapter가_각자의_route를_보존한다() {
    let root = temporary_project("polyglot-routes");
    fs::write(
        root.join("QuarkusResource.java"),
        r#"
import io.quarkus.runtime.Quarkus;
@Path("/quarkus")
class QuarkusResource {
  @GET
  @Path("/items")
  void items() {}
}
"#,
    )
    .expect("Quarkus fixture를 써야 한다");
    fs::write(
        root.join("MicronautController.java"),
        r#"
import io.micronaut.http.annotation.Controller;
import io.micronaut.http.annotation.Get;
@Controller("/micronaut")
class MicronautController {
  @Get("/items")
  void items() {}
}
"#,
    )
    .expect("Micronaut fixture를 써야 한다");
    fs::write(
        root.join("sanic.py"),
        r#"
from sanic import Sanic
app = Sanic("matrix")
@app.get("/sanic-matrix")
async def sanic_route(request):
    return request
"#,
    )
    .expect("Sanic fixture를 써야 한다");
    fs::write(
        root.join("shelf.dart"),
        r#"
import "package:shelf/shelf.dart";
void shelf_route() {}
void setup(Router router) { router.get("/shelf-matrix", shelf_route); }
"#,
    )
    .expect("Shelf fixture를 써야 한다");
    fs::write(
        root.join("axum.rs"),
        r#"
use axum::{routing::get, Router};
fn axum_handler() {}
fn build() { let app = Router::new().route("/axum-matrix", get(axum_handler)); }
"#,
    )
    .expect("Axum fixture를 써야 한다");

    let overview = analyze(AnalysisRequest::new(&root))
        .expect("다중 언어 분석이 성공해야 한다")
        .overview
        .expect("Overview가 생성되어야 한다");

    for framework_id in [
        "java.quarkus",
        "java.micronaut",
        "python.sanic",
        "dart.shelf",
        "rust.axum",
    ] {
        assert!(
            has_framework(&overview, framework_id),
            "framework가 없어: {framework_id}"
        );
    }
    for (framework_id, path) in [
        ("java.quarkus", "/quarkus/items"),
        ("java.micronaut", "/micronaut/items"),
        ("python.sanic", "/sanic-matrix"),
        ("dart.shelf", "/shelf-matrix"),
        ("rust.axum", "/axum-matrix"),
    ] {
        assert!(
            has_route(&overview, framework_id, path),
            "route가 없어: {framework_id} {path}"
        );
    }

    fs::remove_dir_all(root).expect("임시 프로젝트를 정리해야 한다");
}
