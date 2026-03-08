use std::{collections::HashMap, sync::Arc};

use crate::{
    AppError, AppResult,
    domain::{
        Account, Bug, NotificationProvider, Severity, Ticket, TicketPriority, TicketProvider,
    },
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};

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
        let jira = Arc::new(LocalTicketingProvider::new(TicketProvider::Jira));
        let linear = Arc::new(LocalTicketingProvider::new(TicketProvider::Linear));
        let tracklines = Arc::new(LocalTicketingProvider::new(TicketProvider::Tracklines));
        let slack = Arc::new(LocalNotificationProvider::new(NotificationProvider::Slack));
        let teams = Arc::new(LocalNotificationProvider::new(NotificationProvider::Teams));
        let resend = Arc::new(LocalNotificationProvider::new(NotificationProvider::Resend));

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

pub struct LocalTicketingProvider {
    kind: TicketProvider,
}

impl LocalTicketingProvider {
    pub fn new(kind: TicketProvider) -> Self {
        Self { kind }
    }
}

#[async_trait]
impl TicketingProviderClient for LocalTicketingProvider {
    fn kind(&self) -> TicketProvider {
        self.kind
    }

    async fn create_ticket(&self, request: TicketCreateRequest) -> AppResult<RemoteTicket> {
        let prefix = match self.kind {
            TicketProvider::Jira => "JIRA",
            TicketProvider::Linear => "LIN",
            TicketProvider::Tracklines => "TL",
        };
        let remote_id = format!("{prefix}-{}", request.bug.id.simple());
        let remote_url = format!("https://stub.{}/issues/{remote_id}", self.kind);

        tracing::info!(
            provider = %self.kind,
            bug_id = %request.bug.id,
            priority = %request.priority,
            "created stub remote ticket"
        );

        Ok(RemoteTicket {
            remote_id,
            remote_url,
            status: "open".to_string(),
        })
    }

    async fn add_comment(&self, request: TicketCommentRequest) -> AppResult<()> {
        tracing::info!(
            provider = %self.kind,
            ticket_id = %request.ticket.id,
            remote_id = %request.ticket.remote_id,
            comment = %request.comment,
            "added stub ticket comment"
        );
        Ok(())
    }

    async fn update_priority(&self, request: TicketPriorityRequest) -> AppResult<()> {
        tracing::info!(
            provider = %self.kind,
            ticket_id = %request.ticket.id,
            remote_id = %request.ticket.remote_id,
            priority = %request.priority,
            "updated stub ticket priority"
        );
        Ok(())
    }
}

pub struct LocalNotificationProvider {
    kind: NotificationProvider,
}

impl LocalNotificationProvider {
    pub fn new(kind: NotificationProvider) -> Self {
        Self { kind }
    }
}

#[async_trait]
impl NotificationProviderClient for LocalNotificationProvider {
    fn kind(&self) -> NotificationProvider {
        self.kind
    }

    async fn send(&self, request: NotificationRequest) -> AppResult<()> {
        tracing::info!(
            provider = %self.kind,
            account_id = %request.account.id,
            bug_id = %request.bug.id,
            severity = %request.severity,
            "sent stub notification: {}",
            request.message
        );
        Ok(())
    }
}

pub struct HeuristicAiAdvisor;

#[async_trait]
impl AiAdvisor for HeuristicAiAdvisor {
    async fn recommend_fix(&self, bug: &Bug, source_stacktrace: &str) -> AppResult<String> {
        let lower = source_stacktrace.to_ascii_lowercase();
        let recommendation = if lower.contains("nullpointer")
            || lower.contains("nil pointer")
            || lower.contains("none type")
        {
            "Check the failing call site for missing null or nil guards and capture the unexpected input that reaches this code path.".to_string()
        } else if lower.contains("timeout") {
            "Inspect upstream latency, retry policy, and connection pool saturation around the failing dependency before changing application logic.".to_string()
        } else if lower.contains("connection refused") || lower.contains("econnrefused") {
            "Verify dependency availability and configuration first; this stacktrace suggests a network or service boot-order failure rather than a code defect.".to_string()
        } else if lower.contains("index out of bounds") || lower.contains("outofrange") {
            "Validate collection bounds before indexing and capture the input size that triggers the failing branch.".to_string()
        } else {
            format!(
                "Start with the top non-framework frame in the {} stacktrace, reproduce it with the same inputs, and add structured context around the failing path before patching.",
                bug.language
            )
        };

        Ok(recommendation)
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
