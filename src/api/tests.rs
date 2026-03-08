use serde_json::json;

use crate::domain::{GoBugPayload, GoLogPayload, Severity};

use super::{AgentAuth, decode_go_bytes, format_location, map_go_bug_payload, map_go_log_payload};

#[test]
fn decodes_go_log_stack_and_headers_into_event() {
    let payload = GoLogPayload {
        log: "panic happened".to_string(),
        level: "error".to_string(),
        file: Some("main.go".to_string()),
        line: Some("42".to_string()),
        line_number: Some(42),
        log_fmt: None,
        stack: Some("Z29yb3V0aW5lIDEgW3J1bm5pbmdd".to_string()),
    };

    let event = map_go_log_payload(
        AgentAuth {
            key: "key".to_string(),
            secret: "secret".to_string(),
        },
        payload,
    )
    .expect("event");

    assert_eq!(event.agent_secret.as_deref(), Some("secret"));
    assert_eq!(event.level, Severity::Error);
    assert!(event.stacktrace.contains("main.go:42"));
    assert!(event.stacktrace.contains("goroutine 1 [running]"));
}

#[test]
fn maps_go_bug_payload_into_stacktrace() {
    let payload = GoBugPayload {
        bug: json!("panic: test"),
        raw: json!("frame 1\nframe 2"),
        bug_line: Some("main.go:42".to_string()),
        file: Some("main.go".to_string()),
        line: Some("42".to_string()),
        line_number: Some(42),
        level: "panic".to_string(),
    };

    let event = map_go_bug_payload(
        AgentAuth {
            key: "key".to_string(),
            secret: "secret".to_string(),
        },
        payload,
    )
    .expect("event");

    assert_eq!(event.level, Severity::Fatal);
    assert!(event.stacktrace.contains("frame 1"));
    assert!(event.stacktrace.contains("panic: test"));
}

#[test]
fn leaves_plain_strings_untouched() {
    assert_eq!(
        decode_go_bytes("plain text"),
        Some("plain text".to_string())
    );
    assert_eq!(
        format_location(Some("main.go"), Some("99")),
        Some("main.go:99".to_string())
    );
}

#[test]
fn accepts_rust_log_payload_shape() {
    let payload = GoLogPayload {
        log: "db timeout".to_string(),
        level: "error".to_string(),
        file: Some("src/main.rs".to_string()),
        line: Some("27".to_string()),
        line_number: Some(27),
        log_fmt: Some("path=src/main.rs level=error msg=\"db timeout\" line=27".to_string()),
        stack: Some(
            "0: app::worker::run\nat /workspace/src/worker.rs:42:7\n1: std::rt::lang_start"
                .to_string(),
        ),
    };

    let event = map_go_log_payload(
        AgentAuth {
            key: "key".to_string(),
            secret: "secret".to_string(),
        },
        payload,
    )
    .expect("event");

    assert_eq!(event.level, Severity::Error);
    assert!(event.stacktrace.contains("src/main.rs:27"));
    assert!(event.stacktrace.contains("app::worker::run"));
    assert!(event.stacktrace.contains("/workspace/src/worker.rs:42:7"));
}

#[test]
fn accepts_rust_bug_payload_shape() {
    let payload = GoBugPayload {
        bug: json!("panic: worker crashed\n\n -> app::worker::run"),
        raw: json!("0: app::worker::run\nat /workspace/src/worker.rs:42:7"),
        bug_line: Some("/workspace/src/worker.rs:42:7".to_string()),
        file: Some("/workspace/src/worker.rs".to_string()),
        line: Some("42".to_string()),
        line_number: Some(42),
        level: "crash".to_string(),
    };

    let event = map_go_bug_payload(
        AgentAuth {
            key: "key".to_string(),
            secret: "secret".to_string(),
        },
        payload,
    )
    .expect("event");

    assert_eq!(event.level, Severity::Fatal);
    assert!(event.stacktrace.contains("worker crashed"));
    assert!(event.stacktrace.contains("/workspace/src/worker.rs:42:7"));
}
