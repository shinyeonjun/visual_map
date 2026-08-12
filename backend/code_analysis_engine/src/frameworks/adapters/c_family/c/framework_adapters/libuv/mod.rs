//! C libuv의 정적 callback 등록 경계를 추출한다.
use crate::facts::FactStore;
use crate::frameworks::common::callbacks::{
    add_callback_registrations_many, CallbackRegistrationRule,
};
use crate::frameworks::registry::detector::FrameworkDetection;

const CALLS: &[CallbackRegistrationRule] = &[
    CallbackRegistrationRule {
        framework_id: "c.libuv",
        call_name: "uv_read_start",
        callback_argument_indices: &[1, 2],
        method: "LIBUV_CALLBACK",
    },
    CallbackRegistrationRule {
        framework_id: "c.libuv",
        call_name: "uv_write",
        callback_argument_indices: &[4],
        method: "LIBUV_CALLBACK",
    },
    CallbackRegistrationRule {
        framework_id: "c.libuv",
        call_name: "uv_connect",
        callback_argument_indices: &[3],
        method: "LIBUV_CALLBACK",
    },
    CallbackRegistrationRule {
        framework_id: "c.libuv",
        call_name: "uv_fs_open",
        callback_argument_indices: &[5],
        method: "LIBUV_CALLBACK",
    },
    CallbackRegistrationRule {
        framework_id: "c.libuv",
        call_name: "uv_timer_start",
        callback_argument_indices: &[1],
        method: "LIBUV_CALLBACK",
    },
    CallbackRegistrationRule {
        framework_id: "c.libuv",
        call_name: "uv_async_init",
        callback_argument_indices: &[2],
        method: "LIBUV_CALLBACK",
    },
];

pub(super) fn enrich(facts: &mut FactStore, detections: &[FrameworkDetection]) {
    add_callback_registrations_many(facts, detections, CALLS);
}
