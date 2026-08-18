use code_analysis_engine::facts::{CodeUnitKind, ReferenceKind};
use code_analysis_engine::{analyze, AnalysisRequest};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn temporary_project(name: &str) -> PathBuf {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("visual-map-{name}-{suffix}"));
    fs::create_dir_all(&path).expect("임시 프로젝트 디렉터리를 만들어야 한다");
    path
}

#[test]
fn 도메인_overview에_도메인_진입점_자원_관계가_담긴다() {
    let root = temporary_project("domain-pipeline");
    fs::create_dir_all(root.join("src/order")).expect("order 디렉터리를 만들어야 한다");
    fs::create_dir_all(root.join("src/payment")).expect("payment 디렉터리를 만들어야 한다");
    fs::create_dir_all(root.join("src/auth")).expect("auth 디렉터리를 만들어야 한다");
    fs::create_dir_all(root.join("src/common")).expect("common 디렉터리를 만들어야 한다");

    fs::write(
        root.join("package.json"),
        r#"{"dependencies":{"express":"5.0.0"}}"#,
    )
    .expect("package manifest를 써야 한다");
    fs::write(
        root.join("src/order/OrderController.ts"),
        r#"
import { OrderService } from './OrderService';
import express from 'express';
const router = express();
router.post('/orders', createOrder);
export function createOrder(request: Request) {
  return OrderService.createOrder(request);
}
"#,
    )
    .expect("order controller를 써야 한다");
    fs::write(
        root.join("src/order/OrderService.ts"),
        r#"
import { PaymentService } from '../payment/PaymentService';
export class OrderService {
  static createOrder(request: Request) {
    PaymentService.authorize(request);
    return saveOrder(request);
  }
}
"#,
    )
    .expect("order service를 써야 한다");
    fs::write(
        root.join("src/order/OrderRepository.ts"),
        r#"
export function saveOrder(order: Order) {
  return db.query('INSERT INTO orders VALUES (?)', order);
}
"#,
    )
    .expect("order repository를 써야 한다");
    fs::write(
        root.join("src/payment/PaymentService.ts"),
        r#"
export class PaymentService {
  static authorize(order: Order) {
    return paymentGateway.authorize(order);
  }
}
"#,
    )
    .expect("payment service를 써야 한다");
    fs::write(
        root.join("src/auth/AuthService.ts"),
        r#"
export function login(request: Request) {
  return userRepository.findUser(request.userId);
}
"#,
    )
    .expect("auth service를 써야 한다");
    fs::write(
        root.join("src/common/Logger.ts"),
        "export function log(message: string) { console.log(message); }\n",
    )
    .expect("common 파일을 써야 한다");
    fs::write(
        root.join("src/common/DynamicDispatcher.ts"),
        r#"
export function dispatch(name: string, request: Request) {
  const handlers = { order: createOrder };
  return handlers[name](request);
}
"#,
    )
    .expect("동적 디스패처를 써야 한다");

    let result = analyze(AnalysisRequest::new(&root)).expect("도메인 분석이 성공해야 한다");
    let overview = result.overview.expect("Overview가 생성되어야 한다");
    let keys: Vec<_> = overview
        .domains
        .iter()
        .map(|domain| domain.key.as_str())
        .collect();

    assert!(
        overview.domains.iter().any(|domain| {
            domain.key.contains("order")
                || domain.entrypoint_ids.iter().any(|entrypoint_id| {
                    overview.entrypoints.iter().any(|entrypoint| {
                        &entrypoint.id == entrypoint_id
                            && entrypoint.path.as_deref() == Some("/orders")
                    })
                })
        }),
        "주문 경로 버킷 또는 /orders 진입점 도메인이 없어: {keys:?}"
    );
    assert!(
        overview.domains.len() <= 20,
        "도메인 수가 과도하게 많다: {}",
        overview.domains.len()
    );
    let order_feature = overview
        .features
        .iter()
        .find(|feature| {
            feature.entrypoint_ids.iter().any(|entrypoint_id| {
                overview.entrypoints.iter().any(|entrypoint| {
                    &entrypoint.id == entrypoint_id && entrypoint.path.as_deref() == Some("/orders")
                })
            })
        })
        .expect("POST /orders 기능이 있어야 한다");
    assert!(
        !order_feature.unit_ids.iter().any(|unit_id| {
            overview
                .units
                .iter()
                .find(|unit| &unit.id == unit_id)
                .is_some_and(|unit| unit.relative_path.contains("payment"))
        }),
        "주문 기능의 소유 유닛에 결제 모듈이 포함되면 안 된다"
    );
    assert!(overview.coverage.total_units >= 6);
    assert!(overview.coverage.total_entrypoints >= 1);
    assert!(overview.coverage.total_resources >= 1);
    assert!(!overview.features.is_empty());
    assert!(overview.features.iter().any(|feature| {
        feature.entrypoint_ids.iter().any(|entrypoint_id| {
            overview.entrypoints.iter().any(|entrypoint| {
                &entrypoint.id == entrypoint_id && entrypoint.path.as_deref() == Some("/orders")
            })
        })
    }));
    assert_eq!(overview.coverage.total_features, overview.features.len());
    assert_eq!(
        overview.coverage.total_execution_flows,
        overview.execution_flows.flows.len()
    );
    let flow_ids: HashSet<_> = overview
        .execution_flows
        .flows
        .iter()
        .map(|flow| flow.id.as_str())
        .collect();
    let unit_ids: HashSet<_> = overview.units.iter().map(|unit| unit.id.as_str()).collect();
    for feature in &overview.features {
        assert!(
            feature
                .unit_ids
                .iter()
                .all(|unit_id| unit_ids.contains(unit_id.as_str())),
            "기능이 존재하지 않는 유닛을 가리킨다: {}",
            feature.key
        );
        assert!(
            feature
                .flow_ids
                .iter()
                .all(|flow_id| flow_ids.contains(flow_id.as_str())),
            "기능이 존재하지 않는 실행 흐름을 가리킨다: {}",
            feature.key
        );
    }
    assert!(
        !order_feature.flow_ids.is_empty(),
        "HTTP 엔드포인트와 같은 파일의 실행 흐름은 기능에 붙어야 한다"
    );
    assert_eq!(
        overview.coverage.total_dynamic_boundaries,
        overview.dynamic_boundaries.len()
    );
    assert!(overview
        .domains
        .iter()
        .any(|domain| !domain.feature_ids.is_empty()));
    assert!(overview
        .entrypoints
        .iter()
        .any(|entrypoint| entrypoint.path.as_deref() == Some("/orders")));
    assert!(overview
        .resources
        .iter()
        .any(|resource| resource.name == "orders"));
    assert!(!overview.dynamic_boundaries.is_empty());
    assert_eq!(
        overview.dynamic_boundary_ids.len(),
        overview.dynamic_boundaries.len()
    );
    assert!(overview
        .detected_frameworks
        .iter()
        .any(|framework| framework.id == "javascript.express"));
    assert!(overview
        .entrypoints
        .iter()
        .any(|entrypoint| entrypoint.framework_id.as_deref() == Some("javascript.express")));
    assert_eq!(
        overview.static_graph.edges.len(),
        overview.coverage.total_references
    );
    assert_eq!(
        overview.coverage.total_confirmed_references
            + overview.coverage.total_candidate_references
            + overview.coverage.total_unknown_references
            + overview.coverage.total_dynamic_references,
        overview.coverage.total_references
    );
    assert_eq!(
        overview.coverage.total_dynamic_references,
        overview.coverage.total_dynamic_boundaries
    );
    assert_eq!(
        overview.static_graph.dynamic_edge_ids.len(),
        overview.dynamic_boundary_ids.len()
    );
    let prepared = code_analysis_engine::clean::prepare(&overview, &result.files);
    let prepared_json = serde_json::to_string(&prepared).expect("전처리 결과를 직렬화해야 한다");
    assert_eq!(prepared.schema_version, "prepared-static-overview.v1");
    assert!(!prepared_json.contains("staticGraph"));
    assert!(prepared.features.iter().all(|feature| feature
        .unit_ids
        .iter()
        .all(|unit_id| { prepared.units.iter().any(|unit| &unit.id == unit_id) })));
    assert!(prepared.features.iter().all(|feature| {
        feature.domain_ids.iter().all(|domain_id| {
            prepared
                .domains
                .iter()
                .any(|domain| &domain.id == domain_id)
        })
    }));
    assert!(result
        .files
        .iter()
        .any(|file| file.relative_path == "src/order/OrderController.ts"));
    assert_eq!(
        overview.semantic_status,
        code_analysis_engine::semantic::SemanticStatus::Disabled
    );

    fs::remove_dir_all(root).expect("임시 프로젝트를 정리해야 한다");
}

#[test]
fn 반복되는_generic_심볼은_프로젝트_개념_후보로_보존된다() {
    let root = temporary_project("domain-generic-symbol");
    for concept in ["analysis", "model", "transaction", "inventory", "결제"] {
        let directory = root.join("src").join(concept);
        fs::create_dir_all(&directory).expect("개념 디렉터리를 만들어야 한다");
        let (first_name, second_name) = if concept == "결제" {
            ("create_결제".to_string(), "load_결제".to_string())
        } else {
            let pascal = format!("{}{}", concept[..1].to_ascii_uppercase(), &concept[1..]);
            (format!("create{pascal}"), format!("load{pascal}"))
        };
        fs::write(
            directory.join("service.ts"),
            format!(
                "export function {first_name}(input: string) {{ return input; }}\nexport function {second_name}(id: string) {{ return id; }}\n"
            ),
        )
        .expect("반복되는 개념 심볼을 써야 한다");
    }

    let result = analyze(AnalysisRequest::new(&root)).expect("도메인 분석이 성공해야 한다");
    let overview = result.overview.expect("Overview가 생성되어야 한다");
    assert!(
        overview.coverage.total_units >= 10,
        "반복 개념 디렉터리의 유닛이 추출되어야 한다"
    );
    assert!(
        overview.features.len() <= overview.coverage.total_entrypoints.max(1) + 5,
        "진입점 없는 개념 디렉터리는 별도 기능으로 폭발하지 않아야 한다"
    );

    fs::remove_dir_all(root).expect("임시 프로젝트를 정리해야 한다");
}

#[test]
fn tsx_import와_type_reference가_실제_유닛으로_연결된다() {
    let root = temporary_project("tsx-type-relation");
    fs::create_dir_all(root.join("src")).expect("src 디렉터리를 만들어야 한다");
    fs::write(
        root.join("src/models.ts"),
        "export type UserProps = { name: string };\n",
    )
    .expect("타입 모델을 써야 한다");
    fs::write(
        root.join("src/view.tsx"),
        "import type { UserProps as CardProps } from \"./models\";\nexport function Card(props: CardProps) { return <div>{props.name}</div>; }\n",
    )
    .expect("TSX 화면을 써야 한다");

    let result = analyze(AnalysisRequest::new(&root)).expect("TSX 분석이 성공해야 한다");
    let overview = result.overview.expect("Overview가 생성되어야 한다");
    let type_id = overview
        .units
        .iter()
        .find(|unit| unit.kind == CodeUnitKind::TypeAlias && unit.name == "UserProps")
        .map(|unit| unit.id.clone())
        .expect("UserProps 타입 유닛이 있어야 한다");

    assert!(overview.static_graph.edges.iter().any(|reference| {
        reference.kind == ReferenceKind::Uses
            && reference.target_unit_id.as_deref() == Some(type_id.as_str())
            && reference.target_name == "CardProps"
    }));

    fs::remove_dir_all(root).expect("임시 프로젝트를 정리해야 한다");
}
