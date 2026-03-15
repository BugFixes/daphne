use std::{collections::HashMap, fmt, str::FromStr};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::{AppError, AppResult};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Debug,
    Info,
    Warn,
    #[default]
    Error,
    Fatal,
}

impl Severity {
    pub fn rank(self) -> u8 {
        match self {
            Self::Debug => 1,
            Self::Info => 2,
            Self::Warn => 3,
            Self::Error => 4,
            Self::Fatal => 5,
        }
    }
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::Debug => "debug",
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Error => "error",
            Self::Fatal => "fatal",
        };
        write!(f, "{value}")
    }
}

impl FromStr for Severity {
    type Err = AppError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "debug" => Ok(Self::Debug),
            "info" => Ok(Self::Info),
            "warn" | "warning" => Ok(Self::Warn),
            "error" => Ok(Self::Error),
            "fatal" | "panic" | "crash" => Ok(Self::Fatal),
            _ => Err(AppError::Validation(format!(
                "unsupported severity: {value}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TicketProvider {
    Jira,
    Github,
    Linear,
    Tracklines,
}

impl fmt::Display for TicketProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::Jira => "jira",
            Self::Github => "github",
            Self::Linear => "linear",
            Self::Tracklines => "tracklines",
        };
        write!(f, "{value}")
    }
}

impl FromStr for TicketProvider {
    type Err = AppError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "jira" => Ok(Self::Jira),
            "github" => Ok(Self::Github),
            "linear" => Ok(Self::Linear),
            "tracklines" => Ok(Self::Tracklines),
            _ => Err(AppError::Validation(format!(
                "unsupported ticket provider: {value}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationProvider {
    Slack,
    Teams,
    Resend,
}

impl fmt::Display for NotificationProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::Slack => "slack",
            Self::Teams => "teams",
            Self::Resend => "resend",
        };
        write!(f, "{value}")
    }
}

impl FromStr for NotificationProvider {
    type Err = AppError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "slack" => Ok(Self::Slack),
            "teams" => Ok(Self::Teams),
            "resend" => Ok(Self::Resend),
            _ => Err(AppError::Validation(format!(
                "unsupported notification provider: {value}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountProviderKind {
    Ticketing,
    Notification,
    Ai,
}

impl fmt::Display for AccountProviderKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::Ticketing => "ticketing",
            Self::Notification => "notification",
            Self::Ai => "ai",
        };
        write!(f, "{value}")
    }
}

impl FromStr for AccountProviderKind {
    type Err = AppError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "ticketing" => Ok(Self::Ticketing),
            "notification" => Ok(Self::Notification),
            "ai" => Ok(Self::Ai),
            _ => Err(AppError::Validation(format!(
                "unsupported account provider kind: {value}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrganizationRole {
    Owner,
    Admin,
    Member,
}

impl OrganizationRole {
    pub fn can_manage_memberships(self) -> bool {
        matches!(self, Self::Owner | Self::Admin)
    }
}

impl fmt::Display for OrganizationRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::Owner => "owner",
            Self::Admin => "admin",
            Self::Member => "member",
        };
        write!(f, "{value}")
    }
}

impl FromStr for OrganizationRole {
    type Err = AppError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "owner" => Ok(Self::Owner),
            "admin" => Ok(Self::Admin),
            "member" => Ok(Self::Member),
            _ => Err(AppError::Validation(format!(
                "unsupported organization role: {value}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TicketPriority {
    Low,
    Medium,
    High,
    Critical,
}

impl TicketPriority {
    pub fn from_severity(level: Severity) -> Self {
        match level {
            Severity::Debug | Severity::Info => Self::Low,
            Severity::Warn => Self::Medium,
            Severity::Error => Self::High,
            Severity::Fatal => Self::Critical,
        }
    }

    pub fn rank(self) -> u8 {
        match self {
            Self::Low => 1,
            Self::Medium => 2,
            Self::High => 3,
            Self::Critical => 4,
        }
    }

    pub fn escalated(self) -> Self {
        match self {
            Self::Low => Self::Medium,
            Self::Medium => Self::High,
            Self::High | Self::Critical => Self::Critical,
        }
    }
}

impl fmt::Display for TicketPriority {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Critical => "critical",
        };
        write!(f, "{value}")
    }
}

impl FromStr for TicketPriority {
    type Err = AppError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "low" => Ok(Self::Low),
            "medium" => Ok(Self::Medium),
            "high" => Ok(Self::High),
            "critical" => Ok(Self::Critical),
            _ => Err(AppError::Validation(format!(
                "unsupported ticket priority: {value}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Organization {
    pub id: Uuid,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: Uuid,
    pub email: String,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Membership {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub user_id: Uuid,
    pub role: OrganizationRole,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganizationAccess {
    pub organization: Organization,
    pub membership: Membership,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MembershipRecord {
    pub membership: Membership,
    pub user: User,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub name: String,
    pub create_tickets: bool,
    pub ticket_provider: TicketProvider,
    pub ticketing_api_key: Option<String>,
    pub notification_provider: NotificationProvider,
    pub notification_api_key: Option<String>,
    pub ai_enabled: bool,
    pub use_managed_ai: bool,
    pub ai_api_key: Option<String>,
    pub notify_min_level: Severity,
    pub rapid_occurrence_window_minutes: i64,
    pub rapid_occurrence_threshold: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountProviderConfig {
    pub id: Uuid,
    pub account_id: Uuid,
    pub kind: AccountProviderKind,
    pub provider: String,
    pub api_key: Option<String>,
    pub settings: Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    pub id: Uuid,
    pub account_id: Uuid,
    pub name: String,
    pub api_key: String,
    pub api_secret: String,
}

/// Stored deduplicated bug record.
///
/// The current model intentionally mixes several categories of data:
/// - canonical identity: `account_id + stacktrace_hash`
/// - canonicalized identity materialization: `normalized_stacktrace`
/// - first-occurrence snapshots: `agent_id`, `language`, `first_seen_at`
/// - latest or aggregate occurrence state: `severity`, `latest_stacktrace`, `last_seen_at`,
///   `occurrence_count`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bug {
    pub id: Uuid,
    pub account_id: Uuid,
    pub agent_id: Uuid,
    pub language: String,
    pub severity: Severity,
    pub stacktrace_hash: String,
    pub normalized_stacktrace: String,
    pub latest_stacktrace: String,
    pub first_seen_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    pub occurrence_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Occurrence {
    pub id: Uuid,
    pub bug_id: Uuid,
    pub severity: Severity,
    pub stacktrace: String,
    pub occurred_at: DateTime<Utc>,
    pub service: Option<String>,
    pub environment: Option<String>,
    pub attributes: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ticket {
    pub id: Uuid,
    pub bug_id: Uuid,
    pub provider: TicketProvider,
    pub remote_id: String,
    pub remote_url: String,
    pub priority: TicketPriority,
    pub recommendation: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationRecord {
    pub id: Uuid,
    pub bug_id: Uuid,
    pub provider: NotificationProvider,
    pub message: String,
    pub sent_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogRecord {
    pub id: Uuid,
    pub account_id: Uuid,
    pub agent_id: Uuid,
    pub language: String,
    pub level: Severity,
    pub message: String,
    pub stacktrace: Option<String>,
    pub occurred_at: DateTime<Utc>,
    pub service: Option<String>,
    pub environment: Option<String>,
    pub attributes: HashMap<String, String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogArchive {
    pub id: Uuid,
    pub cutoff_before: DateTime<Utc>,
    pub log_count: i64,
    pub summary: Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateAccountRequest {
    #[serde(default)]
    pub organization_id: Option<Uuid>,
    pub name: String,
    pub create_tickets: bool,
    pub ticket_provider: TicketProvider,
    #[serde(default)]
    pub ticketing_api_key: Option<String>,
    pub notification_provider: NotificationProvider,
    #[serde(default)]
    pub notification_api_key: Option<String>,
    #[serde(default = "default_true")]
    pub ai_enabled: bool,
    #[serde(default = "default_true")]
    pub use_managed_ai: bool,
    #[serde(default)]
    pub ai_api_key: Option<String>,
    pub notify_min_level: Severity,
    pub rapid_occurrence_window_minutes: i64,
    pub rapid_occurrence_threshold: i64,
}

impl CreateAccountRequest {
    pub fn validate(&self) -> AppResult<()> {
        if self.name.trim().is_empty() {
            return Err(AppError::Validation(
                "account name cannot be empty".to_string(),
            ));
        }
        if self.rapid_occurrence_window_minutes <= 0 {
            return Err(AppError::Validation(
                "rapid_occurrence_window_minutes must be greater than zero".to_string(),
            ));
        }
        if self.rapid_occurrence_threshold <= 0 {
            return Err(AppError::Validation(
                "rapid_occurrence_threshold must be greater than zero".to_string(),
            ));
        }
        Ok(())
    }
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateOrganizationRequest {
    pub name: String,
    pub owner_email: String,
    pub owner_name: String,
}

impl CreateOrganizationRequest {
    pub fn validate(&self) -> AppResult<()> {
        if self.name.trim().is_empty() {
            return Err(AppError::Validation(
                "organization name cannot be empty".to_string(),
            ));
        }
        if normalize_email(&self.owner_email).is_none() {
            return Err(AppError::Validation(
                "owner_email must be a valid email".to_string(),
            ));
        }
        if self.owner_name.trim().is_empty() {
            return Err(AppError::Validation(
                "owner_name cannot be empty".to_string(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddOrganizationMemberRequest {
    pub email: String,
    pub name: String,
    pub role: OrganizationRole,
}

impl AddOrganizationMemberRequest {
    pub fn validate(&self) -> AppResult<()> {
        if normalize_email(&self.email).is_none() {
            return Err(AppError::Validation(
                "email must be a valid email".to_string(),
            ));
        }
        if self.name.trim().is_empty() {
            return Err(AppError::Validation("name cannot be empty".to_string()));
        }
        if self.role == OrganizationRole::Owner {
            return Err(AppError::Validation(
                "members can only be added as admin or member".to_string(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateOrganizationMembershipRequest {
    pub role: OrganizationRole,
}

impl UpdateOrganizationMembershipRequest {
    pub fn validate(&self) -> AppResult<()> {
        if self.role == OrganizationRole::Owner {
            return Err(AppError::Validation(
                "owner role cannot be assigned through membership updates".to_string(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateAgentRequest {
    pub account_id: Uuid,
    pub name: String,
}

impl CreateAgentRequest {
    pub fn validate(&self) -> AppResult<()> {
        if self.name.trim().is_empty() {
            return Err(AppError::Validation(
                "agent name cannot be empty".to_string(),
            ));
        }
        Ok(())
    }
}

pub fn normalize_email(value: &str) -> Option<String> {
    let normalized = value.trim().to_ascii_lowercase();
    if normalized.is_empty() || !normalized.contains('@') {
        return None;
    }

    Some(normalized)
}

/// Canonical stacktrace-first event used by the intake service.
///
/// Field roles:
/// - transport and authentication: `agent_key`, `agent_secret`
/// - bug identity primitive: `stacktrace`
/// - occurrence metadata: `level`, `occurred_at`, `service`, `environment`, `attributes`
///
/// The deduplicated bug key is `account_id + sha256(normalize(stacktrace))`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StacktraceEvent {
    pub agent_key: String,
    pub agent_secret: Option<String>,
    pub language: String,
    pub stacktrace: String,
    #[serde(default)]
    pub level: Severity,
    pub occurred_at: Option<DateTime<Utc>>,
    pub service: Option<String>,
    pub environment: Option<String>,
    #[serde(default)]
    pub attributes: HashMap<String, String>,
}

impl StacktraceEvent {
    pub fn validate(&self) -> AppResult<()> {
        if self.agent_key.trim().is_empty() {
            return Err(AppError::Validation(
                "agent_key cannot be empty".to_string(),
            ));
        }
        if self.language.trim().is_empty() {
            return Err(AppError::Validation("language cannot be empty".to_string()));
        }
        if self.stacktrace.trim().is_empty() {
            return Err(AppError::Validation(
                "stacktrace cannot be empty".to_string(),
            ));
        }
        Ok(())
    }
}

/// Raw API payload for the generic stacktrace intake endpoint.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StacktraceEventPayload {
    pub agent_key: String,
    pub agent_secret: Option<String>,
    pub language: String,
    pub stacktrace: String,
    #[serde(default)]
    pub level: Severity,
    pub occurred_at: Option<DateTime<Utc>>,
    pub service: Option<String>,
    pub environment: Option<String>,
    #[serde(default)]
    pub attributes: HashMap<String, String>,
}

impl StacktraceEventPayload {
    pub fn into_stacktrace_event(self) -> StacktraceEvent {
        StacktraceEvent {
            agent_key: self.agent_key,
            agent_secret: self.agent_secret,
            language: self.language,
            stacktrace: self.stacktrace,
            level: self.level,
            occurred_at: self.occurred_at,
            service: self.service,
            environment: self.environment,
            attributes: self.attributes,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AuthenticatedStacktraceEventPayload {
    pub language: String,
    pub stacktrace: String,
    #[serde(default)]
    pub level: Severity,
    pub occurred_at: Option<DateTime<Utc>>,
    pub service: Option<String>,
    pub environment: Option<String>,
    #[serde(default)]
    pub attributes: HashMap<String, String>,
}

impl AuthenticatedStacktraceEventPayload {
    pub fn into_stacktrace_event(self, agent_key: String, agent_secret: String) -> StacktraceEvent {
        StacktraceEvent {
            agent_key,
            agent_secret: Some(agent_secret),
            language: self.language,
            stacktrace: self.stacktrace,
            level: self.level,
            occurred_at: self.occurred_at,
            service: self.service,
            environment: self.environment,
            attributes: self.attributes,
        }
    }
}

/// Raw log payload emitted by `go-bugfixes/logs` and compatible agents.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GoLogPayload {
    pub log: String,
    pub level: String,
    pub file: Option<String>,
    pub line: Option<String>,
    pub line_number: Option<i64>,
    pub log_fmt: Option<String>,
    pub stack: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LogEvent {
    pub agent_key: String,
    pub agent_secret: Option<String>,
    pub language: String,
    pub message: String,
    pub stacktrace: Option<String>,
    #[serde(default)]
    pub level: Severity,
    pub occurred_at: Option<DateTime<Utc>>,
    pub service: Option<String>,
    pub environment: Option<String>,
    #[serde(default)]
    pub attributes: HashMap<String, String>,
}

impl LogEvent {
    pub fn validate(&self) -> AppResult<()> {
        if self.agent_key.trim().is_empty() {
            return Err(AppError::Validation(
                "agent_key cannot be empty".to_string(),
            ));
        }
        if self.language.trim().is_empty() {
            return Err(AppError::Validation("language cannot be empty".to_string()));
        }
        if self.message.trim().is_empty() {
            return Err(AppError::Validation("message cannot be empty".to_string()));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LogEventPayload {
    pub language: String,
    pub message: String,
    pub stacktrace: Option<String>,
    #[serde(default)]
    pub level: Severity,
    pub occurred_at: Option<DateTime<Utc>>,
    pub service: Option<String>,
    pub environment: Option<String>,
    #[serde(default)]
    pub attributes: HashMap<String, String>,
}

impl LogEventPayload {
    pub fn into_log_event(self, agent_key: String, agent_secret: String) -> LogEvent {
        LogEvent {
            agent_key,
            agent_secret: Some(agent_secret),
            language: self.language,
            message: self.message,
            stacktrace: self.stacktrace,
            level: self.level,
            occurred_at: self.occurred_at,
            service: self.service,
            environment: self.environment,
            attributes: self.attributes,
        }
    }
}

/// Raw panic payload emitted by `go-bugfixes/middleware` and compatible agents.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GoBugPayload {
    pub bug: Value,
    pub raw: Value,
    pub bug_line: Option<String>,
    pub file: Option<String>,
    pub line: Option<String>,
    pub line_number: Option<i64>,
    pub level: String,
}

impl From<StacktraceEvent> for StacktraceEventPayload {
    fn from(value: StacktraceEvent) -> Self {
        Self {
            agent_key: value.agent_key,
            agent_secret: value.agent_secret,
            language: value.language,
            stacktrace: value.stacktrace,
            level: value.level,
            occurred_at: value.occurred_at,
            service: value.service,
            environment: value.environment,
            attributes: value.attributes,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TicketAction {
    Created,
    Escalated,
    Commented,
    Unchanged,
    Skipped,
}

impl fmt::Display for TicketAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::Created => "created",
            Self::Escalated => "escalated",
            Self::Commented => "commented",
            Self::Unchanged => "unchanged",
            Self::Skipped => "skipped",
        };
        write!(f, "{value}")
    }
}

impl FromStr for TicketAction {
    type Err = AppError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "created" => Ok(Self::Created),
            "escalated" => Ok(Self::Escalated),
            "commented" => Ok(Self::Commented),
            "unchanged" => Ok(Self::Unchanged),
            "skipped" => Ok(Self::Skipped),
            _ => Err(AppError::Validation(format!(
                "unsupported ticket action: {value}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TicketEvent {
    pub id: Uuid,
    pub ticket_id: Uuid,
    pub bug_id: Uuid,
    pub provider: TicketProvider,
    pub action: TicketAction,
    pub comment: Option<String>,
    pub previous_priority: Option<TicketPriority>,
    pub next_priority: Option<TicketPriority>,
    pub occurred_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationEventStatus {
    Sent,
    Skipped,
}

impl fmt::Display for NotificationEventStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::Sent => "sent",
            Self::Skipped => "skipped",
        };
        write!(f, "{value}")
    }
}

impl FromStr for NotificationEventStatus {
    type Err = AppError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "sent" => Ok(Self::Sent),
            "skipped" => Ok(Self::Skipped),
            _ => Err(AppError::Validation(format!(
                "unsupported notification event status: {value}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationEvent {
    pub id: Uuid,
    pub bug_id: Uuid,
    pub provider: NotificationProvider,
    pub status: NotificationEventStatus,
    pub reason: String,
    pub message: Option<String>,
    pub severity: Severity,
    pub ticket_action: TicketAction,
    pub occurred_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationOutcome {
    pub sent: bool,
    pub provider: Option<NotificationProvider>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogIntakeOutcome {
    pub log_id: Uuid,
    pub stored: bool,
    pub retention_days: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogRetentionOutcome {
    pub archived_batches: i64,
    pub archived_logs: i64,
    pub deleted_logs: i64,
}

/// Summary returned after ingesting an event into the bug and occurrence model.
///
/// `stacktrace_hash` is derived from the normalized event stacktrace, and
/// `occurrence_count` reflects the aggregate bug state after the event is stored.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntakeOutcome {
    pub bug_id: Uuid,
    pub stacktrace_hash: String,
    pub occurrence_count: i64,
    pub is_new_bug: bool,
    pub ticket_action: TicketAction,
    pub ticket: Option<Ticket>,
    pub ai_recommendation: Option<String>,
    pub notification: NotificationOutcome,
}
