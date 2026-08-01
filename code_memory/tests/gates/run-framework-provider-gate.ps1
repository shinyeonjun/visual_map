param(
    [string]$Bridge = (Join-Path $PSScriptRoot '..\..\rust\target\release\code-memory-language.exe'),
    [string]$Root = (Join-Path $PSScriptRoot '..\..'),
    [ValidateSet('all','typescript','javascript','python','java','csharp','c','cpp','go','rust','php','ruby','dart')]
    [string]$Language = 'all',
    [string]$Framework = 'all',
    [string]$ProvidersRoot = ''
)

$ErrorActionPreference = 'Stop'
if (Get-Variable PSNativeCommandUseErrorActionPreference -ErrorAction SilentlyContinue) {
    $PSNativeCommandUseErrorActionPreference = $false
}
$bundledProviders = Join-Path $Root 'providers'
if ([string]::IsNullOrWhiteSpace($ProvidersRoot) -and (Test-Path $bundledProviders)) {
    $ProvidersRoot = $bundledProviders
}

function New-JsSource {
    param([string]$Language, [string]$Framework)

    $extension = if ($Language -eq 'typescript') { 'ts' } else { 'js' }
    $jsxExtension = if ($Language -eq 'typescript') { 'tsx' } else { 'jsx' }
    switch ($Framework) {
        'react' {
            return @{
                path = "src/App.$jsxExtension"
                source = @"
import React from "react";
export function handle() { return true; }
export function App() { return <button onClick={handle}>ok</button>; }
"@
            }
        }
        'nextjs' {
            return @(
                @{ path = "src/app/route.$extension"; source = 'export async function GET() { return new Response("ok"); }' },
                @{ path = "src/app/page.$jsxExtension"; source = 'export default function Page() { return <div>page</div>; } export async function action() { "use server"; }' }
            )
        }
        'angular' {
            return @{
                path = "src/App.$extension"
                source = @"
import { Component, Injectable } from "@angular/core";
@Component({ selector: "app-root", template: "<button></button>" })
export class App {}
@Injectable()
export class UserService {}
export const route = Route("/fixture", App);
"@
            }
        }
        'vue' {
            return @{
                path = "src/App.$extension"
                source = @"
import { defineComponent } from "vue";
export function handle() { return true; }
export const App = defineComponent({ template: '<button @click="handle">ok</button>' });
"@
            }
        }
        'nuxt' {
            return @(
                @{ path = "server/api/fixture.$extension"; source = 'import { defineEventHandler } from "nuxt"; export function handler() { return "ok"; } export default defineEventHandler(handler);' },
                @{ path = "src/App.$extension"; source = 'import { defineComponent } from "nuxt"; export const App = defineComponent({ template: "<div>ok</div>" });' }
            )
        }
        'sveltekit' {
            return @(
                @{ path = "src/routes/fixture/+server.$extension"; source = 'export async function GET() { return new Response("ok"); }' },
                @{ path = "src/routes/+page.$extension"; source = 'import { defineComponent } from "svelte"; export const App = defineComponent({ template: "<button>ok</button>" }); export function load() { return {}; }' }
            )
        }
        'express' {
            return @{
                path = "src/fixture.$extension"
                source = @"
import express from "express";
const app = express();
export function authMiddleware(_req: unknown, _res: unknown, next: () => void) { next(); }
export function handler() { return "ok"; }
app.use(authMiddleware);
app.get("/fixture", handler);
"@
            }
        }
        'fastify' {
            return @{
                path = "src/fixture.$extension"
                source = @"
import fastify from "fastify";
export async function plugin() {}
export async function handler() { return "ok"; }
export function authMiddleware(_req: unknown, _res: unknown, next: () => void) { next(); }
fastify.register(plugin);
fastify.addHook("onRequest", authMiddleware);
fastify.get("/fixture", handler);
"@
            }
        }
        'nestjs' {
            return @{
                path = "src/fixture.$extension"
                source = @"
import { Controller, Get, Injectable } from "@nestjs/common";
@Injectable()
export class UserService {}
function AuthGuard() { return true; }
@Controller("/api")
export class AppController {
  @UseGuards(AuthGuard)
  @Get("/fixture")
  handler() { return "ok"; }
}
"@
            }
        }
        'koa' {
            return @{
                path = "src/fixture.$extension"
                source = @"
import Koa from "koa";
const app = new Koa();
const router = { get(_path: string, _handler: () => string) {} };
export function authMiddleware(_ctx: unknown, next: () => void) { next(); }
export function handler() { return "ok"; }
router.get("/fixture", handler);
app.use(authMiddleware);
"@
            }
        }
        'tauri' {
            return @{
                path = "src/fixture.$extension"
                source = 'import { invoke } from "@tauri-apps/api/core"; export async function loadSessions() { return await invoke("list_sessions"); }'
            }
        }
        default { throw "No JavaScript fixture template for $Language/$Framework" }
    }
}

function New-ProjectFiles {
    param([string]$Language, [string]$Framework, $PackFixture)

    if ($Language -in @('typescript','javascript')) {
        $files = @(New-JsSource -Language $Language -Framework $Framework)
        $files += @{ path = 'package.json'; source = (@($PackFixture.files | Where-Object path -eq 'package.json' | Select-Object -First 1).source) }
        if ($null -eq $files[-1].source) {
            $files[-1].source = '{"name":"framework-fixture","version":"1.0.0"}'
        }
        $config = @{
            compilerOptions = @{
                allowJs = ($Language -eq 'javascript')
                checkJs = $false
                jsx = 'react-jsx'
                module = 'NodeNext'
                moduleResolution = 'NodeNext'
                target = 'ES2022'
            }
            include = @('src/**/*','server/**/*')
        } | ConvertTo-Json -Compress
        $files += @{ path = 'tsconfig.json'; source = $config }
        return $files
    }

    if ($Language -eq 'python') {
        $source = switch ($Framework) {
            'django' {
                @"
from django.urls import path

class UserService:
    pass

class AuthMiddleware:
    def __call__(self, request):
        return request

def handler(request):
    return None

urlpatterns = [path("/fixture", handler)]
"@
            }
            'flask' {
                @"
from flask import Flask

app = Flask(__name__)

def auth_middleware():
    return None

@app.route("/fixture")
def handler():
    return "ok"

app.before_request(auth_middleware)
"@
            }
            'fastapi' {
                @"
from fastapi import Depends, FastAPI

app = FastAPI()

def auth():
    return True

@app.get("/fixture")
def handler(authorized: bool = Depends(auth)):
    return {"ok": authorized}
"@
            }
            'starlette' {
                @"
from starlette.routing import Route

def auth_middleware(request):
    return request

def handler(request):
    return None

routes = [Route("/fixture", handler)]
app.add_middleware(auth_middleware)
"@
            }
            'sanic' {
                @"
from sanic import Sanic

app = Sanic("fixture")

@app.get("/fixture")
async def handler(request):
    return {"ok": True}

@app.middleware("request")
async def auth_middleware(request):
    return None
"@
            }
            default { throw "No Python fixture template for $Framework" }
        }
        return @(
            @{ path = 'src/fixture.py'; source = $source },
            @{ path = 'pyproject.toml'; source = "[project]`nname = `"framework-fixture`"`nversion = `"0.1.0`"`nrequires-python = `">=3.11`"`n" }
        )
    }

    if ($Language -eq 'java') {
        $source = switch ($Framework) {
            'spring' {
                @"
package fixture;
import org.springframework.web.bind.annotation.*;
@Service class UserService {}
@RestController
@RequestMapping("/api")
class FixtureController {
    @Filter void filter() {}
    @GetMapping("/fixture") public void handler() {}
    @Autowired UserService service;
}
"@
            }
            'spring-boot' {
                @"
package fixture;
import org.springframework.boot.autoconfigure.SpringBootApplication;
import org.springframework.web.bind.annotation.*;
@SpringBootApplication
@RestController
class FixtureController {
    @GetMapping("/fixture") public void handler() {}
    @Autowired
    UserService service;
}
@Service class UserService {}
"@
            }
            'spring-mvc' {
                @"
package fixture;
import org.springframework.web.bind.annotation.*;
@Controller
@RequestMapping("/api")
class FixtureController {
    @Filter void filter() {}
    @GetMapping("/fixture") public void handler() {}
}
"@
            }
            'spring-webflux' {
                @"
package fixture;
import reactor.core.publisher.Mono;
import org.springframework.web.reactive.function.server.RouterFunction;
import java.util.concurrent.Future;
class FixtureController {
    public void handler() {}
    void routes() { RouterFunctions.route(RequestPredicates.GET("/fixture"), request -> handler()); }
    Future<String> asyncHandler() { return null; }
}
"@
            }
            'jakarta-ee' {
                @"
package fixture;
import jakarta.ws.rs.*;
@Path("/fixture")
class FixtureResource {
    @GET @Path("/fixture") public void handler() {}
    @Inject UserService service;
}
class UserService {}
"@
            }
            'quarkus' {
                @"
package fixture;
import io.quarkus.runtime.annotations.RegisterForReflection;
import jakarta.ws.rs.*;
import jakarta.enterprise.context.ApplicationScoped;
@ApplicationScoped
@Path("/fixture")
class FixtureResource {
    @GET @Path("/fixture") public void handler() {}
    @Inject UserService service;
}
class UserService {}
"@
            }
            'micronaut' {
                @"
package fixture;
import io.micronaut.http.annotation.*;
@Singleton class UserService {}
@Controller("/api")
class FixtureController {
    @Get("/fixture") public void handler() {}
    @Inject UserService service;
}
"@
            }
            'play' {
                @"
package fixture;
import play.mvc.*;
@Filter class AuthFilter {}
public class Application {
    public Result handler() { return null; }
}
"@
            }
            default { throw "No Java fixture template for $Framework" }
        }
        $files = @(
            @{ path = 'src/main/java/fixture/Fixture.java'; source = $source },
            @{ path = 'pom.xml'; source = '<project><modelVersion>4.0.0</modelVersion><groupId>fixture</groupId><artifactId>fixture</artifactId><version>1.0.0</version></project>' }
        )
        if ($Framework -eq 'play') {
            $files += @{ path = 'conf/routes'; source = 'GET /fixture controllers.Application.handler' }
        }
        return $files
    }

    if ($Language -eq 'csharp') {
        $source = switch ($Framework) {
            'aspnet-core' {
                @"
using Microsoft.AspNetCore.Builder;
using Microsoft.AspNetCore.Mvc;
public class UserService {}
public class AuthMiddleware {}
public class Program {
    public static void Main() {
        var builder = WebApplication.CreateBuilder();
        builder.Services.AddScoped<UserService>();
        var app = builder.Build();
        app.UseMiddleware<AuthMiddleware>();
        app.MapGet("/fixture", Handler);
    }
    public static IResult Handler() { return Results.Ok(); }
}
"@
            }
            'aspnet-mvc' {
                @"
using Microsoft.AspNetCore.Mvc;
[Controller]
[ApiController]
[Route("/api")]
public class FixtureController : ControllerBase {
    [HttpGet("/fixture")]
    public IActionResult Handler() { return Ok(); }
}
public class Pipeline { public void Use(object app) {} }
public class AuthMiddleware {}
"@
            }
            'aspnet-web-api' {
                @"
using System.Web.Http;
[ApiController]
[Route("/api")]
public class FixtureController : ApiController {
    [HttpGet] [Route("/fixture")]
    public IHttpActionResult Handler() { return Ok(); }
}
public class AuthMiddleware {}
"@
            }
            'minimal-api' {
                @"
using Microsoft.AspNetCore.Builder;
public class AuthMiddleware {}
public class Program {
    public static void Main() {
        var builder = WebApplication.CreateBuilder();
        var app = builder.Build();
        app.UseMiddleware<AuthMiddleware>();
        app.MapGet("/fixture", Handler);
    }
    public static string Handler() { return "ok"; }
}
"@
            }
            'blazor' {
                @"
using Microsoft.AspNetCore.Components;
@page "/fixture"
public partial class FixtureComponent : ComponentBase {
    public void OnClick() {}
    public RenderFragment Render() => builder => {};
}
public static class Markup {
    public const string Value = "<button @onclick=\"OnClick\">ok</button>";
}
"@
            }
            'dotnet-maui' {
                @"
using Microsoft.Maui.Controls;
public class MainPage : ContentPage {
    public void OnClicked() {}
    public RenderFragment Render() => builder => {};
}
public class ShellHost : Shell {}
public static class Markup {
    public const string Value = "<Widget Clicked=OnClicked />";
}
"@
            }
            default { throw "No C# fixture template for $Framework" }
        }
        return @(
            @{ path = 'Fixture.cs'; source = $source },
            @{ path = 'fixture.csproj'; source = '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net8.0</TargetFramework><EnableDefaultCompileItems>true</EnableDefaultCompileItems></PropertyGroup></Project>' },
            @{ path = 'fixture.sln'; source = @'
Microsoft Visual Studio Solution File, Format Version 12.00
# Visual Studio Version 17
VisualStudioVersion = 17.0.31903.59
MinimumVisualStudioVersion = 10.0.40219.1
Project("{FAE04EC0-301F-11D3-BF4B-00C04F79EFBC}") = "fixture", "fixture.csproj", "{11111111-1111-1111-1111-111111111111}"
EndProject
Global
	GlobalSection(SolutionConfigurationPlatforms) = preSolution
		Debug|Any CPU = Debug|Any CPU
		Release|Any CPU = Release|Any CPU
	EndGlobalSection
	GlobalSection(ProjectConfigurationPlatforms) = postSolution
		{11111111-1111-1111-1111-111111111111}.Debug|Any CPU.ActiveCfg = Debug|Any CPU
		{11111111-1111-1111-1111-111111111111}.Debug|Any CPU.Build.0 = Debug|Any CPU
		{11111111-1111-1111-1111-111111111111}.Release|Any CPU.ActiveCfg = Release|Any CPU
		{11111111-1111-1111-1111-111111111111}.Release|Any CPU.Build.0 = Release|Any CPU
	EndGlobalSection
EndGlobal
'@ }
        )
    }

    if ($Language -in @('c','cpp')) {
        $extension = if ($Language -eq 'c') { 'c' } else { 'cpp' }
        $source = switch ($Framework) {
            'gtk-glib' {
                @"
#include <gtk/gtk.h>
typedef struct GtkWidget GtkWidget;
void callback(GtkWidget *widget, void *data) {}
void build_ui(void) { GtkWidget *button = 0; g_signal_connect(button, "clicked", callback); }
"@
            }
            'qt' {
                @"
#include <QtCore/QObject>
typedef struct QWidget QWidget;
void handler(void) {}
int signals = 0;
void slots(void) {}
void build_ui(void) { QWidget *window = 0; QObject_connect(window, handler); QtConcurrent_run(handler); }
"@
            }
            'libuv' {
                @"
#include <uv.h>
void read_callback(void *stream, int status, const void *buffer) {}
void alloc(void) {}
void start(void *stream) { uv_read_start(stream, alloc, read_callback); uv_run(0, UV_RUN_DEFAULT); }
"@
            }
            'libevent' {
                @"
#include <event2/event.h>
void event_callback(int fd, short events, void *arg) {}
void start_event(void *base) { event_base_dispatch(base); }
"@
            }
            'mfc' {
                @"
#include <afxwin.h>
#define BEGIN_MESSAGE_MAP(a,b)
#define END_MESSAGE_MAP()
#define ON_COMMAND(a,b)
class MainWindow : public CWnd { public: void OnClick() {} };
BEGIN_MESSAGE_MAP(MainWindow, CWnd)
ON_COMMAND(ID_CLICK, OnClick)
END_MESSAGE_MAP()
"@
            }
            'boost-asio' {
                @"
#include <boost/asio.hpp>
namespace boost { namespace asio { struct io_context {}; } }
struct Socket { template<class B, class F> void async_read_some(B, F) {} };
void handler(void) {}
void start(void) { boost::asio::io_context io; Socket socket; socket.async_read_some(0, handler); }
"@
            }
            'poco' {
                @"
#include <Poco/Net/HTTPServer.h>
void handler(void) {}
void authMiddleware(void) {}
void Route(const char *path, void (*target)(void)) {}
struct App { void use(void (*target)(void)) {} };
void start(void) { App app; Route("/fixture", handler); app.use(authMiddleware); }
"@
            }
            'unreal-engine' {
                @"
#include "CoreMinimal.h"
#define UCLASS(...)
#define UFUNCTION(...)
struct AActor {};
UCLASS()
class AGameMode : public AActor { public: UFUNCTION() void OnEvent() {} };
"@
            }
            'drogon' {
                @"
#include <drogon/drogon.h>
#define ADD_METHOD_TO(handler, path, method)
void handler(void) {}
void authMiddleware(void) {}
struct App { void use(void (*target)(void)) {} };
void start(void) { App app; ADD_METHOD_TO(handler, "/fixture", Get); app.use(authMiddleware); }
"@
            }
            'crow' {
                @"
#include "crow.h"
#define CROW_ROUTE(app, path) RouteRegistration()
struct RouteRegistration { void operator()(void (*target)(void)) {} };
void handler(void) {}
void authMiddleware(void) {}
struct App { void use(void (*target)(void)) {} };
void start(void) { App app; CROW_ROUTE(app, "/fixture")(handler); app.use(authMiddleware); }
"@
            }
            'grpc' {
                @"
#include <grpcpp/grpcpp.h>
namespace grpc { struct Service {}; }
class UserService final : public grpc::Service {};
void RegisterService(grpc::Service *service) {}
void start(void) { UserService service; RegisterService(&service); }
"@
            }
            default { throw "No C/C++ fixture template for $Framework" }
        }
        $metadata = @{
            path = 'CMakeLists.txt'
            source = "cmake_minimum_required(VERSION 3.20)`nproject(framework_fixture)`n"
        }
        $files = @(
            @{ path = "src/fixture.$extension"; source = $source },
            $metadata
        )
        $compile = if ($Language -eq 'c') { 'clang -std=c17 -c src/fixture.c -o src/fixture.o' } else { 'clang++ -std=c++20 -c src/fixture.cpp -o src/fixture.o' }
        $compileDatabase = @(@{ directory = '.'; command = $compile; file = "src/fixture.$extension" })
        $files += @{ path = 'compile_commands.json'; source = (ConvertTo-Json -InputObject $compileDatabase -Compress) }
        return $files
    }

    if ($Language -eq 'go') {
        $source = switch ($Framework) {
            'net-http' {
                @"
package main
import "net/http"
func handler(w http.ResponseWriter, r *http.Request) { w.WriteHeader(http.StatusOK) }
func authMiddleware(w http.ResponseWriter, r *http.Request) {}
func start() { http.HandleFunc("/fixture", handler); http.Handle("/auth", http.HandlerFunc(authMiddleware)); http.ListenAndServe(":8080", nil) }
"@
            }
            'gin' {
                @"
package main
import _ "github.com/gin-gonic/gin"
type Router struct{}
func (Router) GET(path string, handler any) {}
func (Router) Use(handler any) {}
var router Router
func handler() {}
func authMiddleware() {}
func start() { router.Use(authMiddleware); router.GET("/fixture", handler) }
"@
            }
            'echo' {
                @"
package main
import _ "github.com/labstack/echo"
type Echo struct{}
func (Echo) GET(path string, handler any) {}
func (Echo) Use(handler any) {}
var e Echo
func handler() {}
func authMiddleware() {}
func start() { e.Use(authMiddleware); e.GET("/fixture", handler) }
"@
            }
            'fiber' {
                @"
package main
import _ "github.com/gofiber/fiber/v2"
type App struct{}
func (App) Get(path string, handler any) {}
func (App) Use(handler any) {}
var app App
func handler() {}
func authMiddleware() {}
func start() { app.Use(authMiddleware); app.Get("/fixture", handler) }
"@
            }
            'chi' {
                @"
package main
import _ "github.com/go-chi/chi/v5"
type Router struct{}
func (Router) Get(path string, handler any) {}
func (Router) Use(handler any) {}
var r Router
func handler() {}
func authMiddleware() {}
func start() { r.Use(authMiddleware); r.Get("/fixture", handler) }
"@
            }
            'beego' {
                @"
package main
import beego "github.com/beego/beego/v2"
type Controller struct{}
func handler() {}
func authMiddleware() {}
func start() { beego.Router("/fixture", &Controller{}, "get:handler"); beego.InsertFilter("/*", "Before", authMiddleware) }
"@
            }
            'grpc' {
                @"
package main
import grpc "google.golang.org/grpc"
type UserService struct{}
type Server struct{}
func (Server) RegisterService(service *UserService) {}
func start() { var server Server; service := &UserService{}; server.RegisterService(service); _ = grpc.NewServer() }
"@
            }
            default { throw "No Go fixture template for $Framework" }
        }
        return @(
            @{ path = 'main.go'; source = $source },
            @{ path = 'go.mod'; source = "module framework-fixture`n`ngo 1.23`n" }
        )
    }

    if ($Language -eq 'rust') {
        $source = switch ($Framework) {
            'axum' {
                @"
use axum::{routing::get, Router};
fn handler() -> &'static str { "ok" }
fn auth_middleware() {}
fn main() { let app = Router::new().route("/fixture", get(handler)).layer(auth_middleware); let _ = app; }
"@
            }
            'actix-web' {
                @"
use actix_web::{web, App, HttpServer};
fn handler() -> &'static str { "ok" }
fn auth_middleware() {}
fn main() { let app = App::new().route("/fixture", web::get().to(handler)).wrap(auth_middleware); let _ = app; }
"@
            }
            'rocket' {
                @"
use rocket::{get, launch};
fn middleware() {}
#[get("/fixture")]
fn handler() -> &'static str { "ok" }
#[launch]
fn rocket() -> rocket::Rocket<rocket::Build> { rocket::build() }
"@
            }
            'warp' {
                @"
use warp::Filter;
fn handler() -> Result<impl warp::Reply, warp::Rejection> { Ok("ok") }
fn auth_middleware() {}
fn main() { let route = warp::path("/fixture").and(warp::get()).and_then(handler).with(auth_middleware); let _ = route; }
"@
            }
            'poem' {
                @"
use poem::{get, handler, Route};
fn middleware() {}
#[handler]
async fn handler_fn() -> &'static str { "ok" }
fn main() { let route = Route::new().at("/fixture", get(handler_fn)).with(middleware); let _ = route; }
"@
            }
            'tokio' {
                @"
#[tokio::main]
async fn main() { tokio::spawn(task()); schedule("0 * * * *"); }
async fn task() {}
fn schedule(_cron: &str) {}
"@
            }
            'tonic' {
                @"
use tonic::transport::Server;
struct UserService {}
fn start() { Server::builder().add_service(UserService {}); tonic::include_proto!("users"); }
"@
            }
            'tauri' {
                @"
#[tauri::command]
fn list_sessions() -> Result<(), String> { Ok(()) }
fn main() { tauri::Builder::default().invoke_handler(tauri::generate_handler![list_sessions]); }
"@
            }
            default { throw "No Rust fixture template for $Framework" }
        }
        return @(
            @{ path = 'src/main.rs'; source = $source },
            @{ path = 'Cargo.toml'; source = "[package]`nname = `"framework-fixture`"`nversion = `"0.1.0`"`nedition = `"2021`"`n" }
        )
    }

    if ($Language -eq 'php') {
        $source = switch ($Framework) {
            'laravel' {
                @"
<?php
use Illuminate\Support\Facades\Route;
class UserService {}
class FixtureController { public function handler() {} }
class AuthMiddleware {}
Route::middleware("auth")->group(function () {});
Route::get("/fixture", "FixtureController::handler");
"@
            }
            'symfony' {
                @"
<?php
use Symfony\Component\Routing\Annotation\Route;
class AuthMiddleware {}
class UserService {}
class FixtureController {
    #[Route("/fixture")]
    public function handler() {}
}
"@
            }
            'codeigniter' {
                @"
<?php
namespace App;
use CodeIgniter\Controller;
class Handler { public function index() {} }
class AuthMiddleware {}
function register(`$routes) { `$routes->get("/fixture", "Handler::index"); }
"@
            }
            'laminas' {
                @"
<?php
use Laminas\Router\Http\TreeRouteStack;
class FixtureController { public function handler() {} }
class AuthMiddleware {}
function register(`$router) { `$router->addRoute("fixture", "/fixture", "FixtureController::handler"); }
"@
            }
            'slim' {
                @"
<?php
use Slim\App;
class FixtureController { public function handler(`$request, `$response) { return `$response; } }
class AuthMiddleware {}
`$app = new App();
`$app->addMiddleware(new AuthMiddleware());
`$app->get("/fixture", "FixtureController::handler");
"@
            }
            'cakephp' {
                @"
<?php
use Cake\Routing\Router;
class FixtureController { public function handler() {} }
class AuthMiddleware {}
Router::get("/fixture", "FixtureController::handler");
"@
            }
            'api-platform' {
                @"
<?php
use ApiPlatform\Metadata\ApiResource;
use ApiPlatform\Metadata\Get;
#[ApiResource]
class User {}
#[Get("/fixture")]
class UserEndpoint {}
"@
            }
            default { throw "No PHP fixture template for $Framework" }
        }
        $files = @(
            @{ path = 'src/Fixture.php'; source = $source },
            @{ path = 'composer.json'; source = '{"name":"framework/fixture","require":{"php":">=8.1","nikic/php-parser":"^5.0"},"autoload":{"files":["main.php"],"classmap":["src"],"psr-4":{"Fixture\\":"src/"}}}' }
        )
        if ($Framework -eq 'codeigniter') {
            $files[1].source = $files[1].source.Replace('"classmap":["src"]', '"classmap":["src","app"]')
            $files += @{ path = 'app/Config/Routes.php'; source = @'
<?php
function register_routes($routes) { $routes->get("/fixture", "Handler::index"); }
'@ }
        }
        return $files
    }

    if ($Language -eq 'ruby') {
        $source = switch ($Framework) {
            'rails' {
                @'
require "rails"
class ApplicationController < ActionController::Base
  def handler
  end
end
class UserService
end
class AuthMiddleware
end
use AuthMiddleware
'@
            }
            'sinatra' {
                @'
require "sinatra/base"
class App < Sinatra::Base
  get "/fixture", :handler
  def handler
  end
  use AuthMiddleware
end
class AuthMiddleware
end
'@
            }
            'hanami' {
                @'
require "hanami"
class Handler < Hanami::Action
  def handler
  end
  def call(request)
  end
end
get "/fixture", handler
class AuthMiddleware
end
use AuthMiddleware
'@
            }
            'rack' {
                @'
require "rack"
class App < Rack::Builder
  def self.handler
  end
end
map "/fixture", handler
class AuthMiddleware
end
use AuthMiddleware
run App
'@
            }
            'grape' {
                @'
require "grape"
class API < Grape::API
  resource "/"
  get "/fixture", handler
  def handler
  end
end
class AuthMiddleware
end
use AuthMiddleware
'@
            }
            'roda' {
                @'
require "roda"
class App < Roda
  route do |r|
    r.on "/fixture", handler
  end
end
def handler
end
class AuthMiddleware
end
app.use(AuthMiddleware)
'@
            }
            default { throw "No Ruby fixture template for $Framework" }
        }
        $gem = @"
source "https://rubygems.org"
gem "$Framework"
"@
        $files = @(
            @{ path = 'src/fixture.rb'; source = $source },
            @{ path = 'Gemfile'; source = $gem }
        )
        if ($Framework -eq 'rails') {
            $files += @{ path = 'config/routes.rb'; source = 'get "/fixture", to: "handler"' }
        }
        return $files
    }

    if ($Language -eq 'dart') {
        $source = switch ($Framework) {
            'flutter' {
                @'
import 'package:flutter/material.dart';

void handleTap() {}

class Home extends StatelessWidget {
  const Home({super.key});
  @override
  Widget build(BuildContext context) {
    return GestureDetector(onTap: handleTap, child: const Text('ok'));
  }
}
'@
            }
            'shelf' {
                @'
import 'package:shelf/shelf.dart';
import 'package:shelf_router/shelf_router.dart';

Response handler(Request request) => Response.ok('ok');
Middleware authMiddleware(Middleware next) => next;
final router = Router();
router.get('/fixture', handler);
final app = Pipeline().addMiddleware(authMiddleware).addHandler(router);
'@
            }
            'serverpod' {
                @'
import 'package:serverpod/serverpod.dart';

class UserEndpoint extends Endpoint {
  Future<void> getUser(Session session) async {}
}
'@
            }
            'dart-frog' {
                @'
import 'package:dart_frog/dart_frog.dart';

Response onRequest(Request request) => Response.text('ok');
Middleware authMiddleware(Middleware next) => next;
final app = Pipeline().addMiddleware(authMiddleware);
'@
            }
            default { throw "No Dart fixture template for $Framework" }
        }
        $pubspec = @"
name: code_memory_framework_fixture
environment:
  sdk: '>=3.0.0 <4.0.0'
dependencies:
  ${Framework}: ^1.0.0
"@
        $path = if ($Framework -eq 'dart-frog') { 'routes/fixture.dart' } else { 'src/fixture.dart' }
        return @(
            @{ path = $path; source = $source },
            @{ path = 'pubspec.yaml'; source = $pubspec }
        )
    }

    throw "Provider fixture template not implemented yet: $Language/$Framework"
}

function Write-Project {
    param([string]$Path, $Files)
    if (Test-Path $Path) {
        Remove-Item -LiteralPath $Path -Recurse -Force
    }
    $utf8NoBom = [System.Text.UTF8Encoding]::new($false)
    foreach ($file in @($Files)) {
        $target = Join-Path $Path ($file.path -replace '/', '\')
        New-Item -ItemType Directory -Force -Path (Split-Path $target) | Out-Null
        [System.IO.File]::WriteAllText($target, [string]$file.source, $utf8NoBom)
    }
}

$frameworkRoot = Join-Path $Root 'packs\framework'
$catalog = Get-Content (Join-Path $frameworkRoot 'catalog.json') -Raw | ConvertFrom-Json
$languages = if ($Language -eq 'all') { @($catalog.languages.id) } else { @($Language) }
$bridgePath = (Resolve-Path $Bridge).Path
$gateRoot = Join-Path $Root 'build\framework-provider-gate'
$projectRoot = Join-Path $Root 'build\framework-provider-fixtures'
New-Item -ItemType Directory -Force -Path $gateRoot,$projectRoot | Out-Null
$passed = 0
$total = 0

foreach ($languageId in $languages) {
    $languageSpec = $catalog.languages | Where-Object id -eq $languageId
    if ($null -eq $languageSpec) { throw "Unknown framework language: $languageId" }
    $languageCatalogPath = Join-Path $frameworkRoot $languageSpec.file
    $languageCatalog = Get-Content $languageCatalogPath -Raw | ConvertFrom-Json
    foreach ($packRef in $languageCatalog.packs) {
        if ($Framework -ne 'all' -and $packRef.id -ne $Framework) { continue }
        $total++
        $packPath = Join-Path (Split-Path $languageCatalogPath) $packRef.path
        $packFixture = Get-Content (Join-Path (Split-Path $packPath) 'fixture.json') -Raw | ConvertFrom-Json
        $project = Join-Path $projectRoot "$languageId-$($packRef.id)"
        $out = Join-Path $gateRoot "$languageId-$($packRef.id).json"
        $files = New-ProjectFiles -Language $languageId -Framework $packRef.id -PackFixture $packFixture
        Write-Project -Path $project -Files $files
        if ($languageId -eq 'dart') {
            # The bundled analyzer intentionally refuses to start without resolved
            # package metadata. This fixture has no dependency installation step,
            # so provide the smallest valid local-only package config.
            $dartTool = Join-Path $project '.dart_tool'
            New-Item -ItemType Directory -Force -Path $dartTool | Out-Null
            Set-Content -LiteralPath (Join-Path $dartTool 'package_config.json') -Value '{"configVersion":2,"packages":[]}' -NoNewline
        }
        if ($languageId -eq 'php') {
            $vendorSource = Join-Path $Root 'tests\fixtures\scip-php\vendor'
            Copy-Item -LiteralPath $vendorSource -Destination (Join-Path $project 'vendor') -Recurse -Force
            Copy-Item -LiteralPath (Join-Path $Root 'tests\fixtures\scip-php\main.php') -Destination (Join-Path $project 'main.php') -Force
            Copy-Item -LiteralPath (Join-Path $Root 'tests\fixtures\scip-php\src\Types.php') -Destination (Join-Path $project 'src\Types.php') -Force
            Copy-Item -LiteralPath (Join-Path $Root 'tests\fixtures\scip-php\composer.lock') -Destination (Join-Path $project 'composer.lock') -Force
        }
        $args = @('index','--root',$project,'--out',$out,'--packs-root',$Root)
        if (-not [string]::IsNullOrWhiteSpace($ProvidersRoot)) { $args += @('--providers-root',$ProvidersRoot) }
        # Provider stderr contains expected external-header diagnostics in C/C++
        # fixtures; the bridge status and resulting index are the gate signals.
        $bridgeErrorAction = $ErrorActionPreference
        $ErrorActionPreference = 'Continue'
        & $bridgePath @args 2>&1 | Out-Null
        $bridgeExitCode = $LASTEXITCODE
        $ErrorActionPreference = $bridgeErrorAction
        if ($bridgeExitCode -ne 0 -or -not (Test-Path $out)) { throw "Bridge failed for $languageId/$($packRef.id)" }
        $result = Get-Content $out -Raw | ConvertFrom-Json
        $languageResult = @($result.languages | Where-Object id -eq $languageId) | Select-Object -First 1
        if ($null -eq $languageResult -or $languageResult.status -ne 'indexed') {
            throw "$languageId/$($packRef.id) provider did not index: $($languageResult.status)"
        }
        $pack = @($result.frameworks | Where-Object { $_.id -eq $packRef.id -and $_.language -eq $languageId }) | Select-Object -First 1
        if ($null -eq $pack -or $pack.status -ne 'detected') { throw "$languageId/$($packRef.id) was not detected" }
        foreach ($factKind in @($packFixture.expected.facts)) {
            $facts = @($pack.facts | Where-Object kind -eq $factKind)
            if ($facts.Count -eq 0) {
                # Java component packs intentionally canonicalize duplicate facts
                # across Spring/Spring Boot. Accept the canonical owner only when
                # its source file is one of this pack's detected files.
                $facts = @(
                    $result.frameworks |
                        Where-Object { $_.language -eq $languageId } |
                        ForEach-Object { $_.facts } |
                        Where-Object {
                            $_.kind -eq $factKind -and
                            $pack.files -contains $_.source_file
                        }
                )
            }
            if ($facts.Count -eq 0) { throw "$languageId/$($packRef.id) did not emit $factKind" }
            if ($factKind -eq 'HTTP_ROUTE' -and @($facts | Where-Object {
                    [string]::IsNullOrWhiteSpace([string]$_.method) -or
                    [string]::IsNullOrWhiteSpace([string]$_.path)
                }).Count -gt 0) {
                throw "$languageId/$($packRef.id) has an HTTP route without method/path"
            }
            if (@($facts | Where-Object { @($_.source_range).Count -ne 4 }).Count -ne 0) { throw "$languageId/$($packRef.id) has invalid $factKind range" }
            $unresolved = @($facts | Where-Object {
                [string]::IsNullOrWhiteSpace([string]$_.symbol) -and
                $_.properties.resolution -ne 'framework_alias'
            })
            if ($unresolved.Count -gt 0) {
                throw "$languageId/$($packRef.id) has unresolved $factKind without an explicit alias resolution"
            }
            foreach ($fact in $facts) {
                if ([string]::IsNullOrWhiteSpace([string]$fact.source_file)) {
                    throw "$languageId/$($packRef.id) has a $factKind fact without source_file"
                }
                $factPath = Join-Path $project ($fact.source_file -replace '/', '\')
                if (-not (Test-Path -LiteralPath $factPath -PathType Leaf)) {
                    throw "$languageId/$($packRef.id) points $factKind at missing $($fact.source_file)"
                }
                $range = @($fact.source_range)
                if ($range[0] -lt 0 -or $range[1] -lt 0 -or $range[2] -lt $range[0]) {
                    throw "$languageId/$($packRef.id) has invalid $factKind source range"
                }
                if ($fact.source_line -ne ($range[0] + 1)) {
                    throw "$languageId/$($packRef.id) has mismatched $factKind source line/range"
                }
            }
        }
        $expectsHandles = @($packFixture.expected.relations | Where-Object { $_ -eq 'HANDLES' }).Count -gt 0
        if ($expectsHandles) {
            $routes = @($pack.facts | Where-Object { $_.kind -in @('HTTP_ROUTE', 'RPC_ENDPOINT') })
            if ($routes.Count -eq 0) { throw "$languageId/$($packRef.id) expects HANDLES without a route/RPC fact" }
            if (@($routes | Where-Object { [string]::IsNullOrWhiteSpace([string]$_.symbol) }).Count -gt 0 -and
                @($routes | Where-Object { $_.kind -eq 'HTTP_ROUTE' }).Count -gt 0) {
                throw "$languageId/$($packRef.id) has an HTTP route without a resolved provider symbol"
            }
            foreach ($route in @($routes | Where-Object { -not [string]::IsNullOrWhiteSpace([string]$_.symbol) })) {
                if (@($result.framework_relations | Where-Object {
                    $_.framework -eq $packRef.id -and $_.kind -eq 'HANDLES' -and $_.to -eq $route.id
                }).Count -eq 0) {
                    throw "$languageId/$($packRef.id) has a resolved $($route.kind) without HANDLES"
                }
            }
        }
        foreach ($relationKind in @($packFixture.expected.relations)) {
            if (@($result.framework_relations | Where-Object { $_.framework -eq $packRef.id -and $_.kind -eq $relationKind }).Count -eq 0) {
                throw "$languageId/$($packRef.id) did not emit $relationKind"
            }
        }
        $passed++
        Write-Host "PASS $languageId/$($packRef.id)"
    }
}

Write-Host "framework provider gate: passed=$passed total=$total"
