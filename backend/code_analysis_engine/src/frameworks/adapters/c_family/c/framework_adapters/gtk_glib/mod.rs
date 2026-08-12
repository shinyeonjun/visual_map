//! C GTK/GLib의 signal·timer callback 등록 경계를 추출한다.
use crate::facts::FactStore;
use crate::frameworks::common::callbacks::{
    add_callback_registrations_many, CallbackRegistrationRule,
};
use crate::frameworks::registry::detector::FrameworkDetection;

const CALLS: &[CallbackRegistrationRule] = &[
    CallbackRegistrationRule {
        framework_id: "c.gtk_glib",
        call_name: "g_signal_connect",
        callback_argument_indices: &[2],
        method: "GLIB_CALLBACK",
    },
    CallbackRegistrationRule {
        framework_id: "c.gtk_glib",
        call_name: "g_signal_connect_swapped",
        callback_argument_indices: &[2],
        method: "GLIB_CALLBACK",
    },
    CallbackRegistrationRule {
        framework_id: "c.gtk_glib",
        call_name: "g_timeout_add",
        callback_argument_indices: &[1],
        method: "GLIB_CALLBACK",
    },
    CallbackRegistrationRule {
        framework_id: "c.gtk_glib",
        call_name: "g_idle_add",
        callback_argument_indices: &[0],
        method: "GLIB_CALLBACK",
    },
    CallbackRegistrationRule {
        framework_id: "c.gtk_glib",
        call_name: "g_source_set_callback",
        callback_argument_indices: &[1],
        method: "GLIB_CALLBACK",
    },
];

pub(super) fn enrich(facts: &mut FactStore, detections: &[FrameworkDetection]) {
    add_callback_registrations_many(facts, detections, CALLS);
}
