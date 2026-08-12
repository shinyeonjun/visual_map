use code_analysis_engine::facts::EntrypointKind;
use code_analysis_engine::{analyze, AnalysisRequest};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn temporary_project() -> PathBuf {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("visual-map-framework-callbacks-{suffix}"));
    fs::create_dir_all(&path).expect("임시 프로젝트 디렉터리를 만들어야 한다");
    path
}

#[test]
fn c_계열의_콜백_등록을_정적_callback_진입점으로_연결한다() {
    let root = temporary_project();
    fs::write(
        root.join("event_loop.c"),
        r#"
#include <uv.h>
#include <event2/event.h>
#include <gtk/gtk.h>

void on_read(uv_stream_t* stream, ssize_t nread, const uv_buf_t* buf) {}
void on_timer(uv_timer_t* timer) {}
void on_event(evutil_socket_t fd, short events, void* arg) {}
void on_clicked(GtkWidget* widget, gpointer data) {}

void setup(uv_stream_t* stream, uv_timer_t* timer, struct event_base* base, GtkWidget* button) {
  uv_read_start(stream, alloc_buffer, on_read);
  uv_timer_start(timer, on_timer, 10, 0);
  event_new(base, -1, 0, on_event, 0);
  g_signal_connect(button, "clicked", on_clicked, 0);
}
"#,
    )
    .expect("C callback fixture를 써야 한다");
    fs::write(
        root.join("asio.cpp"),
        r#"
#include <boost/asio.hpp>
void on_accept(const boost::system::error_code& error) {}
void setup_asio() { socket.async_accept(peer, on_accept); }
"#,
    )
    .expect("Boost.Asio callback fixture를 써야 한다");
    fs::write(
        root.join("components.cpp"),
        r#"
#include <QtWidgets/QWidget>
#include <afxwin.h>
#include "CoreMinimal.h"
class QtPanel : public QWidget {};
class DesktopApp : public CWinApp {};
class GameActor : public AActor {};
class Receiver : public QObject {
public:
  void handle();
};
void handle() {}
void connect_signals() { QObject::connect(sender, &Sender::triggered, receiver, &Receiver::handle); }
"#,
    )
    .expect("C++ component fixture를 써야 한다");
    fs::write(
        root.join("react.tsx"),
        r#"
import React from "react";
class UserCard extends React.Component {}
"#,
    )
    .expect("React component fixture를 써야 한다");

    let overview = analyze(AnalysisRequest::new(&root))
        .expect("분석이 성공해야 한다")
        .overview
        .expect("Overview가 생성되어야 한다");

    for (framework_id, name) in [
        ("c.libuv", "on_read"),
        ("c.libuv", "on_timer"),
        ("c.libevent", "on_event"),
        ("c.gtk_glib", "on_clicked"),
        ("cpp.boost_asio", "on_accept"),
    ] {
        assert!(
            overview.entrypoints.iter().any(|entrypoint| {
                entrypoint.framework_id.as_deref() == Some(framework_id)
                    && entrypoint.kind == EntrypointKind::Callback
                    && entrypoint.name == name
            }),
            "callback 진입점이 없어: framework={framework_id}, name={name}, entrypoints={:?}",
            overview.entrypoints
        );
    }

    for (name, modifier) in [
        ("QtPanel", "framework:qt-component"),
        ("DesktopApp", "framework:mfc-component"),
        ("GameActor", "framework:unreal-component"),
        ("UserCard", "framework:react-component"),
    ] {
        assert!(
            overview.units.iter().any(|unit| {
                unit.name == name && unit.modifiers.iter().any(|value| value == modifier)
            }),
            "component marker가 없어: name={name}, modifier={modifier}, units={:?}",
            overview.units
        );
    }
    assert!(overview.entrypoints.iter().any(|entrypoint| {
        entrypoint.framework_id.as_deref() == Some("cpp.qt")
            && entrypoint.kind == EntrypointKind::Callback
            && entrypoint.name == "handle"
    }));
}

#[test]
fn 언어_공통_이벤트_등록을_정적_경계로_보존한다() {
    let root = temporary_project();
    fs::write(
        root.join("events.ts"),
        r#"
export function bind() {
  window.addEventListener("click", handleClick);
}
function handleClick(event: Event) {}
"#,
    )
    .expect("DOM 이벤트 fixture를 써야 한다");
    fs::write(
        root.join("window.rs"),
        r#"
fn setup(window: Window) {
  window.on_window_event(|event| handle_window(event));
}
fn handle_window(event: Event) {}
"#,
    )
    .expect("Rust window event fixture를 써야 한다");

    let overview = analyze(AnalysisRequest::new(&root))
        .expect("분석이 성공해야 한다")
        .overview
        .expect("Overview가 생성되어야 한다");

    assert!(overview.entrypoints.iter().any(|entrypoint| {
        entrypoint.kind == EntrypointKind::Event
            && entrypoint.name == "click"
            && entrypoint.method.as_deref() == Some("DOM_EVENT_LISTENER")
    }));
    assert!(overview.entrypoints.iter().any(|entrypoint| {
        entrypoint.kind == EntrypointKind::Callback
            && entrypoint.method.as_deref() == Some("TAURI_WINDOW_EVENT")
    }));

    fs::remove_dir_all(root).expect("이벤트 fixture를 정리해야 한다");
}
