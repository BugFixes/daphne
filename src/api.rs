use std::{collections::HashMap, str::FromStr, sync::Arc};

use axum::{
    Json, Router,
    extract::State,
    http::HeaderMap,
    routing::{get, post},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::Serialize;
use serde_json::Value;

use crate::{
    AppError, AppResult,
    domain::{
        CreateAccountRequest, CreateAgentRequest, GoBugPayload, GoLogPayload, Severity,
        StacktraceEventRequest,
    },
    repository::Repository,
    service::IntakeService,
};

#[derive(Clone)]
pub struct AppState {
    repository: Arc<Repository>,
    intake_service: Arc<IntakeService>,
}

pub fn router(repository: Arc<Repository>, intake_service: Arc<IntakeService>) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/v1/accounts", post(create_account))
        .route("/v1/agents", post(create_agent))
        .route("/v1/events/stacktraces", post(ingest_stacktrace))
        .route("/v1/log", post(ingest_go_log))
        .route("/v1/bug", post(ingest_go_bug))
        .route("/log", post(ingest_go_log))
        .route("/bug", post(ingest_go_bug))
        .with_state(AppState {
            repository,
            intake_service,
        })
}

async fn healthz() -> Json<HealthResponse> {
    Json(HealthResponse { ok: true })
}

async fn create_account(
    State(state): State<AppState>,
    Json(request): Json<CreateAccountRequest>,
) -> AppResult<Json<crate::domain::Account>> {
    let account = state.repository.create_account(request).await?;
    Ok(Json(account))
}

async fn create_agent(
    State(state): State<AppState>,
    Json(request): Json<CreateAgentRequest>,
) -> AppResult<Json<crate::domain::Agent>> {
    let agent = state.repository.create_agent(request).await?;
    Ok(Json(agent))
}

async fn ingest_stacktrace(
    State(state): State<AppState>,
    Json(request): Json<StacktraceEventRequest>,
) -> AppResult<Json<crate::domain::IntakeOutcome>> {
    let outcome = state.intake_service.ingest(request).await?;
    Ok(Json(outcome))
}

async fn ingest_go_log(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<GoLogPayload>,
) -> AppResult<Json<crate::domain::IntakeOutcome>> {
    let auth = extract_agent_auth(&headers)?;
    let request = map_go_log_payload(auth, payload)?;
    let outcome = state.intake_service.ingest(request).await?;
    Ok(Json(outcome))
}

async fn ingest_go_bug(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<GoBugPayload>,
) -> AppResult<Json<crate::domain::IntakeOutcome>> {
    let auth = extract_agent_auth(&headers)?;
    let request = map_go_bug_payload(auth, payload)?;
    let outcome = state.intake_service.ingest(request).await?;
    Ok(Json(outcome))
}

fn map_go_log_payload(auth: AgentAuth, payload: GoLogPayload) -> AppResult<StacktraceEventRequest> {
    let level = parse_level(&payload.level)?;
    let mut parts = Vec::new();

    if let Some(location) = format_location(payload.file.as_deref(), payload.line.as_deref()) {
        parts.push(location);
    }
    if !payload.log.trim().is_empty() {
        parts.push(payload.log);
    }
    if let Some(log_fmt) = payload.log_fmt.filter(|value| !value.trim().is_empty()) {
        parts.push(log_fmt);
    }
    if let Some(stack) = payload.stack.and_then(|value| decode_go_bytes(&value)) {
        parts.push(stack);
    }

    Ok(StacktraceEventRequest {
        agent_key: auth.key,
        agent_secret: Some(auth.secret),
        language: "go".to_string(),
        stacktrace: parts.join("\n"),
        level,
        occurred_at: None,
        service: None,
        environment: None,
        attributes: HashMap::from([("source".to_string(), "go_logs".to_string())]),
    })
}

fn map_go_bug_payload(auth: AgentAuth, payload: GoBugPayload) -> AppResult<StacktraceEventRequest> {
    let level = parse_level(&payload.level)?;
    let mut parts = Vec::new();

    if let Some(location) = format_location(payload.file.as_deref(), payload.line.as_deref()) {
        parts.push(location);
    }
    if let Some(bug_line) = payload.bug_line.filter(|value| !value.trim().is_empty()) {
        parts.push(bug_line);
    }
    if let Some(bug) = stringify_json_value(&payload.bug) {
        parts.push(bug);
    }
    if let Some(raw) = stringify_json_value(&payload.raw) {
        parts.push(raw);
    }

    Ok(StacktraceEventRequest {
        agent_key: auth.key,
        agent_secret: Some(auth.secret),
        language: "go".to_string(),
        stacktrace: parts.join("\n"),
        level,
        occurred_at: None,
        service: None,
        environment: None,
        attributes: HashMap::from([("source".to_string(), "go_middleware".to_string())]),
    })
}

fn extract_agent_auth(headers: &HeaderMap) -> AppResult<AgentAuth> {
    let key = headers
        .get("X-API-KEY")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| AppError::Validation("missing X-API-KEY header".to_string()))?;
    let secret = headers
        .get("X-API-SECRET")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| AppError::Validation("missing X-API-SECRET header".to_string()))?;

    Ok(AgentAuth {
        key: key.to_string(),
        secret: secret.to_string(),
    })
}

fn parse_level(value: &str) -> AppResult<Severity> {
    Severity::from_str(&value.trim().to_ascii_lowercase())
}

fn format_location(file: Option<&str>, line: Option<&str>) -> Option<String> {
    match (
        file.filter(|value| !value.trim().is_empty()),
        line.filter(|value| !value.trim().is_empty()),
    ) {
        (Some(file), Some(line)) => Some(format!("{file}:{line}")),
        (Some(file), None) => Some(file.to_string()),
        _ => None,
    }
}

fn decode_go_bytes(value: &str) -> Option<String> {
    if value.trim().is_empty() {
        return None;
    }

    let decoded = STANDARD.decode(value).ok();
    match decoded.and_then(|bytes| String::from_utf8(bytes).ok()) {
        Some(text) if !text.trim().is_empty() => Some(text),
        _ => Some(value.to_string()),
    }
}

fn stringify_json_value(value: &Value) -> Option<String> {
    match value {
        Value::Null => None,
        Value::String(text) if text.trim().is_empty() => None,
        Value::String(text) => Some(text.clone()),
        other => Some(other.to_string()),
    }
}

#[derive(Serialize)]
struct HealthResponse {
    ok: bool,
}

struct AgentAuth {
    key: String,
    secret: String,
}

#[cfg(test)]
mod tests {
    use super::{
        AgentAuth, decode_go_bytes, format_location, map_go_bug_payload, map_go_log_payload,
    };
    use crate::domain::{GoBugPayload, GoLogPayload, Severity};
    use serde_json::json;

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
}
