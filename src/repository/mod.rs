use std::{fs, path::PathBuf, str::FromStr};

use chrono::{DateTime, Utc};
use sqlx::{Any, AnyPool, FromRow, QueryBuilder, any::AnyPoolOptions};
use uuid::Uuid;

use crate::{
    AppError, AppResult,
    config::Config,
    domain::{
        Account, Agent, Bug, CreateAccountRequest, CreateAgentRequest, NotificationProvider,
        NotificationRecord, Severity, Ticket, TicketPriority, TicketProvider,
    },
};

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!();

#[derive(Clone)]
pub struct Repository {
    pool: AnyPool,
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

impl Repository {
    pub async fn connect(config: &Config) -> AppResult<Self> {
        sqlx::any::install_default_drivers();
        ensure_sqlite_database_exists(&config.database_url)?;

        let max_connections = if config.database_url.contains(":memory:") {
            1
        } else {
            5
        };

        let pool = AnyPoolOptions::new()
            .max_connections(max_connections)
            .connect(&config.database_url)
            .await?;

        if is_sqlite_url(&config.database_url) {
            sqlx::query("PRAGMA foreign_keys = ON")
                .execute(&pool)
                .await?;
            sqlx::query("PRAGMA journal_mode = WAL")
                .execute(&pool)
                .await?;
        }

        let repository = Self { pool };
        repository.run_migrations().await?;
        Ok(repository)
    }

    async fn run_migrations(&self) -> AppResult<()> {
        MIGRATOR.run(&self.pool).await?;
        Ok(())
    }

    pub async fn create_account(&self, request: CreateAccountRequest) -> AppResult<Account> {
        request.validate()?;
        let account = Account {
            id: Uuid::new_v4(),
            name: request.name,
            create_tickets: request.create_tickets,
            ticket_provider: request.ticket_provider,
            notification_provider: request.notification_provider,
            notify_min_level: request.notify_min_level,
            rapid_occurrence_window_minutes: request.rapid_occurrence_window_minutes,
            rapid_occurrence_threshold: request.rapid_occurrence_threshold,
        };

        let mut query = QueryBuilder::<Any>::new(
            "INSERT INTO accounts (id, name, create_tickets, ticket_provider, notification_provider, notify_min_level, rapid_occurrence_window_minutes, rapid_occurrence_threshold) VALUES (",
        );
        {
            let mut separated = query.separated(", ");
            separated.push_bind(account.id.to_string());
            separated.push_bind(account.name.clone());
            separated.push_bind(account.create_tickets as i64);
            separated.push_bind(account.ticket_provider.to_string());
            separated.push_bind(account.notification_provider.to_string());
            separated.push_bind(account.notify_min_level.to_string());
            separated.push_bind(account.rapid_occurrence_window_minutes);
            separated.push_bind(account.rapid_occurrence_threshold);
            separated.push_unseparated(")");
        }
        query.build().execute(&self.pool).await?;

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

        let mut query = QueryBuilder::<Any>::new(
            "INSERT INTO agents (id, account_id, name, api_key, api_secret) VALUES (",
        );
        {
            let mut separated = query.separated(", ");
            separated.push_bind(agent.id.to_string());
            separated.push_bind(agent.account_id.to_string());
            separated.push_bind(agent.name.clone());
            separated.push_bind(agent.api_key.clone());
            separated.push_bind(agent.api_secret.clone());
            separated.push_unseparated(")");
        }
        query.build().execute(&self.pool).await?;

        Ok(agent)
    }

    pub async fn find_account(&self, account_id: Uuid) -> AppResult<Account> {
        let mut query = QueryBuilder::<Any>::new(
            "SELECT id, name, create_tickets, ticket_provider, notification_provider, notify_min_level, rapid_occurrence_window_minutes, rapid_occurrence_threshold FROM accounts WHERE id = ",
        );
        query.push_bind(account_id.to_string());
        let row = query
            .build_query_as::<AccountRow>()
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("account {account_id}")))?;

        row.try_into()
    }

    pub async fn find_agent_by_key(&self, api_key: &str) -> AppResult<Agent> {
        let mut query = QueryBuilder::<Any>::new(
            "SELECT id, account_id, name, api_key, api_secret FROM agents WHERE api_key = ",
        );
        query.push_bind(api_key);
        let row = query
            .build_query_as::<AgentRow>()
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
        let mut query = QueryBuilder::<Any>::new(
            "SELECT id, account_id, name, api_key, api_secret FROM agents WHERE api_key = ",
        );
        query.push_bind(api_key);
        query.push(" AND api_secret = ");
        query.push_bind(api_secret);
        let row = query
            .build_query_as::<AgentRow>()
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
        let mut query = QueryBuilder::<Any>::new(
            "SELECT id, account_id, agent_id, language, severity, stacktrace_hash, normalized_stacktrace, latest_stacktrace, first_seen_at, last_seen_at, occurrence_count FROM bugs WHERE account_id = ",
        );
        query.push_bind(account_id.to_string());
        query.push(" AND stacktrace_hash = ");
        query.push_bind(stacktrace_hash);

        query
            .build_query_as::<BugRow>()
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

        let mut query = QueryBuilder::<Any>::new(
            "INSERT INTO bugs (id, account_id, agent_id, language, severity, stacktrace_hash, normalized_stacktrace, latest_stacktrace, first_seen_at, last_seen_at, occurrence_count) VALUES (",
        );
        {
            let mut separated = query.separated(", ");
            separated.push_bind(bug.id.to_string());
            separated.push_bind(bug.account_id.to_string());
            separated.push_bind(bug.agent_id.to_string());
            separated.push_bind(bug.language.clone());
            separated.push_bind(bug.severity.to_string());
            separated.push_bind(bug.stacktrace_hash.clone());
            separated.push_bind(bug.normalized_stacktrace.clone());
            separated.push_bind(bug.latest_stacktrace.clone());
            separated.push_bind(bug.first_seen_at.to_rfc3339());
            separated.push_bind(bug.last_seen_at.to_rfc3339());
            separated.push_bind(bug.occurrence_count);
            separated.push_unseparated(")");
        }
        query.build().execute(&self.pool).await?;

        Ok(bug)
    }

    pub async fn record_occurrence(
        &self,
        bug_id: Uuid,
        severity: Severity,
        stacktrace: &str,
        occurred_at: DateTime<Utc>,
    ) -> AppResult<()> {
        let mut query = QueryBuilder::<Any>::new(
            "INSERT INTO occurrences (id, bug_id, severity, stacktrace, occurred_at) VALUES (",
        );
        {
            let mut separated = query.separated(", ");
            separated.push_bind(Uuid::new_v4().to_string());
            separated.push_bind(bug_id.to_string());
            separated.push_bind(severity.to_string());
            separated.push_bind(stacktrace);
            separated.push_bind(occurred_at.to_rfc3339());
            separated.push_unseparated(")");
        }
        query.build().execute(&self.pool).await?;

        Ok(())
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

        let mut query = QueryBuilder::<Any>::new("UPDATE bugs SET severity = ");
        query.push_bind(effective_severity.to_string());
        query.push(", latest_stacktrace = ");
        query.push_bind(latest_stacktrace);
        query.push(", last_seen_at = ");
        query.push_bind(occurred_at.to_rfc3339());
        query.push(", occurrence_count = ");
        query.push_bind(occurrence_count);
        query.push(" WHERE id = ");
        query.push_bind(bug.id.to_string());
        query.build().execute(&self.pool).await?;

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
        let mut query =
            QueryBuilder::<Any>::new("SELECT COUNT(*) AS count FROM occurrences WHERE bug_id = ");
        query.push_bind(bug_id.to_string());
        query.push(" AND occurred_at >= ");
        query.push_bind(since.to_rfc3339());
        let row = query
            .build_query_as::<CountRow>()
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

        let mut query = QueryBuilder::<Any>::new(
            "INSERT INTO tickets (id, bug_id, provider, remote_id, remote_url, priority, recommendation, status, created_at, updated_at) VALUES (",
        );
        {
            let mut separated = query.separated(", ");
            separated.push_bind(ticket.id.to_string());
            separated.push_bind(ticket.bug_id.to_string());
            separated.push_bind(ticket.provider.to_string());
            separated.push_bind(ticket.remote_id.clone());
            separated.push_bind(ticket.remote_url.clone());
            separated.push_bind(ticket.priority.to_string());
            separated.push_bind(ticket.recommendation.clone());
            separated.push_bind(ticket.status.clone());
            separated.push_bind(ticket.created_at.to_rfc3339());
            separated.push_bind(ticket.updated_at.to_rfc3339());
            separated.push_unseparated(")");
        }
        query.build().execute(&self.pool).await?;

        Ok(ticket)
    }

    pub async fn find_ticket_for_bug(&self, bug_id: Uuid) -> AppResult<Option<Ticket>> {
        let mut query = QueryBuilder::<Any>::new(
            "SELECT id, bug_id, provider, remote_id, remote_url, priority, recommendation, status, created_at, updated_at FROM tickets WHERE bug_id = ",
        );
        query.push_bind(bug_id.to_string());

        query
            .build_query_as::<TicketRow>()
            .fetch_optional(&self.pool)
            .await?
            .map(TryInto::try_into)
            .transpose()
    }

    pub async fn update_ticket_priority(
        &self,
        ticket_id: Uuid,
        priority: TicketPriority,
        now: DateTime<Utc>,
    ) -> AppResult<()> {
        let mut query = QueryBuilder::<Any>::new("UPDATE tickets SET priority = ");
        query.push_bind(priority.to_string());
        query.push(", updated_at = ");
        query.push_bind(now.to_rfc3339());
        query.push(" WHERE id = ");
        query.push_bind(ticket_id.to_string());
        query.build().execute(&self.pool).await?;
        Ok(())
    }

    pub async fn add_ticket_comment(
        &self,
        ticket_id: Uuid,
        comment: &str,
        now: DateTime<Utc>,
    ) -> AppResult<()> {
        let mut query = QueryBuilder::<Any>::new(
            "INSERT INTO ticket_comments (id, ticket_id, comment, created_at) VALUES (",
        );
        {
            let mut separated = query.separated(", ");
            separated.push_bind(Uuid::new_v4().to_string());
            separated.push_bind(ticket_id.to_string());
            separated.push_bind(comment);
            separated.push_bind(now.to_rfc3339());
            separated.push_unseparated(")");
        }
        query.build().execute(&self.pool).await?;
        Ok(())
    }

    pub async fn record_notification(
        &self,
        bug_id: Uuid,
        provider: NotificationProvider,
        message: &str,
        now: DateTime<Utc>,
    ) -> AppResult<NotificationRecord> {
        let notification = NotificationRecord {
            id: Uuid::new_v4(),
            bug_id,
            provider,
            message: message.to_string(),
            sent_at: now,
        };

        let mut query = QueryBuilder::<Any>::new(
            "INSERT INTO notifications (id, bug_id, provider, message, sent_at) VALUES (",
        );
        {
            let mut separated = query.separated(", ");
            separated.push_bind(notification.id.to_string());
            separated.push_bind(notification.bug_id.to_string());
            separated.push_bind(notification.provider.to_string());
            separated.push_bind(notification.message.clone());
            separated.push_bind(notification.sent_at.to_rfc3339());
            separated.push_unseparated(")");
        }
        query.build().execute(&self.pool).await?;

        Ok(notification)
    }
}

fn is_sqlite_url(database_url: &str) -> bool {
    database_url.starts_with("sqlite:")
}

fn ensure_sqlite_database_exists(database_url: &str) -> AppResult<()> {
    if !is_sqlite_url(database_url) {
        return Ok(());
    }

    let raw_path = database_url
        .trim_start_matches("sqlite://")
        .trim_start_matches("sqlite:")
        .split('?')
        .next()
        .unwrap_or_default();

    if raw_path.is_empty() || raw_path == ":memory:" {
        return Ok(());
    }

    let path = PathBuf::from(raw_path);
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    if !path.exists() {
        fs::File::create(path)?;
    }

    Ok(())
}

#[derive(Debug, FromRow)]
struct AccountRow {
    id: String,
    name: String,
    create_tickets: i64,
    ticket_provider: String,
    notification_provider: String,
    notify_min_level: String,
    rapid_occurrence_window_minutes: i64,
    rapid_occurrence_threshold: i64,
}

impl TryFrom<AccountRow> for Account {
    type Error = AppError;

    fn try_from(row: AccountRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: Uuid::parse_str(&row.id)?,
            name: row.name,
            create_tickets: row.create_tickets != 0,
            ticket_provider: TicketProvider::from_str(&row.ticket_provider)?,
            notification_provider: NotificationProvider::from_str(&row.notification_provider)?,
            notify_min_level: Severity::from_str(&row.notify_min_level)?,
            rapid_occurrence_window_minutes: row.rapid_occurrence_window_minutes,
            rapid_occurrence_threshold: row.rapid_occurrence_threshold,
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
    occurrence_count: i64,
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
            first_seen_at: DateTime::parse_from_rfc3339(&row.first_seen_at)?.with_timezone(&Utc),
            last_seen_at: DateTime::parse_from_rfc3339(&row.last_seen_at)?.with_timezone(&Utc),
            occurrence_count: row.occurrence_count,
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
            created_at: DateTime::parse_from_rfc3339(&row.created_at)?.with_timezone(&Utc),
            updated_at: DateTime::parse_from_rfc3339(&row.updated_at)?.with_timezone(&Utc),
        })
    }
}

#[derive(Debug, FromRow)]
struct CountRow {
    count: i64,
}
