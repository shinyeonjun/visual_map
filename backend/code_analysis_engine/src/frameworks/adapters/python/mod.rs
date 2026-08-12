//! Python 프레임워크 adapter 조정 계층이다.
//!
//! Python AST 분석기는 프레임워크를 모르는 원시 사실만 만든다. 이 계층은
//! 감지된 프레임워크별 규칙을 적용해 HTTP 진입점과 DB 리소스를 공통 Facts로
//! 변환한다. 프레임워크별 모듈은 서로의 규칙을 직접 참조하지 않는다.

mod common;
mod django;
mod drf;
mod fastapi;
mod flask;
mod orm;
mod sanic;
mod starlette;

use crate::facts::FactStore;
use crate::frameworks::registry::detector::FrameworkDetection;

pub(super) fn enrich(facts: &mut FactStore, detections: &[FrameworkDetection]) {
    let file_frameworks = common::index_file_frameworks(detections);

    fastapi::enrich(facts, &file_frameworks);
    flask::enrich(facts, &file_frameworks);
    sanic::enrich(facts, &file_frameworks);
    starlette::enrich(facts, &file_frameworks);
    django::enrich(facts, &file_frameworks);
    drf::enrich(facts, &file_frameworks);
    orm::enrich(facts, &file_frameworks);
}
