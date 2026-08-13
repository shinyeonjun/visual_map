//! 감지된 프레임워크를 공통 Facts로 변환하는 어댑터 계약.

use crate::facts::FactStore;
use crate::frameworks::registry::detector::FrameworkDetection;
use std::time::Instant;

/// 프레임워크별 문법 보강기가 구현할 계약이다.
pub trait FrameworkFactAdapter {
    fn enrich(&self, facts: &mut FactStore, detections: &[FrameworkDetection]);
}

mod c_family;
mod csharp;
mod dart;
mod ecma;
mod go;
mod java;
mod python;
mod rust;

/// 1단계에서 사용하는 catalog 기반 공통 어댑터다.
///
/// 개별 프레임워크의 route/annotation은 언어 분석기가 이미 공통 HTTP 사실로
/// 바꾼다. 이 어댑터는 그 사실에 감지된 프레임워크 ID와 capability 근거를
/// 연결한다. 이후 프레임워크별 고급 extractor를 이 계약 뒤에 추가할 수 있다.
#[derive(Debug, Default)]
pub struct CatalogFrameworkAdapter;

impl FrameworkFactAdapter for CatalogFrameworkAdapter {
    fn enrich(&self, facts: &mut FactStore, detections: &[FrameworkDetection]) {
        crate::frameworks::common::attach_framework_ids(facts, detections);
        crate::frameworks::common::events::add_language_callback_events(facts);
        run_profiled("c_family", facts, detections, c_family::enrich);
        run_profiled("csharp", facts, detections, csharp::enrich);
        run_profiled("dart", facts, detections, dart::enrich);
        run_profiled("ecma", facts, detections, ecma::enrich);
        run_profiled("go", facts, detections, go::enrich);
        run_profiled("java", facts, detections, java::enrich);
        run_profiled("python", facts, detections, python::enrich);
        run_profiled("rust", facts, detections, rust::enrich);
    }
}

fn run_profiled(
    name: &str,
    facts: &mut FactStore,
    detections: &[FrameworkDetection],
    enrich: fn(&mut FactStore, &[FrameworkDetection]),
) {
    let should_profile = std::env::var_os("VISUAL_MAP_FRAMEWORK_PROFILE").is_some();
    let started = should_profile.then(Instant::now);
    enrich(facts, detections);
    if let Some(started) = started {
        eprintln!(
            "[framework-profile] family={} elapsed_ms={} entrypoints={} resources={}",
            name,
            started.elapsed().as_millis(),
            facts.entrypoints.len(),
            facts.resources.len()
        );
    }
}

pub fn enrich(facts: &mut FactStore, detections: &[FrameworkDetection]) {
    CatalogFrameworkAdapter.enrich(facts, detections);
}
