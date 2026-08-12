use code_analysis_engine::config::AnalysisConfig;
use code_analysis_engine::facts::{
    CodeUnitKind, CodeUnitVisibility, FactStore, ReferenceKind, ResourceKind,
};
use code_analysis_engine::languages::analyze_file;
use code_analysis_engine::model::{FileEntry, Language};
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
