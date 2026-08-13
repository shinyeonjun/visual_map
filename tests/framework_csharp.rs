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
    let path = std::env::temp_dir().join(format!("visual-map-csharp-routes-{suffix}"));
    fs::create_dir_all(&path).expect("임시 프로젝트 디렉터리를 만들어야 한다");
    path
}

#[test]
fn csharp_attribute_prefix가_mvc와_web_api_경로를_합성한다() {
    let root = temporary_project();
    fs::write(
        root.join("mvc.cs"),
        r#"
using System.Web.Mvc;
[RoutePrefix("api/orders")]
public class OrdersController : Controller {
  [HttpGet]
  [Route("{id}")]
  public void Get() {}
}
"#,
    )
    .expect("MVC fixture를 써야 한다");
    fs::write(
        root.join("webapi.cs"),
        r#"
using System.Web.Http;
[RoutePrefix("api/users")]
public class UsersApi {
  [HttpGet]
  [Route("{id}")]
  public IHttpActionResult Get() { return null; }
}
"#,
    )
    .expect("Web API fixture를 써야 한다");

    let overview = analyze(AnalysisRequest::new(&root))
        .expect("분석이 성공해야 한다")
        .overview
        .expect("Overview가 생성되어야 한다");

    assert!(overview.entrypoints.iter().any(|entrypoint| {
        entrypoint.framework_id.as_deref() == Some("csharp.aspnet_mvc")
            && entrypoint.kind == EntrypointKind::Http
            && entrypoint.path.as_deref() == Some("/api/orders/{id}")
    }));
    assert!(overview.entrypoints.iter().any(|entrypoint| {
        entrypoint.framework_id.as_deref() == Some("csharp.aspnet_web_api")
            && entrypoint.kind == EntrypointKind::Http
            && entrypoint.path.as_deref() == Some("/api/users/{id}")
    }));
}
