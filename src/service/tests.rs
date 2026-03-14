use std::{collections::HashSet, sync::Arc};

use chrono::Utc;
use serial_test::serial;
use sqlx::{AnyPool, any::AnyPoolOptions};

use crate::{
    ai::AiRegistry,
    config::Config,
    domain::{
        CreateAccountRequest, CreateAgentRequest, NotificationProvider, Severity, StacktraceEvent,
        TicketAction, TicketPriority, TicketProvider,
    },
    feature_flags::build_feature_flags,
    notifications::NotificationRegistry,
    policy::build_policy_engine,
    repository::{CreateBugRecord, Repository},
    ticketing::TicketingRegistry,
};

use super::IntakeService;

async fn test_service() -> IntakeService {
    test_service_with_disabled_features(HashSet::new()).await
}

fn test_config(disabled_features: HashSet<String>) -> Config {
    let _ = dotenvy::dotenv();
    let mut config = Config::from_env().expect("config");
    config.bind_address = "127.0.0.1:0".to_string();
    config.feature_flags_provider = "local".to_string();
    config.policy_provider = "local".to_string();
    config.policy2_engine_url = "https://api.policy2.net/run".to_string();
    config.flagsgg_project_id = None;
    config.flagsgg_agent_id = None;
    config.flagsgg_environment_id = None;
    config.disabled_features = disabled_features;
    config
}

async fn reset_database(database_url: &str) {
    sqlx::any::install_default_drivers();
    let pool: AnyPool = AnyPoolOptions::new()
        .max_connections(1)
        .connect(database_url)
        .await
        .expect("test database pool");

    sqlx::query(
        "TRUNCATE TABLE ticket_comments, tickets, notifications, occurrences, bugs, agents, accounts RESTART IDENTITY CASCADE",
    )
    .execute(&pool)
    .await
    .expect("truncate tables");

    pool.close().await;
}

async fn test_service_with_disabled_features(disabled_features: HashSet<String>) -> IntakeService {
    let config = test_config(disabled_features);
    let repository = Arc::new(Repository::connect(&config).await.expect("repository"));
    reset_database(&config.database_url).await;
    let ticketing = Arc::new(TicketingRegistry::default());
    let notifications = Arc::new(NotificationRegistry::default());
    let ai = Arc::new(AiRegistry::default());
    let feature_flags = build_feature_flags(&config).expect("feature flags");
    let policy_engine = build_policy_engine(&config).expect("policy engine");
    IntakeService::new(
        repository,
        ticketing,
        notifications,
        ai,
        feature_flags,
        policy_engine,
    )
}

#[tokio::test]
#[serial]
async fn creates_ticket_and_notification_for_new_bug() {
    let service = test_service().await;
    let account = service
        .repository
        .create_account(CreateAccountRequest {
            name: "Acme".to_string(),
            create_tickets: true,
            ticket_provider: TicketProvider::Linear,
            ticketing_api_key: Some("linear_test_key".to_string()),
            notification_provider: NotificationProvider::Slack,
            notification_api_key: Some("slack_test_key".to_string()),
            ai_enabled: true,
            use_managed_ai: true,
            ai_api_key: None,
            notify_min_level: Severity::Error,
            rapid_occurrence_window_minutes: 30,
            rapid_occurrence_threshold: 2,
        })
        .await
        .expect("account");
    let agent = service
        .repository
        .create_agent(CreateAgentRequest {
            account_id: account.id,
            name: "api".to_string(),
        })
        .await
        .expect("agent");

    let response = service
        .ingest(StacktraceEvent {
            agent_key: agent.api_key,
            agent_secret: Some(agent.api_secret),
            language: "rust".to_string(),
            stacktrace: "panic: nil pointer dereference".to_string(),
            level: Severity::Error,
            occurred_at: Some(Utc::now()),
            service: None,
            environment: None,
            attributes: Default::default(),
        })
        .await
        .expect("ingest");

    assert!(response.is_new_bug);
    assert_eq!(response.ticket_action, TicketAction::Created);
    assert!(response.notification.sent);
    assert_eq!(
        response.ticket.expect("ticket").priority,
        TicketPriority::High
    );
    assert!(
        response
            .ai_recommendation
            .expect("recommendation")
            .contains("null")
    );
}

#[tokio::test]
#[serial]
async fn escalates_existing_bug_when_it_repeats_rapidly() {
    let service = test_service().await;
    let account = service
        .repository
        .create_account(CreateAccountRequest {
            name: "Beta".to_string(),
            create_tickets: true,
            ticket_provider: TicketProvider::Jira,
            ticketing_api_key: Some("jira_test_key".to_string()),
            notification_provider: NotificationProvider::Teams,
            notification_api_key: Some("teams_test_key".to_string()),
            ai_enabled: true,
            use_managed_ai: true,
            ai_api_key: None,
            notify_min_level: Severity::Warn,
            rapid_occurrence_window_minutes: 60,
            rapid_occurrence_threshold: 2,
        })
        .await
        .expect("account");
    let agent = service
        .repository
        .create_agent(CreateAgentRequest {
            account_id: account.id,
            name: "worker".to_string(),
        })
        .await
        .expect("agent");
    let now = Utc::now();

    service
        .ingest(StacktraceEvent {
            agent_key: agent.api_key.clone(),
            agent_secret: Some(agent.api_secret.clone()),
            language: "go".to_string(),
            stacktrace: "panic: index out of bounds".to_string(),
            level: Severity::Warn,
            occurred_at: Some(now),
            service: None,
            environment: None,
            attributes: Default::default(),
        })
        .await
        .expect("first ingest");
    let second = service
        .ingest(StacktraceEvent {
            agent_key: agent.api_key,
            agent_secret: Some(agent.api_secret),
            language: "go".to_string(),
            stacktrace: "panic: index out of bounds".to_string(),
            level: Severity::Warn,
            occurred_at: Some(now + chrono::Duration::minutes(10)),
            service: None,
            environment: None,
            attributes: Default::default(),
        })
        .await
        .expect("second ingest");

    assert!(!second.is_new_bug);
    assert_eq!(second.ticket_action, TicketAction::Escalated);
    assert!(second.notification.sent);
    assert_eq!(
        second.ticket.expect("ticket").priority,
        TicketPriority::High
    );
}

#[tokio::test]
#[serial]
async fn suppresses_debug_notification() {
    let service = test_service().await;
    let account = service
        .repository
        .create_account(CreateAccountRequest {
            name: "Gamma".to_string(),
            create_tickets: true,
            ticket_provider: TicketProvider::Tracklines,
            ticketing_api_key: Some("tracklines_test_key".to_string()),
            notification_provider: NotificationProvider::Resend,
            notification_api_key: Some("resend_test_key".to_string()),
            ai_enabled: true,
            use_managed_ai: true,
            ai_api_key: None,
            notify_min_level: Severity::Error,
            rapid_occurrence_window_minutes: 15,
            rapid_occurrence_threshold: 2,
        })
        .await
        .expect("account");
    let agent = service
        .repository
        .create_agent(CreateAgentRequest {
            account_id: account.id,
            name: "frontend".to_string(),
        })
        .await
        .expect("agent");

    let response = service
        .ingest(StacktraceEvent {
            agent_key: agent.api_key,
            agent_secret: Some(agent.api_secret),
            language: "javascript".to_string(),
            stacktrace: "TypeError: Cannot read properties of undefined".to_string(),
            level: Severity::Debug,
            occurred_at: Some(Utc::now()),
            service: None,
            environment: None,
            attributes: Default::default(),
        })
        .await
        .expect("ingest");

    assert_eq!(response.ticket_action, TicketAction::Created);
    assert!(!response.notification.sent);
}

#[tokio::test]
#[serial]
async fn skips_ticket_creation_when_ticket_provider_flag_is_disabled() {
    let service = test_service_with_disabled_features(HashSet::from([String::from("jira")])).await;
    let account = service
        .repository
        .create_account(CreateAccountRequest {
            name: "Delta".to_string(),
            create_tickets: true,
            ticket_provider: TicketProvider::Jira,
            ticketing_api_key: Some("jira_test_key".to_string()),
            notification_provider: NotificationProvider::Slack,
            notification_api_key: Some("slack_test_key".to_string()),
            ai_enabled: true,
            use_managed_ai: true,
            ai_api_key: None,
            notify_min_level: Severity::Error,
            rapid_occurrence_window_minutes: 30,
            rapid_occurrence_threshold: 2,
        })
        .await
        .expect("account");
    let agent = service
        .repository
        .create_agent(CreateAgentRequest {
            account_id: account.id,
            name: "api".to_string(),
        })
        .await
        .expect("agent");

    let response = service
        .ingest(StacktraceEvent {
            agent_key: agent.api_key,
            agent_secret: Some(agent.api_secret),
            language: "rust".to_string(),
            stacktrace: "panic: nil pointer dereference".to_string(),
            level: Severity::Error,
            occurred_at: Some(Utc::now()),
            service: None,
            environment: None,
            attributes: Default::default(),
        })
        .await
        .expect("ingest");

    assert_eq!(response.ticket_action, TicketAction::Skipped);
    assert!(response.ticket.is_none());
    assert!(response.ai_recommendation.is_none());
    assert!(!response.notification.sent);
}

#[tokio::test]
#[serial]
async fn skips_notification_when_notification_provider_flag_is_disabled() {
    let service = test_service_with_disabled_features(HashSet::from([String::from("slack")])).await;
    let account = service
        .repository
        .create_account(CreateAccountRequest {
            name: "Epsilon".to_string(),
            create_tickets: true,
            ticket_provider: TicketProvider::Github,
            ticketing_api_key: Some("github_test_key".to_string()),
            notification_provider: NotificationProvider::Slack,
            notification_api_key: None,
            ai_enabled: true,
            use_managed_ai: true,
            ai_api_key: None,
            notify_min_level: Severity::Error,
            rapid_occurrence_window_minutes: 30,
            rapid_occurrence_threshold: 2,
        })
        .await
        .expect("account");
    let agent = service
        .repository
        .create_agent(CreateAgentRequest {
            account_id: account.id,
            name: "api".to_string(),
        })
        .await
        .expect("agent");

    let response = service
        .ingest(StacktraceEvent {
            agent_key: agent.api_key,
            agent_secret: Some(agent.api_secret),
            language: "rust".to_string(),
            stacktrace: "panic: nil pointer dereference".to_string(),
            level: Severity::Error,
            occurred_at: Some(Utc::now()),
            service: None,
            environment: None,
            attributes: Default::default(),
        })
        .await
        .expect("ingest");

    assert_eq!(response.ticket_action, TicketAction::Created);
    assert!(response.ticket.is_some());
    assert!(!response.notification.sent);
    assert!(response.notification.provider.is_none());
}

#[tokio::test]
#[serial]
async fn skips_ai_when_account_uses_customer_managed_ai_without_api_key() {
    let service = test_service().await;
    let account = service
        .repository
        .create_account(CreateAccountRequest {
            name: "Zeta".to_string(),
            create_tickets: true,
            ticket_provider: TicketProvider::Linear,
            ticketing_api_key: Some("linear_test_key".to_string()),
            notification_provider: NotificationProvider::Slack,
            notification_api_key: Some("slack_test_key".to_string()),
            ai_enabled: true,
            use_managed_ai: false,
            ai_api_key: None,
            notify_min_level: Severity::Error,
            rapid_occurrence_window_minutes: 30,
            rapid_occurrence_threshold: 2,
        })
        .await
        .expect("account");
    let agent = service
        .repository
        .create_agent(CreateAgentRequest {
            account_id: account.id,
            name: "api".to_string(),
        })
        .await
        .expect("agent");

    let response = service
        .ingest(StacktraceEvent {
            agent_key: agent.api_key,
            agent_secret: Some(agent.api_secret),
            language: "rust".to_string(),
            stacktrace: "panic: nil pointer dereference".to_string(),
            level: Severity::Error,
            occurred_at: Some(Utc::now()),
            service: None,
            environment: None,
            attributes: Default::default(),
        })
        .await
        .expect("ingest");

    assert_eq!(response.ticket_action, TicketAction::Created);
    assert_eq!(
        response.ai_recommendation.as_deref(),
        Some("AI recommendation skipped by policy.")
    );
}

#[tokio::test]
#[serial]
async fn creates_ticket_for_repeat_bug_when_bug_exists_without_ticket() {
    let service = test_service().await;
    let account = service
        .repository
        .create_account(CreateAccountRequest {
            name: "Eta".to_string(),
            create_tickets: true,
            ticket_provider: TicketProvider::Jira,
            ticketing_api_key: Some("jira_test_key".to_string()),
            notification_provider: NotificationProvider::Slack,
            notification_api_key: Some("slack_test_key".to_string()),
            ai_enabled: true,
            use_managed_ai: true,
            ai_api_key: None,
            notify_min_level: Severity::Error,
            rapid_occurrence_window_minutes: 30,
            rapid_occurrence_threshold: 2,
        })
        .await
        .expect("account");
    let agent = service
        .repository
        .create_agent(CreateAgentRequest {
            account_id: account.id,
            name: "worker".to_string(),
        })
        .await
        .expect("agent");
    let stacktrace = "panic: missing ticket for existing bug";
    let normalized_stacktrace = super::normalize_stacktrace(stacktrace);
    let stacktrace_hash = super::hash_stacktrace(&normalized_stacktrace);
    let occurred_at = Utc::now();

    let bug = service
        .repository
        .create_bug(CreateBugRecord {
            account_id: account.id,
            agent_id: agent.id,
            language: "rust",
            severity: Severity::Error,
            stacktrace_hash: &stacktrace_hash,
            normalized_stacktrace: &normalized_stacktrace,
            latest_stacktrace: stacktrace,
            occurred_at,
        })
        .await
        .expect("bug");
    service
        .repository
        .record_occurrence(bug.id, Severity::Error, stacktrace, occurred_at)
        .await
        .expect("occurrence");

    let response = service
        .ingest(StacktraceEvent {
            agent_key: agent.api_key,
            agent_secret: Some(agent.api_secret),
            language: "rust".to_string(),
            stacktrace: stacktrace.to_string(),
            level: Severity::Error,
            occurred_at: Some(occurred_at + chrono::Duration::minutes(5)),
            service: None,
            environment: None,
            attributes: Default::default(),
        })
        .await
        .expect("repeat ingest");

    assert!(!response.is_new_bug);
    assert_eq!(response.ticket_action, TicketAction::Created);
    assert!(response.ticket.is_some());
    assert_eq!(response.occurrence_count, 2);
}
