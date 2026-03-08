use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;

use crate::{
    AppError, AppResult,
    domain::{Account, Bug, Ticket, TicketPriority, TicketProvider},
};

pub mod github;
pub mod jira;
pub mod linear;
pub mod tracklines;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone)]
pub struct TicketCreateRequest {
    pub bug: Bug,
    pub account: Account,
    pub priority: TicketPriority,
    pub recommendation: String,
    pub source_stacktrace: String,
}

#[derive(Debug, Clone)]
pub struct RemoteTicket {
    pub remote_id: String,
    pub remote_url: String,
    pub status: String,
}

#[derive(Debug, Clone)]
pub struct TicketCommentRequest {
    pub ticket: Ticket,
    pub comment: String,
}

#[derive(Debug, Clone)]
pub struct TicketPriorityRequest {
    pub ticket: Ticket,
    pub priority: TicketPriority,
}

#[async_trait]
pub trait TicketingProviderClient: Send + Sync {
    fn kind(&self) -> TicketProvider;
    async fn create_ticket(&self, request: TicketCreateRequest) -> AppResult<RemoteTicket>;
    async fn add_comment(&self, request: TicketCommentRequest) -> AppResult<()>;
    async fn update_priority(&self, request: TicketPriorityRequest) -> AppResult<()>;
}

pub use github::GithubProvider;
pub use jira::JiraProvider;
pub use linear::LinearProvider;
pub use tracklines::TracklinesProvider;

pub struct TicketingRegistry {
    providers: HashMap<TicketProvider, Arc<dyn TicketingProviderClient>>,
}

impl TicketingRegistry {
    pub fn get(&self, kind: TicketProvider) -> AppResult<Arc<dyn TicketingProviderClient>> {
        self.providers
            .get(&kind)
            .cloned()
            .ok_or_else(|| AppError::Internal(format!("ticketing provider not registered: {kind}")))
    }
}

impl Default for TicketingRegistry {
    fn default() -> Self {
        let jira = Arc::new(JiraProvider);
        let github = Arc::new(GithubProvider);
        let linear = Arc::new(LinearProvider);
        let tracklines = Arc::new(TracklinesProvider);

        Self {
            providers: HashMap::from([
                (
                    TicketProvider::Jira,
                    jira as Arc<dyn TicketingProviderClient>,
                ),
                (
                    TicketProvider::Github,
                    github as Arc<dyn TicketingProviderClient>,
                ),
                (
                    TicketProvider::Linear,
                    linear as Arc<dyn TicketingProviderClient>,
                ),
                (
                    TicketProvider::Tracklines,
                    tracklines as Arc<dyn TicketingProviderClient>,
                ),
            ]),
        }
    }
}

pub fn build_escalation_comment(recent_count: i64, minutes: i64) -> String {
    format!(
        "The same stacktrace re-occurred {recent_count} times within the last {minutes} minutes, so priority was increased."
    )
}

pub fn build_repeat_comment(when: chrono::DateTime<chrono::Utc>) -> String {
    format!(
        "The same stacktrace occurred again at {}.",
        when.to_rfc3339()
    )
}

fn build_stub_remote_ticket(provider: TicketProvider, bug_id: uuid::Uuid) -> RemoteTicket {
    let prefix = match provider {
        TicketProvider::Jira => "JIRA",
        TicketProvider::Github => "GH",
        TicketProvider::Linear => "LIN",
        TicketProvider::Tracklines => "TL",
    };
    let remote_id = format!("{prefix}-{}", bug_id.simple());
    let remote_url = format!("https://stub.{provider}/issues/{remote_id}");

    RemoteTicket {
        remote_id,
        remote_url,
        status: "open".to_string(),
    }
}

fn log_stub_ticket_creation(provider: TicketProvider, request: &TicketCreateRequest) {
    tracing::info!(
        provider = %provider,
        bug_id = %request.bug.id,
        priority = %request.priority,
        "created stub remote ticket"
    );
}

fn log_stub_ticket_comment(provider: TicketProvider, request: &TicketCommentRequest) {
    tracing::info!(
        provider = %provider,
        ticket_id = %request.ticket.id,
        remote_id = %request.ticket.remote_id,
        comment = %request.comment,
        "added stub ticket comment"
    );
}

fn log_stub_ticket_priority(provider: TicketProvider, request: &TicketPriorityRequest) {
    tracing::info!(
        provider = %provider,
        ticket_id = %request.ticket.id,
        remote_id = %request.ticket.remote_id,
        priority = %request.priority,
        "updated stub ticket priority"
    );
}
