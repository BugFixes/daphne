use std::str::FromStr;

use chrono::{DateTime, Utc};
use sqlx::{
    FromRow, SqlitePool,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions},
};
use uuid::Uuid;

use crate::{
    AppError, AppResult,
    config::Config,
    domain::{
        Account, Agent, Bug, CreateAccountRequest, CreateAgentRequest, NotificationProvider,
        NotificationRecord, Severity, Ticket, TicketPriority, TicketProvider,
    },
};

#[derive(Clone)]
pub struct Repository {
    pool: SqlitePool,
}

impl Repository {
    pub async fn connect(config: &Config) -> AppResult<Self> {
        let mut options = SqliteConnectOptions::from_str(&config.database_url)
            .map_err(|error| AppError::Internal(format!("invalid database url: {error}")))?;

        options = options
            .create_if_missing(true)
            .foreign_keys(true)
            .journal_mode(SqliteJournalMode::Wal);

        let max_connections = if config.database_url.contains(":memory:") {
            1
        } else {
            5
        };
        let pool = SqlitePoolOptions::new()
            .max_connections(max_connections)
            .connect_with(options)
            .await?;

        let repository = Self { pool };
        repository.init_schema().await?;
        Ok(repository)
    }

    async fn init_schema(&self) -> AppResult<()> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS accounts (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                create_tickets INTEGER NOT NULL,
                ticket_provider TEXT NOT NULL,
                notification_provider TEXT NOT NULL,
                notify_min_level TEXT NOT NULL,
                rapid_occurrence_window_minutes INTEGER NOT NULL,
                rapid_occurrence_threshold INTEGER NOT NULL
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS agents (
                id TEXT PRIMARY KEY,
                account_id TEXT NOT NULL,
                name TEXT NOT NULL,
                api_key TEXT NOT NULL UNIQUE,
                api_secret TEXT NOT NULL,
                FOREIGN KEY(account_id) REFERENCES accounts(id) ON DELETE CASCADE
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS bugs (
                id TEXT PRIMARY KEY,
                account_id TEXT NOT NULL,
                agent_id TEXT NOT NULL,
                language TEXT NOT NULL,
                severity TEXT NOT NULL,
                stacktrace_hash TEXT NOT NULL,
                normalized_stacktrace TEXT NOT NULL,
                latest_stacktrace TEXT NOT NULL,
                first_seen_at TEXT NOT NULL,
                last_seen_at TEXT NOT NULL,
                occurrence_count INTEGER NOT NULL,
                UNIQUE(account_id, stacktrace_hash),
                FOREIGN KEY(account_id) REFERENCES accounts(id) ON DELETE CASCADE,
                FOREIGN KEY(agent_id) REFERENCES agents(id) ON DELETE CASCADE
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS occurrences (
                id TEXT PRIMARY KEY,
                bug_id TEXT NOT NULL,
                severity TEXT NOT NULL,
                stacktrace TEXT NOT NULL,
                occurred_at TEXT NOT NULL,
                FOREIGN KEY(bug_id) REFERENCES bugs(id) ON DELETE CASCADE
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS tickets (
                id TEXT PRIMARY KEY,
                bug_id TEXT NOT NULL UNIQUE,
                provider TEXT NOT NULL,
                remote_id TEXT NOT NULL,
                remote_url TEXT NOT NULL,
                priority TEXT NOT NULL,
                recommendation TEXT NOT NULL,
                status TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                FOREIGN KEY(bug_id) REFERENCES bugs(id) ON DELETE CASCADE
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS ticket_comments (
                id TEXT PRIMARY KEY,
                ticket_id TEXT NOT NULL,
                comment TEXT NOT NULL,
                created_at TEXT NOT NULL,
                FOREIGN KEY(ticket_id) REFERENCES tickets(id) ON DELETE CASCADE
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS notifications (
                id TEXT PRIMARY KEY,
                bug_id TEXT NOT NULL,
                provider TEXT NOT NULL,
                message TEXT NOT NULL,
                sent_at TEXT NOT NULL,
                FOREIGN KEY(bug_id) REFERENCES bugs(id) ON DELETE CASCADE
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

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

        sqlx::query(
            r#"
            INSERT INTO accounts (
                id, name, create_tickets, ticket_provider, notification_provider,
                notify_min_level, rapid_occurrence_window_minutes, rapid_occurrence_threshold
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            "#,
        )
        .bind(account.id.to_string())
        .bind(&account.name)
        .bind(account.create_tickets as i64)
        .bind(account.ticket_provider.to_string())
        .bind(account.notification_provider.to_string())
        .bind(account.notify_min_level.to_string())
        .bind(account.rapid_occurrence_window_minutes)
        .bind(account.rapid_occurrence_threshold)
        .execute(&self.pool)
        .await?;

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
            "INSERT INTO agents (id, account_id, name, api_key, api_secret) VALUES (?1, ?2, ?3, ?4, ?5)",
        )
            .bind(agent.id.to_string())
            .bind(agent.account_id.to_string())
            .bind(&agent.name)
            .bind(&agent.api_key)
            .bind(&agent.api_secret)
            .execute(&self.pool)
            .await?;

        Ok(agent)
    }

    pub async fn find_account(&self, account_id: Uuid) -> AppResult<Account> {
        let row = sqlx::query_as::<_, AccountRow>("SELECT * FROM accounts WHERE id = ?1")
            .bind(account_id.to_string())
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("account {account_id}")))?;

        row.try_into()
    }

    pub async fn find_agent_by_key(&self, api_key: &str) -> AppResult<Agent> {
        let row = sqlx::query_as::<_, AgentRow>("SELECT * FROM agents WHERE api_key = ?1")
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
            "SELECT * FROM agents WHERE api_key = ?1 AND api_secret = ?2",
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
        let row = sqlx::query_as::<_, BugRow>(
            "SELECT * FROM bugs WHERE account_id = ?1 AND stacktrace_hash = ?2",
        )
        .bind(account_id.to_string())
        .bind(stacktrace_hash)
        .fetch_optional(&self.pool)
        .await?;

        row.map(TryInto::try_into).transpose()
    }

    pub async fn create_bug(
        &self,
        account_id: Uuid,
        agent_id: Uuid,
        language: &str,
        severity: Severity,
        stacktrace_hash: &str,
        normalized_stacktrace: &str,
        latest_stacktrace: &str,
        occurred_at: DateTime<Utc>,
    ) -> AppResult<Bug> {
        let bug = Bug {
            id: Uuid::new_v4(),
            account_id,
            agent_id,
            language: language.to_string(),
            severity,
            stacktrace_hash: stacktrace_hash.to_string(),
            normalized_stacktrace: normalized_stacktrace.to_string(),
            latest_stacktrace: latest_stacktrace.to_string(),
            first_seen_at: occurred_at,
            last_seen_at: occurred_at,
            occurrence_count: 1,
        };

        sqlx::query(
            r#"
            INSERT INTO bugs (
                id, account_id, agent_id, language, severity, stacktrace_hash, normalized_stacktrace,
                latest_stacktrace, first_seen_at, last_seen_at, occurrence_count
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
            "#,
        )
        .bind(bug.id.to_string())
        .bind(bug.account_id.to_string())
        .bind(bug.agent_id.to_string())
        .bind(&bug.language)
        .bind(bug.severity.to_string())
        .bind(&bug.stacktrace_hash)
        .bind(&bug.normalized_stacktrace)
        .bind(&bug.latest_stacktrace)
        .bind(bug.first_seen_at.to_rfc3339())
        .bind(bug.last_seen_at.to_rfc3339())
        .bind(bug.occurrence_count)
        .execute(&self.pool)
        .await?;

        Ok(bug)
    }

    pub async fn record_occurrence(
        &self,
        bug_id: Uuid,
        severity: Severity,
        stacktrace: &str,
        occurred_at: DateTime<Utc>,
    ) -> AppResult<()> {
        sqlx::query("INSERT INTO occurrences (id, bug_id, severity, stacktrace, occurred_at) VALUES (?1, ?2, ?3, ?4, ?5)")
            .bind(Uuid::new_v4().to_string())
            .bind(bug_id.to_string())
            .bind(severity.to_string())
            .bind(stacktrace)
            .bind(occurred_at.to_rfc3339())
            .execute(&self.pool)
            .await?;

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

        sqlx::query(
            "UPDATE bugs SET severity = ?1, latest_stacktrace = ?2, last_seen_at = ?3, occurrence_count = ?4 WHERE id = ?5",
        )
        .bind(effective_severity.to_string())
        .bind(latest_stacktrace)
        .bind(occurred_at.to_rfc3339())
        .bind(occurrence_count)
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
        let count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM occurrences WHERE bug_id = ?1 AND occurred_at >= ?2",
        )
        .bind(bug_id.to_string())
        .bind(since.to_rfc3339())
        .fetch_one(&self.pool)
        .await?;

        Ok(count)
    }

    pub async fn create_ticket(
        &self,
        bug_id: Uuid,
        provider: TicketProvider,
        remote_id: &str,
        remote_url: &str,
        priority: TicketPriority,
        recommendation: &str,
        status: &str,
        now: DateTime<Utc>,
    ) -> AppResult<Ticket> {
        let ticket = Ticket {
            id: Uuid::new_v4(),
            bug_id,
            provider,
            remote_id: remote_id.to_string(),
            remote_url: remote_url.to_string(),
            priority,
            recommendation: recommendation.to_string(),
            status: status.to_string(),
            created_at: now,
            updated_at: now,
        };

        sqlx::query(
            r#"
            INSERT INTO tickets (id, bug_id, provider, remote_id, remote_url, priority, recommendation, status, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            "#,
        )
        .bind(ticket.id.to_string())
        .bind(ticket.bug_id.to_string())
        .bind(ticket.provider.to_string())
        .bind(&ticket.remote_id)
        .bind(&ticket.remote_url)
        .bind(ticket.priority.to_string())
        .bind(&ticket.recommendation)
        .bind(&ticket.status)
        .bind(ticket.created_at.to_rfc3339())
        .bind(ticket.updated_at.to_rfc3339())
        .execute(&self.pool)
        .await?;

        Ok(ticket)
    }

    pub async fn find_ticket_for_bug(&self, bug_id: Uuid) -> AppResult<Option<Ticket>> {
        let row = sqlx::query_as::<_, TicketRow>("SELECT * FROM tickets WHERE bug_id = ?1")
            .bind(bug_id.to_string())
            .fetch_optional(&self.pool)
            .await?;

        row.map(TryInto::try_into).transpose()
    }

    pub async fn update_ticket_priority(
        &self,
        ticket_id: Uuid,
        priority: TicketPriority,
        now: DateTime<Utc>,
    ) -> AppResult<()> {
        sqlx::query("UPDATE tickets SET priority = ?1, updated_at = ?2 WHERE id = ?3")
            .bind(priority.to_string())
            .bind(now.to_rfc3339())
            .bind(ticket_id.to_string())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn add_ticket_comment(
        &self,
        ticket_id: Uuid,
        comment: &str,
        now: DateTime<Utc>,
    ) -> AppResult<()> {
        sqlx::query("INSERT INTO ticket_comments (id, ticket_id, comment, created_at) VALUES (?1, ?2, ?3, ?4)")
            .bind(Uuid::new_v4().to_string())
            .bind(ticket_id.to_string())
            .bind(comment)
            .bind(now.to_rfc3339())
            .execute(&self.pool)
            .await?;
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

        sqlx::query("INSERT INTO notifications (id, bug_id, provider, message, sent_at) VALUES (?1, ?2, ?3, ?4, ?5)")
            .bind(notification.id.to_string())
            .bind(notification.bug_id.to_string())
            .bind(notification.provider.to_string())
            .bind(&notification.message)
            .bind(notification.sent_at.to_rfc3339())
            .execute(&self.pool)
            .await?;

        Ok(notification)
    }
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
