//! React class component 경계를 정적 CodeUnit modifier로 보존한다.
use crate::facts::FactStore;
use crate::frameworks::common::components::{mark_components, ComponentMarkerRule};
use crate::frameworks::registry::detector::FrameworkDetection;

pub(super) fn enrich(facts: &mut FactStore, detections: &[FrameworkDetection]) {
    mark_components(
        facts,
        detections,
        ComponentMarkerRule {
            framework_id: "javascript.react",
            modifier: "framework:react-component",
            decorator_names: &[],
            call_names: &["memo", "forwardRef", "createContext"],
            signature_tokens: &[
                "React.Component",
                "PureComponent",
                "FunctionComponent",
                "FC<",
            ],
        },
    );
}
