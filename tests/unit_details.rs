use code_analysis_engine::config::AnalysisConfig;
use code_analysis_engine::facts::{
    CodeUnitKind, CodeUnitVisibility, FactStore, ReferenceKind, ResourceKind,
};
use code_analysis_engine::languages::analyze_file;
use code_analysis_engine::model::{FileEntry, Language, ParseStatus};
use std::collections::HashSet;

fn file(path: &str, language: Language) -> FileEntry {
    FileEntry {
        file_id: format!("file:{path}"),
        relative_path: path.to_string(),
        language,
        size_bytes: 0,
        line_count: 20,
        modified_unix_ms: None,
        content_hash: None,
        is_test: false,
        parse_status: Default::default(),
    }
}

#[test]
fn typescript_유닛이_계층과_상세_시그니처를_보존한다() {
    let source = r#"
export class AuthService {
  public async login(request: LoginRequest): Promise<Session> {
    return createSession(request);
  }
}
"#;
    let bundle = analyze_file(
        &file("src/auth/AuthService.ts", Language::TypeScript),
        source,
        &AnalysisConfig::default(),
    );

    let class_unit = bundle
        .units
        .iter()
        .find(|unit| unit.name == "AuthService")
        .expect("클래스 유닛이 추출되어야 한다");
    assert_eq!(class_unit.kind, CodeUnitKind::Class);
    assert!(class_unit.qualified_name.ends_with("::AuthService"));

    let login = bundle
        .units
        .iter()
        .find(|unit| unit.name == "login")
        .expect("메서드 유닛이 추출되어야 한다");
    assert_eq!(login.kind, CodeUnitKind::Method);
    assert_eq!(login.parent_id.as_deref(), Some(class_unit.id.as_str()));
    assert!(login.qualified_name.ends_with("::AuthService::login"));
    assert!(login
        .signature
        .as_deref()
        .is_some_and(|signature| signature.contains("login")));
    assert_eq!(login.parameters.len(), 1);
    assert_eq!(login.parameters[0].name, "request");
    assert_eq!(
        login.parameters[0].type_annotation.as_deref(),
        Some("LoginRequest")
    );
    assert_eq!(login.return_type.as_deref(), Some("Promise<Session>"));
    assert_eq!(login.visibility, CodeUnitVisibility::Public);
    assert!(login.modifiers.iter().any(|modifier| modifier == "async"));
    assert!(login.body_span.is_some());
}

#[test]
fn tsx는_typescript가_아닌_tsx_grammar으로_분석된다() {
    let source = r#"
type Props = { name: string };
export function UserCard(props: Props) {
  return <div>{props.name}</div>;
}
"#;
    let bundle = analyze_file(
        &file("src/UserCard.tsx", Language::TypeScript),
        source,
        &AnalysisConfig::default(),
    );

    assert_eq!(bundle.parse_status, ParseStatus::Parsed);
    assert!(bundle.units.iter().any(|unit| unit.name == "UserCard"));
    assert!(bundle
        .units
        .iter()
        .any(|unit| unit.kind == CodeUnitKind::TypeAlias && unit.name == "Props"));
}

#[test]
fn 같은_이름의_함수도_위치로_서로_다른_유닛이_된다() {
    let source = "function process(value: string) { return value; }\nfunction process(value: number) { return value; }\n";
    let bundle = analyze_file(
        &file("src/process.ts", Language::TypeScript),
        source,
        &AnalysisConfig::default(),
    );
    let functions: Vec<_> = bundle
        .units
        .iter()
        .filter(|unit| unit.name == "process")
        .collect();
    let ids: HashSet<_> = functions.iter().map(|unit| unit.id.as_str()).collect();

    assert_eq!(functions.len(), 2);
    assert_eq!(ids.len(), 2);
    assert_ne!(functions[0].span.start_line, functions[1].span.start_line);
}

#[test]
fn 여러_언어의_클래스와_함수가_공통_유닛으로_정규화된다() {
    let sources = [
        (
            "main.js",
            Language::JavaScript,
            "class Service { async run(value) { return value; } }",
        ),
        (
            "main.py",
            Language::Python,
            "class Service:\n    def run(self, value: str) -> str:\n        return value\n",
        ),
        (
            "Main.java",
            Language::Java,
            "public class Service { public String run(String value) { return value; } }",
        ),
        (
            "main.cpp",
            Language::Cpp,
            "class Service { public: int run(int value) { return value; } };",
        ),
        (
            "main.c",
            Language::C,
            "struct Service { int value; }; int run(int value) { return value; }",
        ),
        (
            "Main.cs",
            Language::CSharp,
            "public class Service { public string Run(string value) { return value; } }",
        ),
        (
            "main.go",
            Language::Go,
            "package main\ntype Service struct { value int }\nfunc (service Service) run(value int) int { return value }",
        ),
        (
            "main.dart",
            Language::Dart,
            "class Service { String run(String value) { return value; } }",
        ),
        (
            "main.rs",
            Language::Rust,
            "struct Service; impl Service { fn run(value: i32) -> i32 { value } }",
        ),
    ];

    for (path, language, source) in sources {
        let bundle = analyze_file(&file(path, language), source, &AnalysisConfig::default());
        assert!(
            bundle.units.iter().any(|unit| {
                matches!(
                    unit.kind,
                    CodeUnitKind::Class
                        | CodeUnitKind::Struct
                        | CodeUnitKind::Impl
                        | CodeUnitKind::Record
                )
            }),
            "{path}에서 타입 유닛이 추출되어야 한다"
        );
        assert!(
            bundle.units.iter().any(|unit| {
                matches!(
                    unit.kind,
                    CodeUnitKind::Function | CodeUnitKind::Method | CodeUnitKind::Constructor
                )
            }),
            "{path}에서 함수 유닛이 추출되어야 한다"
        );
    }
}

#[test]
fn 호출_기반_외부자원이_언어_공통_사실로_정규화된다() {
    let source = r#"
import fs from "fs";
import axios from "axios";
import redis from "redis";
async function load() {
  const response = await axios.get("https://example.test/users");
  const data = await fs.readFile("./data.json");
  await redis.get("session:1");
await eventBus.publish("user.created");
  return response + data;
}

"#;
    let bundle = analyze_file(
        &file("src/resources.ts", Language::TypeScript),
        source,
        &AnalysisConfig::default(),
    );

    assert!(bundle
        .resources
        .iter()
        .any(|resource| resource.kind == ResourceKind::ExternalApi
            && resource.name == "https://example.test/users"));
    assert!(bundle
        .resources
        .iter()
        .any(|resource| resource.kind == ResourceKind::File && resource.name == "./data.json"));
    assert!(bundle
        .resources
        .iter()
        .any(|resource| resource.kind == ResourceKind::Cache && resource.name == "session:1"));
    assert!(bundle.resources.iter().any(|resource| {
        resource.kind == ResourceKind::EventTopic && resource.name == "user.created"
    }));
}

#[test]
fn import_alias가_변수명과_무관하게_파일_캐시_환경자원을_복원한다() {
    let bundle = analyze_file(
        &file("src/aliased-resources.ts", Language::TypeScript),
        r#"
import { readFile as slurp } from "fs";
import { get as cacheGet } from "redis";
import { getenv as readEnv } from "os";

function load() {
  slurp("./data.json");
  cacheGet("session");
  readEnv("API_KEY");
}
"#,
        &AnalysisConfig::default(),
    );

    assert!(bundle
        .resources
        .iter()
        .any(|resource| { resource.kind == ResourceKind::File && resource.name == "./data.json" }));
    assert!(bundle
        .resources
        .iter()
        .any(|resource| { resource.kind == ResourceKind::Cache && resource.name == "session" }));
    assert!(bundle.resources.iter().any(|resource| {
        resource.kind == ResourceKind::Environment && resource.name == "API_KEY"
    }));
}

#[test]
fn 일반_메서드는_외부자원으로_오인되지_않고_명시된_receiver만_추출된다() {
    let source = r#"
function run(customer, socket, bus) {
  customer.read_text();
  connect("not-a-network-resource");
  publish("not-an-event-resource");
  socket.connect("localhost");
  bus.publish("user.created");
}
"#;
    let bundle = analyze_file(
        &file("src/resources.ts", Language::TypeScript),
        source,
        &AnalysisConfig::default(),
    );

    assert!(!bundle
        .resources
        .iter()
        .any(|resource| resource.name == "not-a-network-resource"));
    assert!(!bundle
        .resources
        .iter()
        .any(|resource| resource.name == "not-an-event-resource"));
    assert!(!bundle
        .resources
        .iter()
        .any(|resource| { resource.kind == ResourceKind::File && resource.name == "customer" }));
    assert!(bundle.resources.iter().any(|resource| {
        resource.kind == ResourceKind::Network && resource.name == "localhost"
    }));
    assert!(bundle.resources.iter().any(|resource| {
        resource.kind == ResourceKind::EventTopic && resource.name == "user.created"
    }));
}

#[test]
fn 네트워크와_환경변수_접근이_리소스_종류를_보존한다() {
    let python = analyze_file(
        &file("src/config.py", Language::Python),
        "import os\nimport socket\ndef load():\n    os.getenv(\"API_KEY\")\n    socket.connect(\"localhost\")\n",
        &AnalysisConfig::default(),
    );
    assert!(python.resources.iter().any(|resource| {
        resource.kind == ResourceKind::Environment && resource.name == "API_KEY"
    }));
    assert!(python.resources.iter().any(|resource| {
        resource.kind == ResourceKind::Network && resource.name == "localhost"
    }));
}

#[test]
fn websocket_process와_파일_receiver의_접근_모드를_보존한다() {
    let typescript = analyze_file(
        &file("src/runtime.ts", Language::TypeScript),
        r#"
function run(path: Path) {
  new WebSocket("wss://example.test");
  path.read_text();
  path.write_text(content);
}
"#,
        &AnalysisConfig::default(),
    );
    assert!(typescript.resources.iter().any(|resource| {
        resource.kind == ResourceKind::WebSocket
            && resource.name == "wss://example.test"
            && resource.mode == code_analysis_engine::facts::AccessMode::ReadWrite
    }));
    assert!(typescript.resources.iter().any(|resource| {
        resource.kind == ResourceKind::File
            && resource.name == "path"
            && resource.mode == code_analysis_engine::facts::AccessMode::Read
    }));
    assert!(typescript.resources.iter().any(|resource| {
        resource.kind == ResourceKind::File
            && resource.name == "path"
            && resource.mode == code_analysis_engine::facts::AccessMode::Write
    }));

    let python = analyze_file(
        &file("src/process.py", Language::Python),
        "import subprocess\ndef run():\n    subprocess.run([\"tool\"])\n",
        &AnalysisConfig::default(),
    );
    assert!(python
        .resources
        .iter()
        .any(|resource| resource.kind == ResourceKind::Process));
}

#[test]
fn ecma_동적_api의_식별자_alias를_동적_경계로_보존한다() {
    let bundle = analyze_file(
        &file("src/dynamic.ts", Language::TypeScript),
        "const runtimeRequire = require;\nfunction load() { return runtimeRequire('module'); }\n",
        &AnalysisConfig::default(),
    );

    assert!(bundle.references.iter().any(|reference| {
        reference.target_name == "runtimeRequire"
            && reference.status == code_analysis_engine::facts::ResolutionStatus::Dynamic
    }));
}

#[test]
fn 멀티라인_sql이_테이블과_읽기쓰기_혼합을_보존한다() {
    let source = r#"
function load() {
  const query = `
    SELECT users.id
    FROM users
    JOIN orders ON orders.user_id = users.id
  `;
  const write = `
    INSERT INTO audit_log (user_id)
    SELECT id FROM users
  `;
  return query + write;
}

"#;
    let bundle = analyze_file(
        &file("src/sql.ts", Language::TypeScript),
        source,
        &AnalysisConfig::default(),
    );
    assert!(bundle.resources.iter().any(|resource| {
        resource.name == "users" && resource.mode == code_analysis_engine::facts::AccessMode::Read
    }));
    assert!(bundle.resources.iter().any(|resource| {
        resource.name == "orders" && resource.mode == code_analysis_engine::facts::AccessMode::Read
    }));
    assert!(!bundle.resources.iter().any(|resource| {
        resource.name == "users"
            && resource.mode == code_analysis_engine::facts::AccessMode::ReadWrite
    }));
    assert!(bundle.resources.iter().any(|resource| {
        resource.name == "audit_log"
            && resource.mode == code_analysis_engine::facts::AccessMode::Write
    }));
}

#[test]
fn 주석과_일반_import_텍스트는_sql_테이블이_되지_않는다() {
    let source = r#"
// SELECT * FROM fake_comment_table;
/* INSERT INTO fake_block_table VALUES (1); */
import { fromValue } from "ordinary-module";
const text = "a value from another source";
const query = `SELECT * FROM real_users`;
"#;
    let bundle = analyze_file(
        &file("src/sql.ts", Language::TypeScript),
        source,
        &AnalysisConfig::default(),
    );
    let tables = bundle
        .resources
        .iter()
        .filter(|resource| resource.kind == ResourceKind::Table)
        .collect::<Vec<_>>();

    assert!(tables.iter().any(|resource| resource.name == "real_users"));
    assert!(!tables.iter().any(|resource| {
        matches!(
            resource.name.as_str(),
            "fake_comment_table" | "fake_block_table" | "ordinary-module" | "another"
        )
    }));
}

#[test]
fn javascript_export가_파일_관계와_로컬_대상_유닛을_보존한다() {
    let source = r#"
function login() {}
const logout = () => {};
export { login, logout as signOut };
export { User } from "./models";
"#;
    let bundle = analyze_file(
        &file("src/exports.ts", Language::TypeScript),
        source,
        &AnalysisConfig::default(),
    );
    let mut store = FactStore::default();
    store.merge(bundle);
    store.resolve_references();

    assert!(store.references.iter().any(|reference| {
        reference.kind == ReferenceKind::Export && reference.target_name == "login"
    }));
    assert!(store.references.iter().any(|reference| {
        reference.kind == ReferenceKind::Export && reference.target_name == "logout"
    }));
    assert!(store.references.iter().any(|reference| {
        reference.kind == ReferenceKind::Export && reference.target_name == "./models"
    }));
    let login_id = store
        .units
        .values()
        .find(|unit| unit.name == "login")
        .map(|unit| unit.id.clone())
        .expect("login 유닛이 있어야 한다");
    assert!(store.references.iter().any(|reference| {
        reference.kind == ReferenceKind::Export
            && reference.target_name == "login"
            && reference.target_unit_id.as_deref() == Some(login_id.as_str())
    }));
}

#[test]
fn 열_개_언어의_공통_유닛_계약이_계층과_종류를_보존한다() {
    let javascript = analyze_file(
        &file("src/service.js", Language::JavaScript),
        "class Service { constructor() {} #secret = 1; run() {} }",
        &AnalysisConfig::default(),
    );
    assert!(javascript
        .units
        .iter()
        .any(|unit| unit.kind == CodeUnitKind::Constructor && unit.name == "constructor"));
    assert!(javascript
        .units
        .iter()
        .any(|unit| unit.kind == CodeUnitKind::Property && unit.name == "#secret"));

    let typescript = analyze_file(
        &file("src/service.ts", Language::TypeScript),
        "abstract class Service { abstract run(value: string): void; }",
        &AnalysisConfig::default(),
    );
    assert!(typescript
        .units
        .iter()
        .any(|unit| unit.kind == CodeUnitKind::Method && unit.name == "run"));

    let java = analyze_file(
        &file("src/Main.java", Language::Java),
        "package qa.variant.deep; class Service { Service() {} }",
        &AnalysisConfig::default(),
    );
    assert!(java
        .units
        .iter()
        .any(|unit| unit.kind == CodeUnitKind::Package && unit.name == "qa.variant.deep"));

    let go = analyze_file(
        &file("src/service.go", Language::Go),
        "package api\ntype Service interface { Run() error }\ntype Impl struct{}\nfunc (Impl) Run() error { return nil }",
        &AnalysisConfig::default(),
    );
    assert!(go
        .units
        .iter()
        .any(|unit| unit.kind == CodeUnitKind::Interface && unit.name == "Service"));
    assert!(
        go.units
            .iter()
            .filter(|unit| unit.kind == CodeUnitKind::Method && unit.name == "Run")
            .count()
            >= 2
    );

    let rust = analyze_file(
        &file("src/service.rs", Language::Rust),
        "trait Service { fn run(&self); } impl Service for Worker { fn run(&self) {} } struct Worker;",
        &AnalysisConfig::default(),
    );
    let rust_methods = rust
        .units
        .iter()
        .filter(|unit| unit.kind == CodeUnitKind::Method && unit.name == "run")
        .map(|unit| unit.qualified_name.as_str())
        .collect::<HashSet<_>>();
    assert_eq!(rust_methods.len(), 2);

    let csharp = analyze_file(
        &file("src/Service.cs", Language::CSharp),
        "namespace qa.variant.deep; class Service { void Run() {} }",
        &AnalysisConfig::default(),
    );
    assert!(csharp
        .units
        .iter()
        .any(|unit| unit.kind == CodeUnitKind::Namespace && unit.name == "qa.variant.deep"));

    let dart = analyze_file(
        &file("src/service.dart", Language::Dart),
        "class Service { Service(); Service.named(); factory Service.from() => Service(); int get value => 1; set value(int value) {} }",
        &AnalysisConfig::default(),
    );
    assert!(dart
        .units
        .iter()
        .any(|unit| unit.kind == CodeUnitKind::Constructor && unit.name == "Service.named"));
    assert!(dart
        .units
        .iter()
        .any(|unit| unit.kind == CodeUnitKind::Property && unit.name == "value"));
}

#[test]
fn 비정형_람다와_csharp_primary_constructor의_정보를_보존한다() {
    let python = analyze_file(
        &file("src/lambda.py", Language::Python),
        "handler = lambda value: value + 1\n",
        &AnalysisConfig::default(),
    );
    let python_lambdas = python
        .units
        .iter()
        .filter(|unit| unit.kind == CodeUnitKind::Lambda)
        .collect::<Vec<_>>();
    assert_eq!(python_lambdas.len(), 1);

    let cpp = analyze_file(
        &file("src/lambda.cpp", Language::Cpp),
        "int run() { return [value](int input) { return input + value; }(1); }\n",
        &AnalysisConfig::default(),
    );
    assert!(cpp
        .units
        .iter()
        .any(|unit| { unit.kind == CodeUnitKind::Lambda && unit.name.starts_with("<lambda@") }));
    assert!(!cpp
        .units
        .iter()
        .any(|unit| { unit.kind == CodeUnitKind::Lambda && unit.name == "value" }));

    let csharp = analyze_file(
        &file("src/Service.cs", Language::CSharp),
        "public class Service(string connection) { public void Run() {} }\n",
        &AnalysisConfig::default(),
    );
    let service = csharp
        .units
        .iter()
        .find(|unit| unit.name == "Service")
        .expect("primary constructor를 가진 클래스가 있어야 한다");
    assert!(service
        .signature
        .as_deref()
        .is_some_and(|signature| signature.contains("connection")));
    assert_eq!(service.parameters.len(), 1);
    assert_eq!(service.parameters[0].name, "connection");
}

#[test]
fn csharp의_대문자_main도_정적_프로그램_진입점으로_보존한다() {
    let bundle = analyze_file(
        &file("src/Program.cs", Language::CSharp),
        "class Program { static void Main(string[] args) {} }\n",
        &AnalysisConfig::default(),
    );
    assert!(bundle.entrypoints.iter().any(|entrypoint| {
        entrypoint.kind == code_analysis_engine::facts::EntrypointKind::Main
            && bundle
                .units
                .iter()
                .find(|unit| unit.id == entrypoint.unit_id)
                .is_some_and(|unit| unit.name == "Main")
    }));
}

#[test]
fn java_reflection_호출은_동적_경계로_표시되고_일반_이름은_오탐하지_않는다() {
    let bundle = analyze_file(
        &file("src/Reflection.java", Language::Java),
        r#"
class Reflection {
  void evaluate() {}
  void run(String name, Object target) throws Exception {
    evaluate();
    Class.forName(name);
    target.getClass().getMethod("run").invoke(target);
  }
}
"#,
        &AnalysisConfig::default(),
    );

    let dynamic_targets = bundle
        .references
        .iter()
        .filter(|reference| {
            reference.status == code_analysis_engine::facts::ResolutionStatus::Dynamic
        })
        .map(|reference| reference.target_name.as_str())
        .collect::<HashSet<_>>();
    assert!(dynamic_targets.contains("Class.forName"));
    assert!(dynamic_targets.contains("getMethod.invoke"));
    assert!(!bundle.references.iter().any(|reference| {
        reference.target_name == "evaluate"
            && reference.status == code_analysis_engine::facts::ResolutionStatus::Dynamic
    }));
}
