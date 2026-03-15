use std::{collections::HashMap, str::FromStr};

use chrono::{DateTime, Utc};
use serde_json::json;
use sqlx::{FromRow, PgPool, Postgres, Transaction, postgres::PgPoolOptions};
use uuid::Uuid;

use crate::{
    AppError, AppResult,
    config::Config,
    domain::{
        Account, AccountProviderConfig, AccountProviderKind, Agent, Bug, CreateAccountRequest,
        CreateAgentRequest, NotificationEvent, NotificationEventStatus, NotificationProvider,
        NotificationRecord, Occurrence, Severity, Ticket, TicketAction, TicketEvent,
        TicketPriority, TicketProvider,
    },
    migrations,
};

#[cfg(test)]
mod tests;

#[derive(Clone)]
pub struct Repository {
    pool: PgPool,
}

pub struct CreateBugRecord<'a> {
    pub account_id: Uuid,
    pub agent_id: Uuid,
    pub language: &'a str,
    pub severity: Severity,
    pub stacktrace_hash: &'a str,
    pub normalized_stacktrace: &'a str,
    pub latest_stacktrace: &'a str,
    pub occurred_at: DateTime<Utc>,
}

pub struct CreateTicketRecord<'a> {
    pub bug_id: Uuid,
    pub provider: TicketProvider,
    pub remote_id: &'a str,
    pub remote_url: &'a str,
    pub priority: TicketPriority,
    pub recommendation: &'a str,
    pub status: &'a str,
    pub now: DateTime<Utc>,
}

pub struct RecordOccurrence<'a> {
    pub bug_id: Uuid,
    pub severity: Severity,
    pub stacktrace: &'a str,
    pub occurred_at: DateTime<Utc>,
    pub service: Option<&'a str>,
    pub environment: Option<&'a str>,
    pub attributes: &'a HashMap<String, String>,
}

impl Repository {
    pub async fn connect(config: &Config) -> AppResult<Self> {
        sqlx::any::install_default_drivers();
        migrations::run(&config.database_url).await?;

        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(&config.database_url)
            .await?;

        Ok(Self { pool })
    }

    pub async fn create_account(&self, request: CreateAccountRequest) -> AppResult<Account> {
        request.validate()?;
        let account = Account {
            id: Uuid::new_v4(),
            name: request.name,
            create_tickets: request.create_tickets,
            ticket_provider: request.ticket_provider,
            ticketing_api_key: normalize_optional(request.ticketing_api_key),
            notification_provider: request.notification_provider,
            notification_api_key: normalize_optional(request.notification_api_key),
            ai_enabled: request.ai_enabled,
            use_managed_ai: request.use_managed_ai,
            ai_api_key: normalize_optional(request.ai_api_key),
            notify_min_level: request.notify_min_level,
            rapid_occurrence_window_minutes: request.rapid_occurrence_window_minutes,
            rapid_occurrence_threshold: request.rapid_occurrence_threshold,
        };
        let now = Utc::now();
        let provider_configs = build_provider_configs(&account, now);
        let mut tx = self.pool.begin().await?;

        sqlx::query(
            "INSERT INTO accounts (id, name, create_tickets, ticket_provider, ticketing_api_key, notification_provider, notification_api_key, ai_enabled, use_managed_ai, ai_api_key, notify_min_level, rapid_occurrence_window_minutes, rapid_occurrence_threshold) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)",
        )
        .bind(account.id.to_string())
        .bind(account.name.clone())
        .bind(account.create_tickets as i32)
        .bind(account.ticket_provider.to_string())
        .bind(account.ticketing_api_key.clone())
        .bind(account.notification_provider.to_string())
        .bind(account.notification_api_key.clone())
        .bind(account.ai_enabled as i32)
        .bind(account.use_managed_ai as i32)
        .bind(account.ai_api_key.clone())
        .bind(account.notify_min_level.to_string())
        .bind(account.rapid_occurrence_window_minutes as i32)
        .bind(account.rapid_occurrence_threshold as i32)
        .execute(&mut *tx)
        .await?;

        for config in &provider_configs {
            insert_account_provider_config_tx(&mut tx, config).await?;
        }

        tx.commit().await?;
        Ok(account)
    }

    pub async fn create_agent(&self, request: CreateAgentRequest) -> AppResult<Agent> {
        request.validate()?;
        self.find_account(request.account_id).await?;

        let agent = Agent {
            id: Uuid::new_v4(),
            account_id: request.account_id,
            name: request.name,
            api_key: Uuid::new_v4().simple().to_string(),
            api_secret: Uuid::new_v4().simple().to_string(),
        };

        sqlx::query(
            "INSERT INTO agents (id, account_id, name, api_key, api_secret) VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(agent.id.to_string())
        .bind(agent.account_id.to_string())
        .bind(agent.name.clone())
        .bind(agent.api_key.clone())
        .bind(agent.api_secret.clone())
        .execute(&self.pool)
        .await?;

        Ok(agent)
    }

    pub async fn find_account(&self, account_id: Uuid) -> AppResult<Account> {
        let row = sqlx::query_as::<_, AccountRow>(
            "SELECT id, name, create_tickets, ticket_provider, ticketing_api_key, notification_provider, notification_api_key, ai_enabled, use_managed_ai, ai_api_key, notify_min_level, rapid_occurrence_window_minutes, rapid_occurrence_threshold FROM accounts WHERE id = $1",
        )
        .bind(account_id.to_string())
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("account {account_id}")))?;

        row.try_into()
    }

    pub async fn list_account_provider_configs(
        &self,
        account_id: Uuid,
    ) -> AppResult<Vec<AccountProviderConfig>> {
        sqlx::query_as::<_, AccountProviderConfigRow>(
            "SELECT id, account_id, kind, provider, api_key, settings_json, created_at, updated_at FROM account_provider_configs WHERE account_id = $1 ORDER BY kind",
        )
        .bind(account_id.to_string())
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(TryInto::try_into)
        .collect()
    }

    pub async fn find_agent_by_key(&self, api_key: &str) -> AppResult<Agent> {
        let row = sqlx::query_as::<_, AgentRow>(
            "SELECT id, account_id, name, api_key, api_secret FROM agents WHERE api_key = $1",
        )
        .bind(api_key)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| AppError::NotFound("agent for provided key".to_string()))?;

        row.try_into()
    }

    pub async fn find_agent_by_credentials(
        &self,
        api_key: &str,
        api_secret: &str,
    ) -> AppResult<Agent> {
        let row = sqlx::query_as::<_, AgentRow>(
            "SELECT id, account_id, name, api_key, api_secret FROM agents WHERE api_key = $1 AND api_secret = $2",
        )
        .bind(api_key)
        .bind(api_secret)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| AppError::NotFound("agent for provided credentials".to_string()))?;

        row.try_into()
    }

    pub async fn find_bug_by_hash(
        &self,
        account_id: Uuid,
        stacktrace_hash: &str,
    ) -> AppResult<Option<Bug>> {
        sqlx::query_as::<_, BugRow>(
            "SELECT id, account_id, agent_id, language, severity, stacktrace_hash, normalized_stacktrace, latest_stacktrace, first_seen_at, last_seen_at, occurrence_count FROM bugs WHERE account_id = $1 AND stacktrace_hash = $2",
        )
        .bind(account_id.to_string())
        .bind(stacktrace_hash)
        .fetch_optional(&self.pool)
        .await?
        .map(TryInto::try_into)
        .transpose()
    }

    pub async fn create_bug(&self, record: CreateBugRecord<'_>) -> AppResult<Bug> {
        let bug = Bug {
            id: Uuid::new_v4(),
            account_id: record.account_id,
            agent_id: record.agent_id,
            language: record.language.to_string(),
            severity: record.severity,
            stacktrace_hash: record.stacktrace_hash.to_string(),
            normalized_stacktrace: record.normalized_stacktrace.to_string(),
            latest_stacktrace: record.latest_stacktrace.to_string(),
            first_seen_at: record.occurred_at,
            last_seen_at: record.occurred_at,
            occurrence_count: 1,
        };

        sqlx::query(
            "INSERT INTO bugs (id, account_id, agent_id, language, severity, stacktrace_hash, normalized_stacktrace, latest_stacktrace, first_seen_at, last_seen_at, occurrence_count) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
        )
        .bind(bug.id.to_string())
        .bind(bug.account_id.to_string())
        .bind(bug.agent_id.to_string())
        .bind(bug.language.clone())
        .bind(bug.severity.to_string())
        .bind(bug.stacktrace_hash.clone())
        .bind(bug.normalized_stacktrace.clone())
        .bind(bug.latest_stacktrace.clone())
        .bind(bug.first_seen_at.to_rfc3339())
        .bind(bug.last_seen_at.to_rfc3339())
        .bind(bug.occurrence_count as i32)
        .execute(&self.pool)
        .await?;

        Ok(bug)
    }

    pub async fn record_occurrence(&self, record: RecordOccurrence<'_>) -> AppResult<Occurrence> {
        let occurrence = Occurrence {
            id: Uuid::new_v4(),
            bug_id: record.bug_id,
            severity: record.severity,
            stacktrace: record.stacktrace.to_string(),
            occurred_at: record.occurred_at,
            service: record.service.map(str::to_string),
            environment: record.environment.map(str::to_string),
            attributes: record.attributes.clone(),
        };

        sqlx::query(
            "INSERT INTO occurrences (id, bug_id, severity, stacktrace, occurred_at, service, environment, attributes_json) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(occurrence.id.to_string())
        .bind(occurrence.bug_id.to_string())
        .bind(occurrence.severity.to_string())
        .bind(occurrence.stacktrace.clone())
        .bind(occurrence.occurred_at.to_rfc3339())
        .bind(occurrence.service.clone())
        .bind(occurrence.environment.clone())
        .bind(serde_json::to_string(&occurrence.attributes)?)
        .execute(&self.pool)
        .await?;

        Ok(occurrence)
    }

    pub async fn list_occurrences_for_bug(&self, bug_id: Uuid) -> AppResult<Vec<Occurrence>> {
        sqlx::query_as::<_, OccurrenceRow>(
            "SELECT id, bug_id, severity, stacktrace, occurred_at, service, environment, attributes_json FROM occurrences WHERE bug_id = $1 ORDER BY occurred_at ASC",
        )
        .bind(bug_id.to_string())
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(TryInto::try_into)
        .collect()
    }

    pub async fn update_bug_on_repeat(
        &self,
        bug: &Bug,
        severity: Severity,
        latest_stacktrace: &str,
        occurred_at: DateTime<Utc>,
    ) -> AppResult<Bug> {
        let effective_severity = if severity.rank() > bug.severity.rank() {
            severity
        } else {
            bug.severity
        };
        let occurrence_count = bug.occurrence_count + 1;

        sqlx::query(
            "UPDATE bugs SET severity = $1, latest_stacktrace = $2, last_seen_at = $3, occurrence_count = $4 WHERE id = $5",
        )
        .bind(effective_severity.to_string())
        .bind(latest_stacktrace)
        .bind(occurred_at.to_rfc3339())
        .bind(occurrence_count as i32)
        .bind(bug.id.to_string())
        .execute(&self.pool)
        .await?;

        Ok(Bug {
            id: bug.id,
            account_id: bug.account_id,
            agent_id: bug.agent_id,
            language: bug.language.clone(),
            severity: effective_severity,
            stacktrace_hash: bug.stacktrace_hash.clone(),
            normalized_stacktrace: bug.normalized_stacktrace.clone(),
            latest_stacktrace: latest_stacktrace.to_string(),
            first_seen_at: bug.first_seen_at,
            last_seen_at: occurred_at,
            occurrence_count,
        })
    }

    pub async fn count_recent_occurrences(
        &self,
        bug_id: Uuid,
        since: DateTime<Utc>,
    ) -> AppResult<i64> {
        let row = sqlx::query_as::<_, CountRow>(
            "SELECT COUNT(*) AS count FROM occurrences WHERE bug_id = $1 AND occurred_at >= $2",
        )
        .bind(bug_id.to_string())
        .bind(since.to_rfc3339())
        .fetch_one(&self.pool)
        .await?;

        Ok(row.count)
    }

    pub async fn create_ticket(&self, record: CreateTicketRecord<'_>) -> AppResult<Ticket> {
        let ticket = Ticket {
            id: Uuid::new_v4(),
            bug_id: record.bug_id,
            provider: record.provider,
            remote_id: record.remote_id.to_string(),
            remote_url: record.remote_url.to_string(),
            priority: record.priority,
            recommendation: record.recommendation.to_string(),
            status: record.status.to_string(),
            created_at: record.now,
            updated_at: record.now,
        };
        let mut tx = self.pool.begin().await?;

        sqlx::query(
            "INSERT INTO tickets (id, bug_id, provider, remote_id, remote_url, priority, recommendation, status, created_at, updated_at) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
        )
        .bind(ticket.id.to_string())
        .bind(ticket.bug_id.to_string())
        .bind(ticket.provider.to_string())
        .bind(ticket.remote_id.clone())
        .bind(ticket.remote_url.clone())
        .bind(ticket.priority.to_string())
        .bind(ticket.recommendation.clone())
        .bind(ticket.status.clone())
        .bind(ticket.created_at.to_rfc3339())
        .bind(ticket.updated_at.to_rfc3339())
        .execute(&mut *tx)
        .await?;

        insert_ticket_event_tx(
            &mut tx,
            &TicketEvent {
                id: Uuid::new_v4(),
                ticket_id: ticket.id,
                bug_id: ticket.bug_id,
                provider: ticket.provider,
                action: TicketAction::Created,
                comment: None,
                previous_priority: None,
                next_priority: Some(ticket.priority),
                occurred_at: record.now,
            },
        )
        .await?;

        tx.commit().await?;
        Ok(ticket)
    }

    pub async fn find_ticket_for_bug(&self, bug_id: Uuid) -> AppResult<Option<Ticket>> {
        sqlx::query_as::<_, TicketRow>(
            "SELECT id, bug_id, provider, remote_id, remote_url, priority, recommendation, status, created_at, updated_at FROM tickets WHERE bug_id = $1",
        )
        .bind(bug_id.to_string())
        .fetch_optional(&self.pool)
        .await?
        .map(TryInto::try_into)
        .transpose()
    }

    pub async fn list_ticket_events_for_bug(&self, bug_id: Uuid) -> AppResult<Vec<TicketEvent>> {
        sqlx::query_as::<_, TicketEventRow>(
            "SELECT id, ticket_id, bug_id, provider, action, comment, previous_priority, next_priority, occurred_at FROM ticket_events WHERE bug_id = $1 ORDER BY occurred_at ASC",
        )
        .bind(bug_id.to_string())
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(TryInto::try_into)
        .collect()
    }

    pub async fn escalate_ticket(
        &self,
        ticket: &Ticket,
        priority: TicketPriority,
        comment: &str,
        now: DateTime<Utc>,
    ) -> AppResult<Ticket> {
        let mut tx = self.pool.begin().await?;

        sqlx::query("UPDATE tickets SET priority = $1, updated_at = $2 WHERE id = $3")
            .bind(priority.to_string())
            .bind(now.to_rfc3339())
            .bind(ticket.id.to_string())
            .execute(&mut *tx)
            .await?;
        insert_ticket_comment_tx(&mut tx, ticket.id, comment, now).await?;
        insert_ticket_event_tx(
            &mut tx,
            &TicketEvent {
                id: Uuid::new_v4(),
                ticket_id: ticket.id,
                bug_id: ticket.bug_id,
                provider: ticket.provider,
                action: TicketAction::Escalated,
                comment: Some(comment.to_string()),
                previous_priority: Some(ticket.priority),
                next_priority: Some(priority),
                occurred_at: now,
            },
        )
        .await?;

        tx.commit().await?;

        Ok(Ticket {
            priority,
            updated_at: now,
            ..ticket.clone()
        })
    }

    pub async fn comment_on_ticket(
        &self,
        ticket: &Ticket,
        comment: &str,
        now: DateTime<Utc>,
    ) -> AppResult<Ticket> {
        let mut tx = self.pool.begin().await?;

        sqlx::query("UPDATE tickets SET updated_at = $1 WHERE id = $2")
            .bind(now.to_rfc3339())
            .bind(ticket.id.to_string())
            .execute(&mut *tx)
            .await?;
        insert_ticket_comment_tx(&mut tx, ticket.id, comment, now).await?;
        insert_ticket_event_tx(
            &mut tx,
            &TicketEvent {
                id: Uuid::new_v4(),
                ticket_id: ticket.id,
                bug_id: ticket.bug_id,
                provider: ticket.provider,
                action: TicketAction::Commented,
                comment: Some(comment.to_string()),
                previous_priority: Some(ticket.priority),
                next_priority: Some(ticket.priority),
                occurred_at: now,
            },
        )
        .await?;

        tx.commit().await?;

        Ok(Ticket {
            updated_at: now,
            ..ticket.clone()
        })
    }

    pub async fn record_notification(
        &self,
        bug_id: Uuid,
        provider: NotificationProvider,
        message: &str,
        severity: Severity,
        ticket_action: TicketAction,
        now: DateTime<Utc>,
    ) -> AppResult<NotificationRecord> {
        let notification = NotificationRecord {
            id: Uuid::new_v4(),
            bug_id,
            provider,
            message: message.to_string(),
            sent_at: now,
        };
        let mut tx = self.pool.begin().await?;

        sqlx::query(
            "INSERT INTO notifications (id, bug_id, provider, message, sent_at) VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(notification.id.to_string())
        .bind(notification.bug_id.to_string())
        .bind(notification.provider.to_string())
        .bind(notification.message.clone())
        .bind(notification.sent_at.to_rfc3339())
        .execute(&mut *tx)
        .await?;
        insert_notification_event_tx(
            &mut tx,
            &NotificationEvent {
                id: Uuid::new_v4(),
                bug_id,
                provider,
                status: NotificationEventStatus::Sent,
                reason: "policy_allowed".to_string(),
                message: Some(message.to_string()),
                severity,
                ticket_action,
                occurred_at: now,
            },
        )
        .await?;

        tx.commit().await?;
        Ok(notification)
    }

    pub async fn record_notification_skip(
        &self,
        bug_id: Uuid,
        provider: NotificationProvider,
        severity: Severity,
        ticket_action: TicketAction,
        reason: &str,
        now: DateTime<Utc>,
    ) -> AppResult<NotificationEvent> {
        let event = NotificationEvent {
            id: Uuid::new_v4(),
            bug_id,
            provider,
            status: NotificationEventStatus::Skipped,
            reason: reason.to_string(),
            message: None,
            severity,
            ticket_action,
            occurred_at: now,
        };
        let mut tx = self.pool.begin().await?;
        insert_notification_event_tx(&mut tx, &event).await?;
        tx.commit().await?;
        Ok(event)
    }

    pub async fn list_notification_events_for_bug(
        &self,
        bug_id: Uuid,
    ) -> AppResult<Vec<NotificationEvent>> {
        sqlx::query_as::<_, NotificationEventRow>(
            "SELECT id, bug_id, provider, status, reason, message, severity, ticket_action, occurred_at FROM notification_events WHERE bug_id = $1 ORDER BY occurred_at ASC",
        )
        .bind(bug_id.to_string())
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(TryInto::try_into)
        .collect()
    }
}

fn normalize_optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn build_provider_configs(account: &Account, now: DateTime<Utc>) -> Vec<AccountProviderConfig> {
    let ai_provider = if account.use_managed_ai {
        "managed"
    } else {
        "customer_managed"
    };

    vec![
        AccountProviderConfig {
            id: Uuid::new_v4(),
            account_id: account.id,
            kind: AccountProviderKind::Ticketing,
            provider: account.ticket_provider.to_string(),
            api_key: account.ticketing_api_key.clone(),
            settings: json!({
                "enabled": account.create_tickets,
            }),
            created_at: now,
            updated_at: now,
        },
        AccountProviderConfig {
            id: Uuid::new_v4(),
            account_id: account.id,
            kind: AccountProviderKind::Notification,
            provider: account.notification_provider.to_string(),
            api_key: account.notification_api_key.clone(),
            settings: json!({
                "notify_min_level": account.notify_min_level.to_string(),
            }),
            created_at: now,
            updated_at: now,
        },
        AccountProviderConfig {
            id: Uuid::new_v4(),
            account_id: account.id,
            kind: AccountProviderKind::Ai,
            provider: ai_provider.to_string(),
            api_key: account.ai_api_key.clone(),
            settings: json!({
                "enabled": account.ai_enabled,
            }),
            created_at: now,
            updated_at: now,
        },
    ]
}

async fn insert_account_provider_config_tx(
    tx: &mut Transaction<'_, Postgres>,
    config: &AccountProviderConfig,
) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO account_provider_configs (id, account_id, kind, provider, api_key, settings_json, created_at, updated_at) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(config.id.to_string())
    .bind(config.account_id.to_string())
    .bind(config.kind.to_string())
    .bind(config.provider.clone())
    .bind(config.api_key.clone())
    .bind(serde_json::to_string(&config.settings)?)
    .bind(config.created_at.to_rfc3339())
    .bind(config.updated_at.to_rfc3339())
    .execute(&mut **tx)
    .await?;

    Ok(())
}

async fn insert_ticket_comment_tx(
    tx: &mut Transaction<'_, Postgres>,
    ticket_id: Uuid,
    comment: &str,
    now: DateTime<Utc>,
) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO ticket_comments (id, ticket_id, comment, created_at) VALUES ($1, $2, $3, $4)",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(ticket_id.to_string())
    .bind(comment)
    .bind(now.to_rfc3339())
    .execute(&mut **tx)
    .await?;

    Ok(())
}

async fn insert_ticket_event_tx(
    tx: &mut Transaction<'_, Postgres>,
    event: &TicketEvent,
) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO ticket_events (id, ticket_id, bug_id, provider, action, comment, previous_priority, next_priority, occurred_at) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
    )
    .bind(event.id.to_string())
    .bind(event.ticket_id.to_string())
    .bind(event.bug_id.to_string())
    .bind(event.provider.to_string())
    .bind(event.action.to_string())
    .bind(event.comment.clone())
    .bind(event.previous_priority.map(|value| value.to_string()))
    .bind(event.next_priority.map(|value| value.to_string()))
    .bind(event.occurred_at.to_rfc3339())
    .execute(&mut **tx)
    .await?;

    Ok(())
}

async fn insert_notification_event_tx(
    tx: &mut Transaction<'_, Postgres>,
    event: &NotificationEvent,
) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO notification_events (id, bug_id, provider, status, reason, message, severity, ticket_action, occurred_at) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
    )
    .bind(event.id.to_string())
    .bind(event.bug_id.to_string())
    .bind(event.provider.to_string())
    .bind(event.status.to_string())
    .bind(event.reason.clone())
    .bind(event.message.clone())
    .bind(event.severity.to_string())
    .bind(event.ticket_action.to_string())
    .bind(event.occurred_at.to_rfc3339())
    .execute(&mut **tx)
    .await?;

    Ok(())
}

fn parse_rfc3339_utc(value: &str) -> AppResult<DateTime<Utc>> {
    Ok(DateTime::parse_from_rfc3339(value)?.with_timezone(&Utc))
}

#[derive(Debug, FromRow)]
struct AccountRow {
    id: String,
    name: String,
    create_tickets: i32,
    ticket_provider: String,
    ticketing_api_key: Option<String>,
    notification_provider: String,
    notification_api_key: Option<String>,
    ai_enabled: i32,
    use_managed_ai: i32,
    ai_api_key: Option<String>,
    notify_min_level: String,
    rapid_occurrence_window_minutes: i32,
    rapid_occurrence_threshold: i32,
}

impl TryFrom<AccountRow> for Account {
    type Error = AppError;

    fn try_from(row: AccountRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: Uuid::parse_str(&row.id)?,
            name: row.name,
            create_tickets: row.create_tickets != 0,
            ticket_provider: TicketProvider::from_str(&row.ticket_provider)?,
            ticketing_api_key: normalize_optional(row.ticketing_api_key),
            notification_provider: NotificationProvider::from_str(&row.notification_provider)?,
            notification_api_key: normalize_optional(row.notification_api_key),
            ai_enabled: row.ai_enabled != 0,
            use_managed_ai: row.use_managed_ai != 0,
            ai_api_key: normalize_optional(row.ai_api_key),
            notify_min_level: Severity::from_str(&row.notify_min_level)?,
            rapid_occurrence_window_minutes: row.rapid_occurrence_window_minutes as i64,
            rapid_occurrence_threshold: row.rapid_occurrence_threshold as i64,
        })
    }
}

#[derive(Debug, FromRow)]
struct AccountProviderConfigRow {
    id: String,
    account_id: String,
    kind: String,
    provider: String,
    api_key: Option<String>,
    settings_json: String,
    created_at: String,
    updated_at: String,
}

impl TryFrom<AccountProviderConfigRow> for AccountProviderConfig {
    type Error = AppError;

    fn try_from(row: AccountProviderConfigRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: Uuid::parse_str(&row.id)?,
            account_id: Uuid::parse_str(&row.account_id)?,
            kind: AccountProviderKind::from_str(&row.kind)?,
            provider: row.provider,
            api_key: normalize_optional(row.api_key),
            settings: serde_json::from_str(&row.settings_json)?,
            created_at: parse_rfc3339_utc(&row.created_at)?,
            updated_at: parse_rfc3339_utc(&row.updated_at)?,
        })
    }
}

#[derive(Debug, FromRow)]
struct AgentRow {
    id: String,
    account_id: String,
    name: String,
    api_key: String,
    api_secret: String,
}

impl TryFrom<AgentRow> for Agent {
    type Error = AppError;

    fn try_from(row: AgentRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: Uuid::parse_str(&row.id)?,
            account_id: Uuid::parse_str(&row.account_id)?,
            name: row.name,
            api_key: row.api_key,
            api_secret: row.api_secret,
        })
    }
}

#[derive(Debug, FromRow)]
struct BugRow {
    id: String,
    account_id: String,
    agent_id: String,
    language: String,
    severity: String,
    stacktrace_hash: String,
    normalized_stacktrace: String,
    latest_stacktrace: String,
    first_seen_at: String,
    last_seen_at: String,
    occurrence_count: i32,
}

impl TryFrom<BugRow> for Bug {
    type Error = AppError;

    fn try_from(row: BugRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: Uuid::parse_str(&row.id)?,
            account_id: Uuid::parse_str(&row.account_id)?,
            agent_id: Uuid::parse_str(&row.agent_id)?,
            language: row.language,
            severity: Severity::from_str(&row.severity)?,
            stacktrace_hash: row.stacktrace_hash,
            normalized_stacktrace: row.normalized_stacktrace,
            latest_stacktrace: row.latest_stacktrace,
            first_seen_at: parse_rfc3339_utc(&row.first_seen_at)?,
            last_seen_at: parse_rfc3339_utc(&row.last_seen_at)?,
            occurrence_count: row.occurrence_count as i64,
        })
    }
}

#[derive(Debug, FromRow)]
struct OccurrenceRow {
    id: String,
    bug_id: String,
    severity: String,
    stacktrace: String,
    occurred_at: String,
    service: Option<String>,
    environment: Option<String>,
    attributes_json: String,
}

impl TryFrom<OccurrenceRow> for Occurrence {
    type Error = AppError;

    fn try_from(row: OccurrenceRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: Uuid::parse_str(&row.id)?,
            bug_id: Uuid::parse_str(&row.bug_id)?,
            severity: Severity::from_str(&row.severity)?,
            stacktrace: row.stacktrace,
            occurred_at: parse_rfc3339_utc(&row.occurred_at)?,
            service: row.service,
            environment: row.environment,
            attributes: serde_json::from_str(&row.attributes_json)?,
        })
    }
}

#[derive(Debug, FromRow)]
struct TicketRow {
    id: String,
    bug_id: String,
    provider: String,
    remote_id: String,
    remote_url: String,
    priority: String,
    recommendation: String,
    status: String,
    created_at: String,
    updated_at: String,
}

impl TryFrom<TicketRow> for Ticket {
    type Error = AppError;

    fn try_from(row: TicketRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: Uuid::parse_str(&row.id)?,
            bug_id: Uuid::parse_str(&row.bug_id)?,
            provider: TicketProvider::from_str(&row.provider)?,
            remote_id: row.remote_id,
            remote_url: row.remote_url,
            priority: TicketPriority::from_str(&row.priority)?,
            recommendation: row.recommendation,
            status: row.status,
            created_at: parse_rfc3339_utc(&row.created_at)?,
            updated_at: parse_rfc3339_utc(&row.updated_at)?,
        })
    }
}

#[derive(Debug, FromRow)]
struct TicketEventRow {
    id: String,
    ticket_id: String,
    bug_id: String,
    provider: String,
    action: String,
    comment: Option<String>,
    previous_priority: Option<String>,
    next_priority: Option<String>,
    occurred_at: String,
}

impl TryFrom<TicketEventRow> for TicketEvent {
    type Error = AppError;

    fn try_from(row: TicketEventRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: Uuid::parse_str(&row.id)?,
            ticket_id: Uuid::parse_str(&row.ticket_id)?,
            bug_id: Uuid::parse_str(&row.bug_id)?,
            provider: TicketProvider::from_str(&row.provider)?,
            action: TicketAction::from_str(&row.action)?,
            comment: row.comment,
            previous_priority: row
                .previous_priority
                .as_deref()
                .map(TicketPriority::from_str)
                .transpose()?,
            next_priority: row
                .next_priority
                .as_deref()
                .map(TicketPriority::from_str)
                .transpose()?,
            occurred_at: parse_rfc3339_utc(&row.occurred_at)?,
        })
    }
}

#[derive(Debug, FromRow)]
struct NotificationEventRow {
    id: String,
    bug_id: String,
    provider: String,
    status: String,
    reason: String,
    message: Option<String>,
    severity: String,
    ticket_action: String,
    occurred_at: String,
}

impl TryFrom<NotificationEventRow> for NotificationEvent {
    type Error = AppError;

    fn try_from(row: NotificationEventRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: Uuid::parse_str(&row.id)?,
            bug_id: Uuid::parse_str(&row.bug_id)?,
            provider: NotificationProvider::from_str(&row.provider)?,
            status: NotificationEventStatus::from_str(&row.status)?,
            reason: row.reason,
            message: row.message,
            severity: Severity::from_str(&row.severity)?,
            ticket_action: TicketAction::from_str(&row.ticket_action)?,
            occurred_at: parse_rfc3339_utc(&row.occurred_at)?,
        })
    }
}

#[derive(Debug, FromRow)]
struct CountRow {
    count: i64,
}
