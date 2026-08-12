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
    let path = std::env::temp_dir().join(format!("visual-map-framework-java-{suffix}"));
    fs::create_dir_all(&path).expect("임시 Java 프로젝트를 만들어야 한다");
    path
}

#[test]
fn spring_validation_import는_jakarta_ee로_오인되지_않고_경로와_원문근거를_보존한다() {
    let root = temporary_project();
    fs::write(
        root.join("OwnerController.java"),
        r#"
@SpringBootApplication
class Application {}

import jakarta.validation.Valid;
import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.RequestMethod;
import org.springframework.web.bind.annotation.RestController;

@RestController
@RequestMapping("/owners")
class OwnerController {
  @GetMapping(value = "pets")
  void pets(@Valid String request) {}

  @RequestMapping(value = "/search", method = RequestMethod.GET)
  void search() {}
}
"#,
    )
    .expect("Spring Java fixture를 써야 한다");
    fs::write(
        root.join("ReactiveRoutes.java"),
        r#"
import org.springframework.web.reactive.function.server.RouterFunction;
import org.springframework.web.reactive.function.server.RequestPredicates;

class ReactiveRoutes {
  RouterFunction<?> routes() {
    return RouterFunctions.route(RequestPredicates.GET("/reactive"), request -> ok());
  }
}
"#,
    )
    .expect("Spring WebFlux Java fixture를 써야 한다");

    let result = analyze(AnalysisRequest::new(&root)).expect("분석이 성공해야 한다");
    let overview = result.overview.expect("Overview가 생성되어야 한다");

    assert!(overview
        .detected_frameworks
        .iter()
        .any(|framework| framework.id == "java.spring_mvc"));
    assert!(!overview
        .detected_frameworks
        .iter()
        .any(|framework| framework.id == "java.jakarta_ee"));
    assert!(overview.units.iter().any(|unit| {
        unit.name == "Application"
            && unit
                .modifiers
                .iter()
                .any(|modifier| modifier == "framework:spring-boot-application")
    }));
    assert!(overview.units.iter().any(|unit| {
        unit.name == "OwnerController"
            && unit
                .modifiers
                .iter()
                .any(|modifier| modifier == "framework:spring-mvc-controller")
    }));

    assert!(overview.entrypoints.iter().any(|entrypoint| {
        entrypoint.framework_id.as_deref() == Some("java.spring_webflux")
            && entrypoint.method.as_deref() == Some("GET")
            && entrypoint.path.as_deref() == Some("/reactive")
    }));
    for (path, method) in [("/owners/pets", "GET"), ("/owners/search", "GET")] {
        let entrypoint = overview
            .entrypoints
            .iter()
            .find(|entrypoint| {
                entrypoint.kind == EntrypointKind::Http
                    && entrypoint.path.as_deref() == Some(path)
                    && entrypoint.method.as_deref() == Some(method)
            })
            .unwrap_or_else(|| panic!("Java route가 없어: {method} {path}"));
        assert_eq!(entrypoint.framework_id.as_deref(), Some("java.spring_mvc"));
        assert!(entrypoint
            .evidence
            .iter()
            .any(|evidence| evidence.kind == "routeSource" && evidence.value.starts_with('@')));
    }

    fs::remove_dir_all(root).expect("임시 Java 프로젝트를 정리해야 한다");
}
