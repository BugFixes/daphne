use axum::http::{HeaderMap, HeaderValue};
use serde_json::json;

use crate::domain::{
    AuthenticatedStacktraceEventPayload, GoBugPayload, GoLogPayload, LogEventPayload, Severity,
    StacktraceEventPayload,
};

use super::{
    AgentAuth, decode_go_bytes, extract_current_user_email, format_location, map_go_bug_payload,
    map_go_log_payload,
};

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
    assert_eq!(event.message, "panic happened");
    assert!(event.stacktrace.expect("stacktrace").contains("main.go:42"));
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
    let stacktrace = event.stacktrace.expect("stacktrace");
    assert!(stacktrace.contains("src/main.rs:27"));
    assert!(stacktrace.contains("app::worker::run"));
    assert!(stacktrace.contains("/workspace/src/worker.rs:42:7"));
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

#[test]
fn preserves_canonical_stacktrace_payload_fields() {
    let payload = StacktraceEventPayload {
        agent_key: "key".to_string(),
        agent_secret: Some("secret".to_string()),
        language: "go".to_string(),
        stacktrace: "panic: test".to_string(),
        level: Severity::Error,
        occurred_at: None,
        service: Some("api".to_string()),
        environment: Some("prod".to_string()),
        attributes: std::collections::HashMap::from([(
            "source".to_string(),
            "stacktrace_api".to_string(),
        )]),
    };

    let event = payload.into_stacktrace_event();

    assert_eq!(event.agent_key, "key");
    assert_eq!(event.agent_secret.as_deref(), Some("secret"));
    assert_eq!(event.service.as_deref(), Some("api"));
    assert_eq!(event.environment.as_deref(), Some("prod"));
    assert_eq!(
        event.attributes.get("source").map(String::as_str),
        Some("stacktrace_api")
    );
}

#[test]
fn preserves_authenticated_stacktrace_payload_fields() {
    let payload = AuthenticatedStacktraceEventPayload {
        language: "rust".to_string(),
        stacktrace: "panic: test".to_string(),
        level: Severity::Error,
        occurred_at: None,
        service: Some("api".to_string()),
        environment: Some("prod".to_string()),
        attributes: std::collections::HashMap::from([(
            "source".to_string(),
            "header_stacktrace_api".to_string(),
        )]),
    };

    let event = payload.into_stacktrace_event("key".to_string(), "secret".to_string());

    assert_eq!(event.agent_key, "key");
    assert_eq!(event.agent_secret.as_deref(), Some("secret"));
    assert_eq!(event.service.as_deref(), Some("api"));
    assert_eq!(event.environment.as_deref(), Some("prod"));
}

#[test]
fn preserves_canonical_log_payload_fields() {
    let payload = LogEventPayload {
        language: "rust".to_string(),
        message: "db timeout".to_string(),
        stacktrace: Some("frame_one".to_string()),
        level: Severity::Warn,
        occurred_at: None,
        service: Some("api".to_string()),
        environment: Some("prod".to_string()),
        attributes: std::collections::HashMap::from([(
            "source".to_string(),
            "log_api".to_string(),
        )]),
    };

    let event = payload.into_log_event("key".to_string(), "secret".to_string());

    assert_eq!(event.agent_key, "key");
    assert_eq!(event.agent_secret.as_deref(), Some("secret"));
    assert_eq!(event.message, "db timeout");
    assert_eq!(event.stacktrace.as_deref(), Some("frame_one"));
}

#[test]
fn extracts_current_user_email_header() {
    let mut headers = HeaderMap::new();
    headers.insert(
        "X-User-Email",
        HeaderValue::from_static("owner@example.com"),
    );

    let email = extract_current_user_email(&headers).expect("email");

    assert_eq!(email, "owner@example.com");
}

#[test]
fn requires_current_user_email_header() {
    let error = extract_current_user_email(&HeaderMap::new()).expect_err("missing header");

    assert_eq!(
        error.to_string(),
        "validation failed: missing X-User-Email header"
    );
}
