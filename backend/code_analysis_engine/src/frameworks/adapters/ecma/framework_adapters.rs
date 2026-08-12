//! JavaScript·TypeScript framework별 adapter 조정 계층.
//!
//! registry의 javascript.* ID는 JavaScript와 TypeScript 양쪽에 적용되므로
//! 계열 공통 adapter를 한 번만 실행한다.

mod angular;
mod express;
mod fastify;
mod koa;
mod nestjs;
mod nextjs;
mod nuxt;
mod react;
mod sveltekit;
mod tauri;
mod vue;

use crate::facts::FactStore;
use crate::frameworks::registry::detector::FrameworkDetection;

pub(super) fn enrich(facts: &mut FactStore, detections: &[FrameworkDetection]) {
    react::enrich(facts, detections);
    nextjs::enrich(facts, detections);
    angular::enrich(facts, detections);
    vue::enrich(facts, detections);
    nuxt::enrich(facts, detections);
    sveltekit::enrich(facts, detections);
    express::enrich(facts, detections);
    fastify::enrich(facts, detections);
    nestjs::enrich(facts, detections);
    koa::enrich(facts, detections);
    tauri::enrich(facts, detections);
}
