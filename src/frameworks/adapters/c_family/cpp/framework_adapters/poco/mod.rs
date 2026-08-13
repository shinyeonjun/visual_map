//! C++ POCO adapter 경계. HTTP·network component 규칙을 이 모듈에 구현한다.
use crate::facts::{EntrypointKind, FactStore};
use crate::frameworks::common::components::{mark_components, ComponentMarkerRule};
use crate::frameworks::common::inherited::{
    add_inherited_method_entrypoints, InheritedMethodEntrypointRule,
};
use crate::frameworks::registry::detector::FrameworkDetection;

pub(super) fn enrich(facts: &mut FactStore, detections: &[FrameworkDetection]) {
    mark_components(
        facts,
        detections,
        ComponentMarkerRule {
            framework_id: "cpp.poco",
            modifier: "framework:poco-http-component",
            decorator_names: &[],
            call_names: &[],
            signature_tokens: &["HTTPRequestHandler", "HTTPRequestHandlerFactory"],
        },
    );
    add_inherited_method_entrypoints(
        facts,
        detections,
        InheritedMethodEntrypointRule {
            framework_id: "cpp.poco",
            base_type_tokens: &["HTTPRequestHandler"],
            method_names: &["handleRequest"],
            kind: EntrypointKind::Http,
            method: "POCO_HTTP",
        },
    );
    add_inherited_method_entrypoints(
        facts,
        detections,
        InheritedMethodEntrypointRule {
            framework_id: "cpp.poco",
            base_type_tokens: &["HTTPRequestHandlerFactory"],
            method_names: &["createRequestHandler"],
            kind: EntrypointKind::Callback,
            method: "POCO_HANDLER_FACTORY",
        },
    );
}
