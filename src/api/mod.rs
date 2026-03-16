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
use tower_http::cors::{Any, CorsLayer};

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
use uuid::Uuid;

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
        .route("/api/dashboard/bugs", get(list_bugs))
        .route("/api/dashboard/bugs/{bug_id}", get(get_bug_detail))
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
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
    let clerk_user_id = extract_current_clerk_user_id(&headers)?;
    let organizations = state
        .repository
        .list_organizations_for_user(&clerk_user_id)
        .await?;
    Ok(Json(organizations))
}

async fn add_organization_member(
    State(state): State<AppState>,
    Path(organization_id): Path<uuid::Uuid>,
    headers: HeaderMap,
    Json(request): Json<AddOrganizationMemberRequest>,
) -> AppResult<Json<crate::domain::MembershipRecord>> {
    let clerk_user_id = extract_current_clerk_user_id(&headers)?;
    let membership = state
        .repository
        .add_organization_member(organization_id, &clerk_user_id, request)
        .await?;
    Ok(Json(membership))
}

async fn list_organization_memberships(
    State(state): State<AppState>,
    Path(organization_id): Path<uuid::Uuid>,
    headers: HeaderMap,
) -> AppResult<Json<Vec<crate::domain::MembershipRecord>>> {
    let clerk_user_id = extract_current_clerk_user_id(&headers)?;
    let memberships = state
        .repository
        .list_organization_memberships(organization_id, &clerk_user_id)
        .await?;
    Ok(Json(memberships))
}

async fn update_organization_membership(
    State(state): State<AppState>,
    Path((organization_id, membership_id)): Path<(uuid::Uuid, uuid::Uuid)>,
    headers: HeaderMap,
    Json(request): Json<UpdateOrganizationMembershipRequest>,
) -> AppResult<Json<crate::domain::MembershipRecord>> {
    let clerk_user_id = extract_current_clerk_user_id(&headers)?;
    let membership = state
        .repository
        .update_organization_membership(organization_id, membership_id, &clerk_user_id, request)
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

async fn list_bugs(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> AppResult<Json<DashboardBugListResponse>> {
    let clerk_org_id = extract_optional_clerk_org_id(&headers);
    let rows = state.repository.list_bugs(clerk_org_id.as_deref()).await?;
    let bugs: Vec<DashboardBugSummary> = rows
        .into_iter()
        .map(|row| {
            let title = first_meaningful_line(&row.normalized_stacktrace).unwrap_or_else(|| {
                first_meaningful_line(&row.latest_stacktrace).unwrap_or_default()
            });
            let tone = severity_to_tone(&row.severity);
            DashboardBugSummary {
                id: row.id,
                title,
                severity: row.severity,
                language: row.language,
                first_seen_at: row.first_seen_at,
                last_seen_at: row.last_seen_at,
                occurrence_count: row.occurrence_count,
                ticket_status: row.ticket_status.unwrap_or_else(|| "none".to_string()),
                ticket_provider: row.ticket_provider.unwrap_or_else(|| "—".to_string()),
                notification_status: row.notification_status,
                account_name: row.account_name,
                agent_name: row.agent_name,
                tone,
            }
        })
        .collect();

    Ok(Json(DashboardBugListResponse {
        title: "Bug inbox".to_string(),
        summary: "Incoming stacktraces, deduplication clusters, and investigation entrypoints for the operator shift.".to_string(),
        bugs,
    }))
}

async fn get_bug_detail(
    State(state): State<AppState>,
    Path(bug_id): Path<String>,
    headers: HeaderMap,
) -> AppResult<Json<DashboardBugDetail>> {
    let clerk_org_id = extract_optional_clerk_org_id(&headers);
    let bug_uuid = Uuid::parse_str(&bug_id)?;
    let bug = state
        .repository
        .find_bug_by_id_scoped(bug_uuid, clerk_org_id.as_deref())
        .await?
        .ok_or_else(|| AppError::NotFound(format!("bug {bug_id}")))?;

    let account = state.repository.find_account_by_id(bug.account_id).await?;
    let agent = state.repository.find_agent_by_id(bug.agent_id).await?;
    let occurrences = state.repository.list_occurrences_for_bug(bug_uuid).await?;
    let ticket = state.repository.find_ticket_for_bug(bug_uuid).await?;
    let notifications = state
        .repository
        .list_notifications_for_bug(bug_uuid)
        .await?;
    let notification_events = state
        .repository
        .list_notification_events_for_bug(bug_uuid)
        .await?;

    let title = first_meaningful_line(&bug.normalized_stacktrace)
        .unwrap_or_else(|| first_meaningful_line(&bug.latest_stacktrace).unwrap_or_default());
    let tone = severity_to_tone(&bug.severity.to_string());

    let tickets: Vec<DashboardTicket> = ticket
        .into_iter()
        .map(|t| DashboardTicket {
            id: t.id.to_string(),
            provider: t.provider.to_string(),
            remote_id: t.remote_id,
            remote_url: t.remote_url,
            priority: t.priority.to_string(),
            status: t.status,
            created_at: t.created_at.to_rfc3339(),
        })
        .collect();

    let occ_items: Vec<DashboardOccurrence> = occurrences
        .into_iter()
        .rev()
        .take(20)
        .map(|o| DashboardOccurrence {
            id: o.id.to_string(),
            occurred_at: o.occurred_at.to_rfc3339(),
            severity: o.severity.to_string(),
            environment: o.environment.unwrap_or_else(|| "—".to_string()),
            service: o.service.unwrap_or_else(|| "—".to_string()),
            agent: agent
                .as_ref()
                .map(|a| a.name.clone())
                .unwrap_or_else(|| "—".to_string()),
        })
        .collect();

    let ntf_items: Vec<DashboardNotification> = notifications
        .into_iter()
        .map(|n| DashboardNotification {
            id: n.id.to_string(),
            provider: n.provider.to_string(),
            message: n.message,
            sent_at: n.sent_at.to_rfc3339(),
        })
        .collect();

    let ntf_event_items: Vec<DashboardNotificationEvent> = notification_events
        .into_iter()
        .map(|e| DashboardNotificationEvent {
            id: e.id.to_string(),
            provider: e.provider.to_string(),
            status: e.status.to_string(),
            reason: e.reason,
            severity: e.severity.to_string(),
            ticket_action: e.ticket_action.to_string(),
            occurred_at: e.occurred_at.to_rfc3339(),
        })
        .collect();

    Ok(Json(DashboardBugDetail {
        id: bug.id.to_string(),
        title,
        severity: bug.severity.to_string(),
        language: bug.language,
        first_seen_at: bug.first_seen_at.to_rfc3339(),
        last_seen_at: bug.last_seen_at.to_rfc3339(),
        occurrence_count: bug.occurrence_count as i32,
        tone,
        latest_stacktrace: bug.latest_stacktrace,
        normalized_stacktrace: bug.normalized_stacktrace,
        stacktrace_hash: bug.stacktrace_hash,
        account_name: account.map(|a| a.name).unwrap_or_else(|| "—".to_string()),
        agent_name: agent.map(|a| a.name).unwrap_or_else(|| "—".to_string()),
        occurrences: occ_items,
        tickets,
        notifications: ntf_items,
        notification_events: ntf_event_items,
    }))
}

fn first_meaningful_line(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_string)
}

fn severity_to_tone(severity: &str) -> String {
    match severity {
        "fatal" | "error" => "critical".to_string(),
        "warn" => "warn".to_string(),
        "info" | "debug" => "good".to_string(),
        _ => "neutral".to_string(),
    }
}

#[derive(Serialize)]
struct DashboardBugListResponse {
    title: String,
    summary: String,
    bugs: Vec<DashboardBugSummary>,
}

#[derive(Serialize)]
struct DashboardBugSummary {
    id: String,
    title: String,
    severity: String,
    language: String,
    first_seen_at: String,
    last_seen_at: String,
    occurrence_count: i32,
    ticket_status: String,
    ticket_provider: String,
    notification_status: String,
    account_name: String,
    agent_name: String,
    tone: String,
}

#[derive(Serialize)]
struct DashboardBugDetail {
    id: String,
    title: String,
    severity: String,
    language: String,
    first_seen_at: String,
    last_seen_at: String,
    occurrence_count: i32,
    tone: String,
    latest_stacktrace: String,
    normalized_stacktrace: String,
    stacktrace_hash: String,
    account_name: String,
    agent_name: String,
    occurrences: Vec<DashboardOccurrence>,
    tickets: Vec<DashboardTicket>,
    notifications: Vec<DashboardNotification>,
    notification_events: Vec<DashboardNotificationEvent>,
}

#[derive(Serialize)]
struct DashboardOccurrence {
    id: String,
    occurred_at: String,
    severity: String,
    environment: String,
    service: String,
    agent: String,
}

#[derive(Serialize)]
struct DashboardTicket {
    id: String,
    provider: String,
    remote_id: String,
    remote_url: String,
    priority: String,
    status: String,
    created_at: String,
}

#[derive(Serialize)]
struct DashboardNotification {
    id: String,
    provider: String,
    message: String,
    sent_at: String,
}

#[derive(Serialize)]
struct DashboardNotificationEvent {
    id: String,
    provider: String,
    status: String,
    reason: String,
    severity: String,
    ticket_action: String,
    occurred_at: String,
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

fn extract_optional_clerk_org_id(headers: &HeaderMap) -> Option<String> {
    headers
        .get("X-Clerk-Org-Id")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.trim().is_empty())
        .map(|value| value.trim().to_string())
}

fn extract_current_clerk_user_id(headers: &HeaderMap) -> AppResult<String> {
    headers
        .get("X-Clerk-User-Id")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.trim().is_empty())
        .map(|value| value.trim().to_string())
        .ok_or_else(|| AppError::Validation("missing X-Clerk-User-Id header".to_string()))
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
