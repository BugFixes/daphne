use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::{
    AppError, AppResult,
    domain::{
        Account, Bug, NotificationProvider, Severity, Ticket, TicketPriority, TicketProvider,
    },
};

pub mod ai;
pub mod jira;
pub mod linear;
pub mod resend;
pub mod slack;
pub mod teams;
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

#[derive(Debug, Clone)]
pub struct NotificationRequest {
    pub account: Account,
    pub bug: Bug,
    pub severity: Severity,
    pub message: String,
}

#[async_trait]
pub trait TicketingProviderClient: Send + Sync {
    fn kind(&self) -> TicketProvider;
    async fn create_ticket(&self, request: TicketCreateRequest) -> AppResult<RemoteTicket>;
    async fn add_comment(&self, request: TicketCommentRequest) -> AppResult<()>;
    async fn update_priority(&self, request: TicketPriorityRequest) -> AppResult<()>;
}

#[async_trait]
pub trait NotificationProviderClient: Send + Sync {
    fn kind(&self) -> NotificationProvider;
    async fn send(&self, request: NotificationRequest) -> AppResult<()>;
}

#[async_trait]
pub trait AiAdvisor: Send + Sync {
    async fn recommend_fix(&self, bug: &Bug, source_stacktrace: &str) -> AppResult<String>;
}

pub use ai::HeuristicAiAdvisor;
pub use jira::JiraProvider;
pub use linear::LinearProvider;
pub use resend::ResendProvider;
pub use slack::SlackProvider;
pub use teams::TeamsProvider;
pub use tracklines::TracklinesProvider;

pub struct ProviderRegistry {
    ticketing: HashMap<TicketProvider, Arc<dyn TicketingProviderClient>>,
    notifications: HashMap<NotificationProvider, Arc<dyn NotificationProviderClient>>,
    ai: Arc<dyn AiAdvisor>,
}

impl ProviderRegistry {
    pub fn ticketing(&self, kind: TicketProvider) -> AppResult<Arc<dyn TicketingProviderClient>> {
        self.ticketing
            .get(&kind)
            .cloned()
            .ok_or_else(|| AppError::Internal(format!("ticketing provider not registered: {kind}")))
    }

    pub fn notifications(
        &self,
        kind: NotificationProvider,
    ) -> AppResult<Arc<dyn NotificationProviderClient>> {
        self.notifications.get(&kind).cloned().ok_or_else(|| {
            AppError::Internal(format!("notification provider not registered: {kind}"))
        })
    }

    pub fn ai(&self) -> Arc<dyn AiAdvisor> {
        self.ai.clone()
    }
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        let jira = Arc::new(JiraProvider);
        let linear = Arc::new(LinearProvider);
        let tracklines = Arc::new(TracklinesProvider);
        let slack = Arc::new(SlackProvider);
        let teams = Arc::new(TeamsProvider);
        let resend = Arc::new(ResendProvider);

        Self {
            ticketing: HashMap::from([
                (
                    TicketProvider::Jira,
                    jira as Arc<dyn TicketingProviderClient>,
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
            notifications: HashMap::from([
                (
                    NotificationProvider::Slack,
                    slack as Arc<dyn NotificationProviderClient>,
                ),
                (
                    NotificationProvider::Teams,
                    teams as Arc<dyn NotificationProviderClient>,
                ),
                (
                    NotificationProvider::Resend,
                    resend as Arc<dyn NotificationProviderClient>,
                ),
            ]),
            ai: Arc::new(HeuristicAiAdvisor),
        }
    }
}

pub fn build_notification_message(account: &Account, bug: &Bug, when: DateTime<Utc>) -> String {
    format!(
        "[{}] {} bug {} reached {} occurrences as of {}",
        account.name,
        bug.language,
        bug.id,
        bug.occurrence_count,
        when.to_rfc3339()
    )
}

pub fn build_escalation_comment(recent_count: i64, minutes: i64) -> String {
    format!(
        "The same stacktrace re-occurred {recent_count} times within the last {minutes} minutes, so priority was increased."
    )
}

pub fn build_repeat_comment(when: DateTime<Utc>) -> String {
    format!(
        "The same stacktrace occurred again at {}.",
        when.to_rfc3339()
    )
}
