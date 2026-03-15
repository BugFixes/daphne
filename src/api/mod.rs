use std::{collections::HashMap, str::FromStr, sync::Arc};

use axum::{
    Json, Router,
    extract::{Path, State},
    http::HeaderMap,
    routing::{get, patch, post},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::Serialize;
use serde_json::Value;

use crate::{
    AppError, AppResult,
    domain::{
        AddOrganizationMemberRequest, AuthenticatedStacktraceEventPayload, CreateAccountRequest,
        CreateAgentRequest, CreateOrganizationRequest, GoBugPayload, GoLogPayload, LogEvent,
        LogEventPayload, Severity, StacktraceEvent, StacktraceEventPayload,
        UpdateOrganizationMembershipRequest,
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
        .route(
            "/v1/organizations",
            post(create_organization).get(list_organizations),
        )
        .route(
            "/v1/organizations/{organization_id}/memberships",
            post(add_organization_member).get(list_organization_memberships),
        )
        .route(
            "/v1/organizations/{organization_id}/memberships/{membership_id}",
            patch(update_organization_membership),
        )
        .route("/v1/accounts", post(create_account))
        .route("/v1/agents", post(create_agent))
        .route("/v1/events/stacktraces", post(ingest_stacktrace))
        .route("/v1/events/bugs", post(ingest_authenticated_stacktrace))
        .route("/v1/events/logs", post(ingest_log_event))
        .route("/v1/log", post(ingest_go_log))
        .route("/v1/bug", post(ingest_go_bug))
        .route("/v1/logs/retention/run", post(run_log_retention))
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

async fn create_organization(
    State(state): State<AppState>,
    Json(request): Json<CreateOrganizationRequest>,
) -> AppResult<Json<crate::domain::OrganizationAccess>> {
    let organization = state.repository.create_organization(request).await?;
    Ok(Json(organization))
}

async fn list_organizations(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> AppResult<Json<Vec<crate::domain::OrganizationAccess>>> {
    let user_email = extract_current_user_email(&headers)?;
    let organizations = state
        .repository
        .list_organizations_for_user(&user_email)
        .await?;
    Ok(Json(organizations))
}

async fn add_organization_member(
    State(state): State<AppState>,
    Path(organization_id): Path<uuid::Uuid>,
    headers: HeaderMap,
    Json(request): Json<AddOrganizationMemberRequest>,
) -> AppResult<Json<crate::domain::MembershipRecord>> {
    let user_email = extract_current_user_email(&headers)?;
    let membership = state
        .repository
        .add_organization_member(organization_id, &user_email, request)
        .await?;
    Ok(Json(membership))
}

async fn list_organization_memberships(
    State(state): State<AppState>,
    Path(organization_id): Path<uuid::Uuid>,
    headers: HeaderMap,
) -> AppResult<Json<Vec<crate::domain::MembershipRecord>>> {
    let user_email = extract_current_user_email(&headers)?;
    let memberships = state
        .repository
        .list_organization_memberships(organization_id, &user_email)
        .await?;
    Ok(Json(memberships))
}

async fn update_organization_membership(
    State(state): State<AppState>,
    Path((organization_id, membership_id)): Path<(uuid::Uuid, uuid::Uuid)>,
    headers: HeaderMap,
    Json(request): Json<UpdateOrganizationMembershipRequest>,
) -> AppResult<Json<crate::domain::MembershipRecord>> {
    let user_email = extract_current_user_email(&headers)?;
    let membership = state
        .repository
        .update_organization_membership(organization_id, membership_id, &user_email, request)
        .await?;
    Ok(Json(membership))
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

async fn ingest_authenticated_stacktrace(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<AuthenticatedStacktraceEventPayload>,
) -> AppResult<Json<crate::domain::IntakeOutcome>> {
    let auth = extract_agent_auth(&headers)?;
    let outcome = state
        .intake_service
        .ingest(payload.into_stacktrace_event(auth.key, auth.secret))
        .await?;
    Ok(Json(outcome))
}

async fn ingest_go_log(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<GoLogPayload>,
) -> AppResult<Json<crate::domain::LogIntakeOutcome>> {
    let auth = extract_agent_auth(&headers)?;
    let event = map_go_log_payload(auth, payload)?;
    let outcome = state.intake_service.ingest_log(event).await?;
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

async fn ingest_log_event(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<LogEventPayload>,
) -> AppResult<Json<crate::domain::LogIntakeOutcome>> {
    let auth = extract_agent_auth(&headers)?;
    let outcome = state
        .intake_service
        .ingest_log(payload.into_log_event(auth.key, auth.secret))
        .await?;
    Ok(Json(outcome))
}

async fn run_log_retention(
    State(state): State<AppState>,
) -> AppResult<Json<crate::domain::LogRetentionOutcome>> {
    let outcome = state
        .intake_service
        .run_log_retention(chrono::Utc::now())
        .await?;
    Ok(Json(outcome))
}

fn map_go_log_payload(auth: AgentAuth, payload: GoLogPayload) -> AppResult<LogEvent> {
    let GoLogPayload {
        log,
        level,
        file,
        line,
        line_number: _,
        log_fmt,
        stack,
    } = payload;
    let level = parse_level(&level)?;
    let mut parts = Vec::new();

    if let Some(location) = format_location(file.as_deref(), line.as_deref()) {
        parts.push(location);
    }
    if !log.trim().is_empty() {
        parts.push(log.clone());
    }
    if let Some(log_fmt) = log_fmt.filter(|value| !value.trim().is_empty()) {
        parts.push(log_fmt);
    }
    if let Some(stack) = stack.and_then(|value| decode_go_bytes(&value)) {
        parts.push(stack);
    }

    let stacktrace = (!parts.is_empty()).then(|| parts.join("\n"));

    Ok(LogEvent {
        agent_key: auth.key,
        agent_secret: Some(auth.secret),
        language: "go".to_string(),
        message: log,
        stacktrace,
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

fn extract_current_user_email(headers: &HeaderMap) -> AppResult<String> {
    headers
        .get("X-User-Email")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
        .ok_or_else(|| AppError::Validation("missing X-User-Email header".to_string()))
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
