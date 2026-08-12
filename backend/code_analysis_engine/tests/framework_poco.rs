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
    let path = std::env::temp_dir().join(format!("visual-map-framework-poco-{suffix}"));
    fs::create_dir_all(&path).expect("임시 프로젝트를 만들어야 한다");
    path
}

#[test]
fn poco_http_handler와_factory의_상속_계약을_진입점으로_변환한다() {
    let root = temporary_project();
    fs::write(
        root.join("server.cpp"),
        r#"
#include <Poco/Net/HTTPServer.h>
#include <Poco/Net/HTTPRequestHandler.h>
#include <Poco/Net/HTTPRequestHandlerFactory.h>

class HealthHandler : public Poco::Net::HTTPRequestHandler {
 public:
  void handleRequest(Poco::Net::HTTPServerRequest& request,
                     Poco::Net::HTTPServerResponse& response) override {}
};

class HandlerFactory : public Poco::Net::HTTPRequestHandlerFactory {
 public:
  Poco::Net::HTTPRequestHandler* createRequestHandler(
      const Poco::Net::HTTPServerRequest& request) override {
    return new HealthHandler();
  }
};

void start() {
  Poco::Net::HTTPServer server(new HandlerFactory(), 8080);
  server.start();
}
"#,
    )
    .expect("POCO fixture를 써야 한다");

    let result = analyze(AnalysisRequest::new(&root)).expect("분석이 성공해야 한다");
    let overview = result.overview.expect("Overview가 있어야 한다");

    assert!(overview
        .detected_frameworks
        .iter()
        .any(|framework| framework.id == "cpp.poco"));
    assert!(overview.entrypoints.iter().any(|entrypoint| {
        entrypoint.framework_id.as_deref() == Some("cpp.poco")
            && entrypoint.kind == EntrypointKind::Http
            && entrypoint.name == "handleRequest"
            && entrypoint.method.as_deref() == Some("POCO_HTTP")
    }));
    assert!(overview.entrypoints.iter().any(|entrypoint| {
        entrypoint.framework_id.as_deref() == Some("cpp.poco")
            && entrypoint.kind == EntrypointKind::Callback
            && entrypoint.name == "createRequestHandler"
    }));
    assert!(overview.units.iter().any(|unit| {
        unit.name == "HealthHandler"
            && unit
                .modifiers
                .iter()
                .any(|modifier| modifier == "framework:poco-http-component")
    }));

    fs::remove_dir_all(root).expect("임시 프로젝트를 정리해야 한다");
}
