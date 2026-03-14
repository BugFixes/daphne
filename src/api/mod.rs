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
        StacktraceEvent, StacktraceEventPayload,
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
    Json(payload): Json<StacktraceEventPayload>,
) -> AppResult<Json<crate::domain::IntakeOutcome>> {
    let outcome = state
        .intake_service
        .ingest(payload.into_stacktrace_event())
        .await?;
    Ok(Json(outcome))
}

async fn ingest_go_log(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<GoLogPayload>,
) -> AppResult<Json<crate::domain::IntakeOutcome>> {
    let auth = extract_agent_auth(&headers)?;
    let event = map_go_log_payload(auth, payload)?;
    let outcome = state.intake_service.ingest(event).await?;
    Ok(Json(outcome))
}

async fn ingest_go_bug(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<GoBugPayload>,
) -> AppResult<Json<crate::domain::IntakeOutcome>> {
    let auth = extract_agent_auth(&headers)?;
    let event = map_go_bug_payload(auth, payload)?;
    let outcome = state.intake_service.ingest(event).await?;
    Ok(Json(outcome))
}

fn map_go_log_payload(auth: AgentAuth, payload: GoLogPayload) -> AppResult<StacktraceEvent> {
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

    Ok(StacktraceEvent {
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

fn map_go_bug_payload(auth: AgentAuth, payload: GoBugPayload) -> AppResult<StacktraceEvent> {
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

    Ok(StacktraceEvent {
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
mod tests;
