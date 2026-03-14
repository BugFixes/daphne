use std::{collections::HashSet, sync::Arc};

use chrono::Utc;
use serial_test::serial;

use crate::{
    ai::AiRegistry,
    domain::{
        CreateAccountRequest, CreateAgentRequest, NotificationProvider, Severity, StacktraceEvent,
        TicketAction, TicketPriority, TicketProvider,
    },
    feature_flags::build_feature_flags,
    notifications::NotificationRegistry,
    policy::build_policy_engine,
    repository::{CreateBugRecord, Repository},
    test_support::{reset_database, test_config_with_disabled_features},
    ticketing::TicketingRegistry,
};

use super::IntakeService;

async fn test_service() -> IntakeService {
    test_service_with_disabled_features(HashSet::new()).await
}

async fn test_service_with_disabled_features(disabled_features: HashSet<String>) -> IntakeService {
    let config = test_config_with_disabled_features(disabled_features).await;
    let repository = Arc::new(Repository::connect(&config).await.expect("repository"));
    reset_database().await;
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

#[tokio::test]
#[serial]
async fn deduplicates_by_normalized_stacktrace_and_updates_derived_bug_fields() {
    let service = test_service().await;
    let account = service
        .repository
        .create_account(CreateAccountRequest {
            name: "Theta".to_string(),
            create_tickets: false,
            ticket_provider: TicketProvider::Github,
            ticketing_api_key: None,
            notification_provider: NotificationProvider::Slack,
            notification_api_key: None,
            ai_enabled: false,
            use_managed_ai: true,
            ai_api_key: None,
            notify_min_level: Severity::Fatal,
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
    let first_occurred_at = Utc::now();
    let second_occurred_at = first_occurred_at + chrono::Duration::minutes(5);
    let first_stacktrace = "panic: nil pointer 0xabc\n  frame_one";
    let second_stacktrace = " panic: nil pointer 0xdef \n\n frame_one ";

    let first = service
        .ingest(StacktraceEvent {
            agent_key: agent.api_key.clone(),
            agent_secret: Some(agent.api_secret.clone()),
            language: "rust".to_string(),
            stacktrace: first_stacktrace.to_string(),
            level: Severity::Warn,
            occurred_at: Some(first_occurred_at),
            service: Some("api".to_string()),
            environment: Some("prod".to_string()),
            attributes: std::collections::HashMap::from([(
                "release".to_string(),
                "1.0.0".to_string(),
            )]),
        })
        .await
        .expect("first ingest");
    let second = service
        .ingest(StacktraceEvent {
            agent_key: agent.api_key,
            agent_secret: Some(agent.api_secret),
            language: "go".to_string(),
            stacktrace: second_stacktrace.to_string(),
            level: Severity::Fatal,
            occurred_at: Some(second_occurred_at),
            service: Some("worker".to_string()),
            environment: Some("staging".to_string()),
            attributes: std::collections::HashMap::from([(
                "release".to_string(),
                "1.0.1".to_string(),
            )]),
        })
        .await
        .expect("second ingest");

    assert!(first.is_new_bug);
    assert!(!second.is_new_bug);
    assert_eq!(first.stacktrace_hash, second.stacktrace_hash);
    assert_eq!(second.occurrence_count, 2);

    let bug = service
        .repository
        .find_bug_by_hash(account.id, &first.stacktrace_hash)
        .await
        .expect("bug lookup")
        .expect("bug");

    assert_eq!(bug.language, "rust");
    assert_eq!(bug.agent_id, agent.id);
    assert_eq!(bug.severity, Severity::Fatal);
    assert_eq!(
        bug.normalized_stacktrace,
        super::normalize_stacktrace(first_stacktrace)
    );
    assert_eq!(bug.latest_stacktrace, second_stacktrace);
    assert_eq!(bug.first_seen_at, first_occurred_at);
    assert_eq!(bug.last_seen_at, second_occurred_at);
    assert_eq!(bug.occurrence_count, 2);
}

#[tokio::test]
#[serial]
async fn comments_on_repeat_bug_before_reaching_escalation_threshold() {
    let service = test_service().await;
    let account = service
        .repository
        .create_account(CreateAccountRequest {
            name: "Iota".to_string(),
            create_tickets: true,
            ticket_provider: TicketProvider::Jira,
            ticketing_api_key: Some("jira_test_key".to_string()),
            notification_provider: NotificationProvider::Slack,
            notification_api_key: Some("slack_test_key".to_string()),
            ai_enabled: true,
            use_managed_ai: true,
            ai_api_key: None,
            notify_min_level: Severity::Warn,
            rapid_occurrence_window_minutes: 60,
            rapid_occurrence_threshold: 3,
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
    let first_occurred_at = Utc::now();

    let first = service
        .ingest(StacktraceEvent {
            agent_key: agent.api_key.clone(),
            agent_secret: Some(agent.api_secret.clone()),
            language: "go".to_string(),
            stacktrace: "panic: temporary backend timeout".to_string(),
            level: Severity::Warn,
            occurred_at: Some(first_occurred_at),
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
            stacktrace: "panic: temporary backend timeout".to_string(),
            level: Severity::Warn,
            occurred_at: Some(first_occurred_at + chrono::Duration::minutes(10)),
            service: None,
            environment: None,
            attributes: Default::default(),
        })
        .await
        .expect("second ingest");

    assert!(first.is_new_bug);
    assert_eq!(first.ticket_action, TicketAction::Created);
    assert!(!second.is_new_bug);
    assert_eq!(second.ticket_action, TicketAction::Commented);
    assert_eq!(second.occurrence_count, 2);
    assert!(!second.notification.sent);
    assert_eq!(
        second.ticket.expect("ticket").priority,
        TicketPriority::Medium
    );
}

#[tokio::test]
#[serial]
async fn comments_on_repeat_bug_when_ticket_priority_is_already_critical() {
    let service = test_service().await;
    let account = service
        .repository
        .create_account(CreateAccountRequest {
            name: "Kappa".to_string(),
            create_tickets: true,
            ticket_provider: TicketProvider::Linear,
            ticketing_api_key: Some("linear_test_key".to_string()),
            notification_provider: NotificationProvider::Slack,
            notification_api_key: Some("slack_test_key".to_string()),
            ai_enabled: true,
            use_managed_ai: true,
            ai_api_key: None,
            notify_min_level: Severity::Warn,
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
    let first_occurred_at = Utc::now();

    let first = service
        .ingest(StacktraceEvent {
            agent_key: agent.api_key.clone(),
            agent_secret: Some(agent.api_secret.clone()),
            language: "rust".to_string(),
            stacktrace: "panic: unrecoverable allocator corruption".to_string(),
            level: Severity::Fatal,
            occurred_at: Some(first_occurred_at),
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
            language: "rust".to_string(),
            stacktrace: "panic: unrecoverable allocator corruption".to_string(),
            level: Severity::Fatal,
            occurred_at: Some(first_occurred_at + chrono::Duration::minutes(5)),
            service: None,
            environment: None,
            attributes: Default::default(),
        })
        .await
        .expect("second ingest");

    assert_eq!(
        first.ticket.expect("ticket").priority,
        TicketPriority::Critical
    );
    assert_eq!(second.ticket_action, TicketAction::Commented);
    assert!(!second.notification.sent);
    assert_eq!(
        second.ticket.expect("ticket").priority,
        TicketPriority::Critical
    );
}

#[tokio::test]
#[serial]
async fn does_not_deduplicate_meaningfully_different_stacktraces() {
    let service = test_service().await;
    let account = service
        .repository
        .create_account(CreateAccountRequest {
            name: "Lambda".to_string(),
            create_tickets: false,
            ticket_provider: TicketProvider::Github,
            ticketing_api_key: None,
            notification_provider: NotificationProvider::Slack,
            notification_api_key: None,
            ai_enabled: false,
            use_managed_ai: true,
            ai_api_key: None,
            notify_min_level: Severity::Fatal,
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
    let first_occurred_at = Utc::now();
    let second_occurred_at = first_occurred_at + chrono::Duration::minutes(5);
    let first_stacktrace = "panic: nil pointer 0xabc\n  frame_one";
    let second_stacktrace = "panic: nil pointer 0xdef\n  frame_two";

    let first = service
        .ingest(StacktraceEvent {
            agent_key: agent.api_key.clone(),
            agent_secret: Some(agent.api_secret.clone()),
            language: "rust".to_string(),
            stacktrace: first_stacktrace.to_string(),
            level: Severity::Error,
            occurred_at: Some(first_occurred_at),
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
            language: "rust".to_string(),
            stacktrace: second_stacktrace.to_string(),
            level: Severity::Error,
            occurred_at: Some(second_occurred_at),
            service: None,
            environment: None,
            attributes: Default::default(),
        })
        .await
        .expect("second ingest");

    assert!(first.is_new_bug);
    assert!(second.is_new_bug);
    assert_ne!(first.stacktrace_hash, second.stacktrace_hash);
}
