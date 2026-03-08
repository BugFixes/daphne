use std::sync::Arc;

use axum::{
    Json, Router,
    extract::State,
    routing::{get, post},
};
use serde::Serialize;

use crate::{
    AppResult,
    domain::{CreateAccountRequest, CreateAgentRequest, StacktraceEventRequest},
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

#[derive(Serialize)]
struct HealthResponse {
    ok: bool,
}
