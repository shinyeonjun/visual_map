//! C libevent의 정적 callback 등록 경계를 추출한다.
use crate::facts::FactStore;
use crate::frameworks::common::callbacks::{
    add_callback_registrations_many, CallbackRegistrationRule,
};
use crate::frameworks::registry::detector::FrameworkDetection;

const CALLS: &[CallbackRegistrationRule] = &[
    CallbackRegistrationRule {
        framework_id: "c.libevent",
        call_name: "event_new",
        callback_argument_indices: &[3],
        method: "LIBEVENT_CALLBACK",
    },
    CallbackRegistrationRule {
        framework_id: "c.libevent",
        call_name: "event_assign",
        callback_argument_indices: &[4],
        method: "LIBEVENT_CALLBACK",
    },
    CallbackRegistrationRule {
        framework_id: "c.libevent",
        call_name: "evhttp_set_cb",
        callback_argument_indices: &[2],
        method: "LIBEVENT_CALLBACK",
    },
    CallbackRegistrationRule {
        framework_id: "c.libevent",
        call_name: "evconnlistener_new_bind",
        callback_argument_indices: &[1],
        method: "LIBEVENT_CALLBACK",
    },
];

pub(super) fn enrich(facts: &mut FactStore, detections: &[FrameworkDetection]) {
    add_callback_registrations_many(facts, detections, CALLS);
}
