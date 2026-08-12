//! C++ Boost.Asio의 비동기 callback 등록 경계를 추출한다.
use crate::facts::FactStore;
use crate::frameworks::common::callbacks::{
    add_callback_registrations_many, CallbackRegistrationRule,
};
use crate::frameworks::registry::detector::FrameworkDetection;

const CALLS: &[CallbackRegistrationRule] = &[
    CallbackRegistrationRule {
        framework_id: "cpp.boost_asio",
        call_name: "async_accept",
        callback_argument_indices: &[1],
        method: "BOOST_ASIO_CALLBACK",
    },
    CallbackRegistrationRule {
        framework_id: "cpp.boost_asio",
        call_name: "async_connect",
        callback_argument_indices: &[2],
        method: "BOOST_ASIO_CALLBACK",
    },
    CallbackRegistrationRule {
        framework_id: "cpp.boost_asio",
        call_name: "async_read",
        callback_argument_indices: &[2],
        method: "BOOST_ASIO_CALLBACK",
    },
    CallbackRegistrationRule {
        framework_id: "cpp.boost_asio",
        call_name: "async_write",
        callback_argument_indices: &[2],
        method: "BOOST_ASIO_CALLBACK",
    },
    CallbackRegistrationRule {
        framework_id: "cpp.boost_asio",
        call_name: "post",
        callback_argument_indices: &[1],
        method: "BOOST_ASIO_CALLBACK",
    },
    CallbackRegistrationRule {
        framework_id: "cpp.boost_asio",
        call_name: "dispatch",
        callback_argument_indices: &[1],
        method: "BOOST_ASIO_CALLBACK",
    },
    CallbackRegistrationRule {
        framework_id: "cpp.boost_asio",
        call_name: "defer",
        callback_argument_indices: &[1],
        method: "BOOST_ASIO_CALLBACK",
    },
];

pub(super) fn enrich(facts: &mut FactStore, detections: &[FrameworkDetection]) {
    add_callback_registrations_many(facts, detections, CALLS);
}
