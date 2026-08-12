use code_analysis_engine::analyze;
use code_analysis_engine::flow::{FlowEdgeKind, FlowNodeKind};
use code_analysis_engine::AnalysisRequest;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn temporary_project() -> PathBuf {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("visual-map-flow-{suffix}"));
    fs::create_dir_all(&path).expect("임시 프로젝트를 만들어야 한다");
    path
}

#[test]
fn 함수별_실행흐름에_분기_호출_return_동적경계가_포함된다() {
    let root = temporary_project();
    fs::write(
        root.join("flow.ts"),
        r#"
export function login(request: Request) {
  if (request.enabled) {
    return loadUser(request);
  }
  return eval(request.handler);
}

function loadUser(request: Request) {
  return request.user;
}
"#,
    )
    .expect("소스 파일을 써야 한다");

    let result = analyze(AnalysisRequest::new(&root)).expect("분석이 성공해야 한다");
    let overview = result.overview.expect("Overview가 있어야 한다");
    let login = overview
        .execution_flows
        .flows
        .iter()
        .find(|flow| {
            overview
                .units
                .iter()
                .find(|unit| unit.id == flow.owner_unit_id)
                .is_some_and(|unit| unit.name == "login")
        })
        .expect("login 실행 흐름이 있어야 한다");

    assert!(login
        .nodes
        .iter()
        .any(|node| node.kind == FlowNodeKind::Condition));
    assert!(login
        .nodes
        .iter()
        .any(|node| node.kind == FlowNodeKind::Call));
    assert!(login
        .nodes
        .iter()
        .any(|node| node.kind == FlowNodeKind::DynamicBoundary));
    assert!(login
        .nodes
        .iter()
        .any(|node| node.kind == FlowNodeKind::Return));
    assert!(login
        .edges
        .iter()
        .any(|edge| edge.kind == FlowEdgeKind::TrueBranch));
    assert!(login
        .edges
        .iter()
        .any(|edge| edge.kind == FlowEdgeKind::FalseBranch));
    assert!(!login.dynamic_boundary_ids.is_empty());

    fs::remove_dir_all(root).expect("임시 프로젝트를 정리해야 한다");
}

#[test]
fn 객체_생성도_실행흐름의_구성_호출로_보존된다() {
    let root = temporary_project();
    fs::write(
        root.join("construct.ts"),
        r#"
class Service {}
export function create() {
  const service = new Service();
  return service;
}
"#,
    )
    .expect("생성자 fixture를 써야 한다");

    let result = analyze(AnalysisRequest::new(&root)).expect("분석이 성공해야 한다");
    let overview = result.overview.expect("Overview가 있어야 한다");
    let create = overview
        .execution_flows
        .flows
        .iter()
        .find(|flow| {
            overview
                .units
                .iter()
                .find(|unit| unit.id == flow.owner_unit_id)
                .is_some_and(|unit| unit.name == "create")
        })
        .expect("create 흐름이 있어야 한다");
    assert!(create.nodes.iter().any(|node| node.label == "Service"));
    assert!(create
        .nodes
        .iter()
        .any(|node| { node.label == "Service" && node.target_unit_id.is_some() }));

    fs::remove_dir_all(root).expect("임시 프로젝트를 정리해야 한다");
}

#[test]
fn try_catch와_loop은_예외_및_반복_경계를_보존한다() {
    let root = temporary_project();
    fs::write(
        root.join("resilience.ts"),
        r#"
export function recover(items: Request[]) {
  try {
    for (const item of items) {
      load(item);
    }
  } catch (error) {
    return fallback(error);
  }
}
function load(item: Request) { return item; }
function fallback(error: unknown) { return error; }
"#,
    )
    .expect("try/catch fixture를 써야 한다");

    let result = analyze(AnalysisRequest::new(&root)).expect("분석이 성공해야 한다");
    let overview = result.overview.expect("Overview가 있어야 한다");
    let recover = overview
        .execution_flows
        .flows
        .iter()
        .find(|flow| {
            overview
                .units
                .iter()
                .find(|unit| unit.id == flow.owner_unit_id)
                .is_some_and(|unit| unit.name == "recover")
        })
        .expect("recover 실행 흐름이 있어야 한다");

    assert!(recover
        .nodes
        .iter()
        .any(|node| node.kind == FlowNodeKind::Catch));
    assert!(recover
        .nodes
        .iter()
        .any(|node| node.kind == FlowNodeKind::Loop));
    assert!(recover
        .edges
        .iter()
        .any(|edge| edge.kind == FlowEdgeKind::Exception));
    assert!(recover
        .edges
        .iter()
        .any(|edge| edge.kind == FlowEdgeKind::LoopBack));

    fs::remove_dir_all(root).expect("임시 프로젝트를 정리해야 한다");
}

#[test]
fn break와_continue는_가장_안쪽_반복문의_정확한_경계로_연결된다() {
    let root = temporary_project();
    fs::write(
        root.join("loop-control.ts"),
        r#"
export function process(items: Request[]) {
  for (const item of items) {
    if (shouldSkip(item)) continue;
    if (shouldStop(item)) break;
    work(item);
  }
  after();
}
function shouldSkip(item: Request) { return item.skip; }
function shouldStop(item: Request) { return item.stop; }
function work(item: Request) { return item; }
function after() { return true; }
export function nested(items: Request[]) {
  for (const outer of items) {
    for (const inner of items) {
      if (shouldStop(inner)) break;
      work(inner);
    }
    afterInner();
  }
  afterOuter();
}
function afterInner() { return true; }
function afterOuter() { return true; }
export function terminalLoop(items: Request[]) {
  for (const item of items) {
    if (shouldStop(item)) break;
  }
}
"#,
    )
    .expect("임시 소스를 써야 한다");

    let result = analyze(AnalysisRequest::new(&root)).expect("분석이 성공해야 한다");
    let overview = result.overview.expect("Overview가 있어야 한다");
    let process = overview
        .execution_flows
        .flows
        .iter()
        .find(|flow| {
            overview
                .units
                .iter()
                .find(|unit| unit.id == flow.owner_unit_id)
                .is_some_and(|unit| unit.name == "process")
        })
        .expect("process 실행 흐름이 있어야 한다");
    let loop_node = process
        .nodes
        .iter()
        .find(|node| node.kind == FlowNodeKind::Loop)
        .expect("반복문 노드가 있어야 한다");
    let continue_node = process
        .nodes
        .iter()
        .find(|node| node.kind == FlowNodeKind::Continue)
        .expect("continue 노드가 있어야 한다");
    let break_node = process
        .nodes
        .iter()
        .find(|node| node.kind == FlowNodeKind::Break)
        .expect("break 노드가 있어야 한다");
    let after_node = process
        .nodes
        .iter()
        .find(|node| node.label == "after")
        .expect("반복문 다음의 after 호출이 있어야 한다");

    assert!(process.edges.iter().any(|edge| {
        edge.source_node_id == continue_node.id
            && edge.target_node_id == loop_node.id
            && edge.kind == FlowEdgeKind::LoopBack
            && edge.label.as_deref() == Some("continue")
    }));
    assert!(process.edges.iter().any(|edge| {
        edge.source_node_id == break_node.id
            && edge.target_node_id == after_node.id
            && edge.kind == FlowEdgeKind::FalseBranch
            && edge.label.as_deref() == Some("break")
    }));
    assert!(!process.edges.iter().any(|edge| {
        edge.source_node_id == break_node.id && edge.target_node_id == loop_node.id
    }));

    let nested = overview
        .execution_flows
        .flows
        .iter()
        .find(|flow| {
            overview
                .units
                .iter()
                .find(|unit| unit.id == flow.owner_unit_id)
                .is_some_and(|unit| unit.name == "nested")
        })
        .expect("nested 실행 흐름이 있어야 한다");
    let nested_break = nested
        .nodes
        .iter()
        .find(|node| node.kind == FlowNodeKind::Break)
        .expect("중첩 반복문의 break 노드가 있어야 한다");
    let after_inner = nested
        .nodes
        .iter()
        .find(|node| node.label == "afterInner")
        .expect("안쪽 반복문 다음 호출이 있어야 한다");
    assert!(nested.edges.iter().any(|edge| {
        edge.source_node_id == nested_break.id
            && edge.target_node_id == after_inner.id
            && edge.label.as_deref() == Some("break")
    }));

    let terminal_loop = overview
        .execution_flows
        .flows
        .iter()
        .find(|flow| {
            overview
                .units
                .iter()
                .find(|unit| unit.id == flow.owner_unit_id)
                .is_some_and(|unit| unit.name == "terminalLoop")
        })
        .expect("terminalLoop 실행 흐름이 있어야 한다");
    let terminal_break = terminal_loop
        .nodes
        .iter()
        .find(|node| node.kind == FlowNodeKind::Break)
        .expect("마지막 이벤트 break가 있어야 한다");
    assert!(!terminal_loop.edges.iter().any(|edge| {
        edge.source_node_id == terminal_break.id && edge.kind == FlowEdgeKind::LoopBack
    }));

    fs::remove_dir_all(root).expect("임시 프로젝트를 정리해야 한다");
}

#[test]
fn throw는_같은_try의_catch로_연결된다() {
    let root = temporary_project();
    fs::write(
        root.join("throw-catch.ts"),
        r#"
export function recover() {
  try {
    throw new Error("failed");
  } catch (error) {
    fallback(error);
  }
  after();
}
function fallback(error: unknown) { return error; }
function after() { return true; }
"#,
    )
    .expect("임시 소스를 써야 한다");

    let result = analyze(AnalysisRequest::new(&root)).expect("분석이 성공해야 한다");
    let overview = result.overview.expect("Overview가 있어야 한다");
    let recover = overview
        .execution_flows
        .flows
        .iter()
        .find(|flow| {
            overview
                .units
                .iter()
                .find(|unit| unit.id == flow.owner_unit_id)
                .is_some_and(|unit| unit.name == "recover")
        })
        .expect("recover 실행 흐름이 있어야 한다");
    let throw_node = recover
        .nodes
        .iter()
        .find(|node| node.kind == FlowNodeKind::Throw)
        .expect("throw 노드가 있어야 한다");
    let catch_node = recover
        .nodes
        .iter()
        .find(|node| node.kind == FlowNodeKind::Catch)
        .expect("catch 노드가 있어야 한다");

    assert!(recover.edges.iter().any(|edge| {
        edge.source_node_id == throw_node.id
            && edge.target_node_id == catch_node.id
            && edge.kind == FlowEdgeKind::Exception
    }));

    fs::remove_dir_all(root).expect("임시 프로젝트를 정리해야 한다");
}
