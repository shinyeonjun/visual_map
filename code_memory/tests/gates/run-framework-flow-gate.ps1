param(
    [string]$Bridge = (Join-Path $PSScriptRoot '..\..\rust\target\release\code-memory-language.exe'),
    [string]$Root = (Join-Path $PSScriptRoot '..\..'),
    [string]$ProvidersRoot = '',
    [string]$OutputRoot = '',
    [string]$CaseId = ''
)

. (Join-Path $PSScriptRoot 'lib\language-ir-stream-authority.ps1')

$ErrorActionPreference = 'Stop'
if (Get-Variable PSNativeCommandUseErrorActionPreference -ErrorAction SilentlyContinue) {
    $PSNativeCommandUseErrorActionPreference = $false
}
$Root = (Resolve-Path $Root).Path
$Bridge = (Resolve-Path $Bridge).Path
$bundledProviders = Join-Path $Root 'providers'
if ([string]::IsNullOrWhiteSpace($ProvidersRoot) -and (Test-Path $bundledProviders)) {
    $ProvidersRoot = (Resolve-Path $bundledProviders).Path
}
if ([string]::IsNullOrWhiteSpace($OutputRoot)) {
    $OutputRoot = Join-Path $Root 'build\framework-flow-gate'
}

function Write-Project {
    param([string]$Path, [object[]]$Files)
    if (Test-Path $Path) { Remove-Item -LiteralPath $Path -Recurse -Force }
    $utf8NoBom = [System.Text.UTF8Encoding]::new($false)
    foreach ($file in $Files) {
        $target = Join-Path $Path ($file.path -replace '/', '\')
        New-Item -ItemType Directory -Force -Path (Split-Path $target) | Out-Null
        [System.IO.File]::WriteAllText($target, [string]$file.source, $utf8NoBom)
    }
}

function Invoke-FlowCase {
    param([pscustomobject]$Case)

    $project = Join-Path $OutputRoot $Case.id
    $out = Join-Path $OutputRoot "$($Case.id).json"
    Write-Project $project $Case.files
    if ($Case.language -eq 'dart') {
        $dartTool = Join-Path $project '.dart_tool'
        New-Item -ItemType Directory -Force -Path $dartTool | Out-Null
        Set-Content -LiteralPath (Join-Path $dartTool 'package_config.json') -Value '{"configVersion":2,"packages":[]}' -NoNewline
    }
    $arguments = @('index', '--root', $project, '--out', $out, '--packs-root', $Root)
    if (-not [string]::IsNullOrWhiteSpace($ProvidersRoot)) {
        $arguments += @('--providers-root', $ProvidersRoot)
    }
    $bridgeErrorAction = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    $bridgeOutput = @(& $Bridge @arguments 2>&1)
    $bridgeExitCode = $LASTEXITCODE
    $ErrorActionPreference = $bridgeErrorAction
    if ($bridgeExitCode -ne 0 -or -not (Test-Path $out)) {
        $detail = ($bridgeOutput | ForEach-Object { $_.ToString() }) -join "`n"
        throw "$($Case.id): bridge failed`n$detail"
    }

    $result = Get-Content $out -Raw | ConvertFrom-Json
    $language = @($result.languages | Where-Object id -eq $Case.language) | Select-Object -First 1
    if ($null -eq $language -or $language.status -ne 'indexed') {
        throw "$($Case.id): language was not indexed ($($language.status))"
    }
    $framework = @($result.frameworks | Where-Object {
            $_.id -eq $Case.framework -and $_.language -eq $Case.language
        }) | Select-Object -First 1
    if ($null -eq $framework -or $framework.status -ne 'detected') {
        throw "$($Case.id): framework was not detected"
    }

    $entrypointKind = if ([string]::IsNullOrWhiteSpace([string]$Case.entrypointKind)) { 'HTTP_ROUTE' } else { $Case.entrypointKind }
    $relationKind = if ([string]::IsNullOrWhiteSpace([string]$Case.relationKind)) { 'HANDLES' } else { $Case.relationKind }
    $entrypoints = @($framework.facts | Where-Object {
            if ($entrypointKind -eq 'HTTP_ROUTE') {
                $_.kind -eq $entrypointKind -and $_.path -eq $Case.route
            } else {
                $_.kind -eq $entrypointKind
            }
        })
    if ($entrypoints.Count -ne 1) {
        $description = if ($entrypointKind -eq 'HTTP_ROUTE') { $Case.route } else { $entrypointKind }
        throw "$($Case.id): expected one $description entrypoint, found $($entrypoints.Count)"
    }
    $entrypoint = $entrypoints[0]
    if ([string]::IsNullOrWhiteSpace([string]$entrypoint.symbol)) {
        throw "$($Case.id): entrypoint handler symbol is unresolved"
    }

    $handles = @($result.framework_relations | Where-Object {
            $_.framework -eq $Case.framework -and
            $_.kind -eq $relationKind -and
            $_.to -eq $entrypoint.id -and
            $_.from -eq $entrypoint.symbol
        })
    if ($handles.Count -ne 1) {
        throw "$($Case.id): entrypoint does not have exactly one $relationKind relation"
    }

    if ($entrypointKind -eq 'HTTP_ROUTE') {
        $languageIr = Get-TaggedJsonReceipt -BridgeOutput $bridgeOutput `
            -Prefix '@codebase-workspace-language-ir ' -Context $Case.id
        if ($languageIr.schema -ne 'codebase-workspace.language-ir-migration-receipt.v6') {
            throw "$($Case.id): unsupported Language IR receipt schema $($languageIr.schema)"
        }
        $null = Get-LanguageIrStreamAuthority -BridgeOutput $bridgeOutput `
            -Receipt $languageIr -Context $Case.id
        $frameworkIr = Get-TaggedJsonReceipt -BridgeOutput $bridgeOutput `
            -Prefix '@codebase-workspace-framework-ir ' -Context $Case.id
        $canonical = Get-TaggedJsonReceipt -BridgeOutput $bridgeOutput `
            -Prefix '@codebase-workspace-canonical-linker ' -Context $Case.id
        if ($frameworkIr.schema -ne 'codebase-workspace.framework-ir.v1') {
            throw "$($Case.id): unsupported Framework IR schema $($frameworkIr.schema)"
        }
        if ([int64]$frameworkIr.plannedRouteRecordCount -ne
            ([int64]$frameworkIr.emittedRouteRecordCount + [int64]$frameworkIr.rejectedRouteRecordCount)) {
            throw "$($Case.id): Framework IR route accounting is inconsistent"
        }
        if ([int64]$frameworkIr.emittedRouteRecordCount -lt 1 -or
            [int64]$frameworkIr.handlerReferenceCount -lt 1) {
            throw "$($Case.id): reviewed route did not enter typed Framework IR"
        }
        if ([int64]$canonical.frameworkRouteNodeCount -lt 1 -or
            [int64]$canonical.frameworkExposesEdgeCount -lt 1 -or
            [int64]$canonical.frameworkHandlesEdgeCount -lt 1) {
            throw "$($Case.id): reviewed route did not enter the canonical Fact Graph"
        }
    }

    $calls = @($result.relations | Where-Object {
            $_.kind -eq 'CALLS' -and
            $_.from -eq $entrypoint.symbol -and
            $_.to -match [regex]::Escape($Case.service)
        })
    if ($calls.Count -ne 1) {
        throw "$($Case.id): handler does not have exactly one CALLS relation to $($Case.service), found $($calls.Count)"
    }
    if (@($calls[0].range).Count -lt 3 -or [string]::IsNullOrWhiteSpace([string]$calls[0].path)) {
        throw "$($Case.id): service CALLS relation has no source range/path"
    }
    $serviceDocument = @($result.documents | Where-Object {
            $_.path -eq $Case.servicePath
        }) | Select-Object -First 1
    if ($null -eq $serviceDocument) {
        throw "$($Case.id): service document $($Case.servicePath) is missing"
    }
    $duplicateCalls = @($result.relations |
        Where-Object { $_.kind -eq 'CALLS' } |
        Group-Object { "$($_.from)|$($_.to)|$($_.path)|$($_.range -join ',')" } |
        Where-Object Count -gt 1)
    if ($duplicateCalls.Count -gt 0) {
        throw "$($Case.id): duplicate CALLS relation was emitted"
    }
    Write-Host "PASS $($Case.id): entrypoint=$($entrypoint.symbol) service=$($calls[0].to)"
}

$cases = @(
    [pscustomobject]@{
        id = 'javascript-express'
        language = 'javascript'
        framework = 'express'
        route = '/orders'
        service = 'createOrder'
        servicePath = 'src/service.js'
        files = @(
            @{ path = 'src/routes.js'; source = @'
const express = require("express");
const { createOrder } = require("./service.js");
const app = express();

function handler(request, response) {
  return createOrder(request.body);
}

app.get("/orders", handler);
module.exports = app;
'@ },
            @{ path = 'src/service.js'; source = @'
function createOrder(input) {
  return { ok: true, input };
}

module.exports = { createOrder };
'@ },
            @{ path = 'package.json'; source = '{"name":"flow-fixture","version":"1.0.0","dependencies":{"express":"4.21.2"}}' },
            @{ path = 'tsconfig.json'; source = '{"compilerOptions":{"allowJs":true,"checkJs":false,"module":"NodeNext","moduleResolution":"NodeNext","target":"ES2022"},"include":["src/**/*"]}' }
        )
    },
    [pscustomobject]@{
        id = 'typescript-express'
        language = 'typescript'
        framework = 'express'
        route = '/orders'
        service = 'createOrder'
        servicePath = 'src/service.ts'
        files = @(
            @{ path = 'src/routes.ts'; source = @'
import express from "express";
import { createOrder } from "./service";
const app = express();

function handler(request: express.Request, response: express.Response) {
  return createOrder(request.body);
}

app.get("/orders", handler);
export default app;
'@ },
            @{ path = 'src/service.ts'; source = @'
export function createOrder(input: unknown) {
  return { ok: true, input };
}
'@ },
            @{ path = 'package.json'; source = '{"name":"flow-fixture","version":"1.0.0","dependencies":{"express":"4.21.2"},"devDependencies":{"@types/express":"5.0.0"}}' },
            @{ path = 'tsconfig.json'; source = '{"compilerOptions":{"module":"NodeNext","moduleResolution":"NodeNext","target":"ES2022","esModuleInterop":true,"strict":false},"include":["src/**/*"]}' }
        )
    },
    [pscustomobject]@{
        id = 'go-gin'
        language = 'go'
        framework = 'gin'
        route = '/orders'
        service = 'CreateOrder'
        servicePath = 'service/service.go'
        files = @(
            @{ path = 'main.go'; source = @'
package main

import (
  _ "github.com/gin-gonic/gin"
  "flowfixture/service"
)

type Router struct{}
func (Router) GET(path string, handler any) {}
var router Router

func handler() { service.CreateOrder() }
func start() { router.GET("/orders", handler) }
'@ },
            @{ path = 'service/service.go'; source = @'
package service

func CreateOrder() string { return "ok" }
'@ },
            @{ path = 'go.mod'; source = "module flowfixture`n`ngo 1.23`n" }
        )
    },
    [pscustomobject]@{
        id = 'rust-axum'
        language = 'rust'
        framework = 'axum'
        route = '/orders'
        service = 'create_order'
        servicePath = 'src/service.rs'
        files = @(
            @{ path = 'src/main.rs'; source = @'
mod service;
use axum::{routing::get, Router};

fn handler() -> &'static str {
    service::create_order()
}

fn main() {
    let app = Router::new().route("/orders", get(handler));
    let _ = app;
}
'@ },
            @{ path = 'src/service.rs'; source = @'
pub fn create_order() -> &'static str {
    "ok"
}
'@ },
            @{ path = 'Cargo.toml'; source = @'
[package]
name = "flow-fixture"
version = "0.1.0"
edition = "2021"
'@ }
        )
    },
    [pscustomobject]@{
        id = 'cpp-crow'
        language = 'cpp'
        framework = 'crow'
        route = '/orders'
        service = 'create_order'
        servicePath = 'src/service.cpp'
        files = @(
            @{ path = 'src/routes.cpp'; source = @'
#include "crow.h"
#include "service.h"
#define CROW_ROUTE(app, path) RouteRegistration()
struct RouteRegistration { void operator()(void (*target)(void)) {} };

void handler(void) { create_order(); }
void start(void) {
  App app;
  CROW_ROUTE(app, "/orders")(handler);
}
'@ },
            @{ path = 'src/service.h'; source = @'
void create_order(void);
'@ },
            @{ path = 'src/service.cpp'; source = @'
#include "service.h"
void create_order(void) {}
'@ },
            @{ path = 'CMakeLists.txt'; source = "cmake_minimum_required(VERSION 3.20)`nproject(flow_fixture)`n" },
            @{ path = 'compile_commands.json'; source = '[{"directory":".","command":"clang++ -std=c++20 -Isrc -c src/routes.cpp -o src/routes.o","file":"src/routes.cpp"},{"directory":".","command":"clang++ -std=c++20 -Isrc -c src/service.cpp -o src/service.o","file":"src/service.cpp"}]' }
        )
    },
    [pscustomobject]@{
        id = 'csharp-aspnet-core'
        language = 'csharp'
        framework = 'minimal-api'
        route = '/orders'
        service = 'CreateOrder'
        servicePath = 'OrderService.cs'
        files = @(
            @{ path = 'Program.cs'; source = @'
using Microsoft.AspNetCore.Builder;

public class Program {
    private static OrderService service = new OrderService();
    public static void Main() {
        var builder = WebApplication.CreateBuilder();
        var app = builder.Build();
        app.MapGet("/orders", Handler);
    }
    public static string Handler() { return service.CreateOrder(); }
}
'@ },
            @{ path = 'OrderService.cs'; source = @'
public class OrderService {
    public string CreateOrder() { return "ok"; }
}
'@ },
            @{ path = 'flow-fixture.csproj'; source = '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net8.0</TargetFramework><EnableDefaultCompileItems>true</EnableDefaultCompileItems></PropertyGroup></Project>' },
            @{ path = 'flow-fixture.sln'; source = @'
Microsoft Visual Studio Solution File, Format Version 12.00
# Visual Studio Version 17
Project("{FAE04EC0-301F-11D1-9B4B-00C04FC2DCD2}") = "flow-fixture", "flow-fixture.csproj", "{11111111-1111-1111-1111-111111111111}"
EndProject
'@ }
        )
    },
    [pscustomobject]@{
        id = 'dart-shelf'
        language = 'dart'
        framework = 'shelf'
        route = '/orders'
        service = 'createOrder'
        servicePath = 'src/service.dart'
        files = @(
            @{ path = 'src/fixture.dart'; source = @'
import 'package:shelf/shelf.dart';
import 'package:shelf_router/shelf_router.dart';
import 'service.dart';

Response handler(Request request) => Response.ok(createOrder());
final router = Router();
router.get('/orders', handler);
'@ },
            @{ path = 'src/service.dart'; source = @'
String createOrder() => 'ok';
'@ },
            @{ path = 'pubspec.yaml'; source = @'
name: flow_fixture
environment:
  sdk: '>=3.0.0 <4.0.0'
dependencies:
  shelf: ^1.0.0
  shelf_router: ^1.0.0
'@ }
        )
    },
    [pscustomobject]@{
        id = 'c-gtk-glib'
        language = 'c'
        framework = 'gtk-glib'
        entrypointKind = 'EVENT_HANDLER'
        relationKind = 'HANDLES_EVENT'
        service = 'create_order'
        servicePath = 'src/service.c'
        files = @(
            @{ path = 'src/ui.c'; source = @'
#include <gtk/gtk.h>
#include "service.h"
typedef struct GtkWidget GtkWidget;

void callback(GtkWidget *widget, void *data) {
    create_order();
}

void build_ui(void) {
    GtkWidget *button = 0;
    g_signal_connect(button, "clicked", callback);
}
'@ },
            @{ path = 'src/service.h'; source = @'
void create_order(void);
'@ },
            @{ path = 'src/service.c'; source = @'
#include "service.h"
void create_order(void) {}
'@ },
            @{ path = 'compile_commands.json'; source = '[{"directory":".","command":"clang -std=c17 -Isrc -c src/ui.c -o src/ui.o","file":"src/ui.c"},{"directory":".","command":"clang -std=c17 -Isrc -c src/service.c -o src/service.o","file":"src/service.c"}]' }
        )
    },
    [pscustomobject]@{
        id = 'python-fastapi'
        language = 'python'
        framework = 'fastapi'
        route = '/orders'
        service = 'create_order'
        servicePath = 'src/service.py'
        files = @(
            @{ path = 'src/__init__.py'; source = '' },
            @{ path = 'src/routes.py'; source = @'
from fastapi import FastAPI
from .service import create_order

app = FastAPI()

@app.get("/orders")
def handler():
    return create_order()
'@ },
            @{ path = 'src/service.py'; source = @'
def create_order():
    return {"ok": True}
'@ },
            @{ path = 'pyproject.toml'; source = @'
[project]
name = "flow-fixture"
version = "0.1.0"
dependencies = ["fastapi"]
'@ }
        )
    },
    [pscustomobject]@{
        id = 'java-spring-mvc'
        language = 'java'
        framework = 'spring-mvc'
        route = '/orders'
        service = 'createOrder'
        servicePath = 'src/main/java/fixture/OrderService.java'
        files = @(
            @{ path = 'src/main/java/fixture/OrderController.java'; source = @'
package fixture;

import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.RestController;

@RestController
public class OrderController {
    private final OrderService service;

    public OrderController(OrderService service) {
        this.service = service;
    }

    @GetMapping("/orders")
    public String handler() {
        return service.createOrder();
    }
}
'@ },
            @{ path = 'src/main/java/fixture/OrderService.java'; source = @'
package fixture;

import org.springframework.stereotype.Service;

@Service
public class OrderService {
    public String createOrder() {
        return "ok";
    }
}
'@ },
            @{ path = 'pom.xml'; source = @'
<project>
  <modelVersion>4.0.0</modelVersion>
  <groupId>fixture</groupId>
  <artifactId>flow-fixture</artifactId>
  <version>0.1.0</version>
  <properties>
    <maven.compiler.release>21</maven.compiler.release>
  </properties>
  <dependencies>
    <dependency>
      <groupId>org.springframework</groupId>
      <artifactId>spring-web</artifactId>
      <version>6.1.8</version>
    </dependency>
  </dependencies>
</project>
'@ },
            @{ path = '.project'; source = @'
<?xml version="1.0" encoding="UTF-8"?>
<projectDescription>
  <name>flow-fixture</name>
  <buildSpec>
    <buildCommand><name>org.eclipse.jdt.core.javabuilder</name></buildCommand>
    <buildCommand><name>org.eclipse.m2e.core.maven2Builder</name></buildCommand>
  </buildSpec>
  <natures>
    <nature>org.eclipse.jdt.core.javanature</nature>
    <nature>org.eclipse.m2e.core.maven2Nature</nature>
  </natures>
</projectDescription>
'@ },
            @{ path = '.classpath'; source = @'
<?xml version="1.0" encoding="UTF-8"?>
<classpath>
  <classpathentry kind="src" path="src/main/java" output="target/classes" />
  <classpathentry kind="con" path="org.eclipse.jdt.launching.JRE_CONTAINER/org.eclipse.jdt.internal.debug.ui.launcher.StandardVMType/JavaSE-21" />
  <classpathentry kind="output" path="target/classes" />
</classpath>
'@ }
        )
    }
)

$selectedCases = if ([string]::IsNullOrWhiteSpace($CaseId)) {
    @($cases)
} else {
    @($cases | Where-Object id -eq $CaseId)
}
if ($selectedCases.Count -eq 0) { throw "Unknown framework flow case: $CaseId" }
if (Test-Path $OutputRoot) { Remove-Item -LiteralPath $OutputRoot -Recurse -Force }
New-Item -ItemType Directory -Force -Path $OutputRoot | Out-Null
foreach ($case in $selectedCases) { Invoke-FlowCase $case }
Write-Host "framework flow gate: passed=$($selectedCases.Count) total=$($selectedCases.Count)"
