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

    pub fn should_notify(self, min: Self) -> bool {
        self.rank() >= min.rank()
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
    Linear,
    Tracklines,
}

impl fmt::Display for TicketProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::Jira => "jira",
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
pub struct Account {
    pub id: Uuid,
    pub name: String,
    pub create_tickets: bool,
    pub ticket_provider: TicketProvider,
    pub notification_provider: NotificationProvider,
    pub notify_min_level: Severity,
    pub rapid_occurrence_window_minutes: i64,
    pub rapid_occurrence_threshold: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    pub id: Uuid,
    pub account_id: Uuid,
    pub name: String,
    pub api_key: String,
    pub api_secret: String,
}

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
pub struct CreateAccountRequest {
    pub name: String,
    pub create_tickets: bool,
    pub ticket_provider: TicketProvider,
    pub notification_provider: NotificationProvider,
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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StacktraceEventRequest {
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

impl StacktraceEventRequest {
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
pub struct GoBugPayload {
    pub bug: Value,
    pub raw: Value,
    pub bug_line: Option<String>,
    pub file: Option<String>,
    pub line: Option<String>,
    pub line_number: Option<i64>,
    pub level: String,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationOutcome {
    pub sent: bool,
    pub provider: Option<NotificationProvider>,
    pub message: Option<String>,
}

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
