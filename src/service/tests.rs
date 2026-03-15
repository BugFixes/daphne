use std::{collections::HashSet, sync::Arc};

use chrono::Utc;
use serial_test::serial;

use crate::{
    ai::AiRegistry,
    domain::{
        CreateAccountRequest, CreateAgentRequest, LogEvent, NotificationEventStatus,
        NotificationProvider, Severity, StacktraceEvent, TicketAction, TicketPriority,
        TicketProvider,
    },
    feature_flags::build_feature_flags,
    notifications::NotificationRegistry,
    policy::build_policy_engine,
    repository::{CreateBugRecord, RecordOccurrence, Repository},
    test_support::{reset_database, test_config_with_disabled_features},
    ticketing::TicketingRegistry,
};

use super::{IntakeService, IntakeServiceSettings};

const GO_EQUIVALENT_TRACE_A: &str = include_str!("fixtures/go_equivalent_a.txt");
const GO_EQUIVALENT_TRACE_B: &str = include_str!("fixtures/go_equivalent_b.txt");
const RUST_EQUIVALENT_TRACE_A: &str = include_str!("fixtures/rust_equivalent_a.txt");
const RUST_EQUIVALENT_TRACE_B: &str = include_str!("fixtures/rust_equivalent_b.txt");
const RUST_DISTINCT_TRACE_A: &str = include_str!("fixtures/rust_distinct_a.txt");
const RUST_DISTINCT_TRACE_B: &str = include_str!("fixtures/rust_distinct_b.txt");

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
        IntakeServiceSettings {
            notification_cooldown_minutes: config.notification_cooldown_minutes,
            log_retention_days: config.log_retention_days,
        },
    )
}

async fn test_service_with_cooldown(notification_cooldown_minutes: i64) -> IntakeService {
    let config = test_config_with_disabled_features(HashSet::new()).await;
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
        IntakeServiceSettings {
            notification_cooldown_minutes,
            log_retention_days: config.log_retention_days,
        },
    )
}

#[tokio::test]
#[serial]
async fn creates_ticket_and_notification_for_new_bug() {
    let service = test_service().await;
    let account = service
        .repository
        .create_account(CreateAccountRequest {
            organization_id: None,
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
            organization_id: None,
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
            organization_id: None,
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
            organization_id: None,
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
            organization_id: None,
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
            organization_id: None,
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
            organization_id: None,
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
    let normalized_stacktrace = super::normalize_stacktrace("rust", stacktrace);
    let stacktrace_hash = super::hash_stacktrace(&normalized_stacktrace);
    let occurred_at = Utc::now();
    let attributes = Default::default();

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
        .record_occurrence(RecordOccurrence {
            bug_id: bug.id,
            severity: Severity::Error,
            stacktrace,
            occurred_at,
            service: None,
            environment: None,
            attributes: &attributes,
        })
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
            organization_id: None,
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
        super::normalize_stacktrace("rust", first_stacktrace)
    );
    assert_eq!(bug.latest_stacktrace, second_stacktrace);
    assert_eq!(bug.first_seen_at, first_occurred_at);
    assert_eq!(bug.last_seen_at, second_occurred_at);
    assert_eq!(bug.occurrence_count, 2);

    let occurrences = service
        .repository
        .list_occurrences_for_bug(bug.id)
        .await
        .expect("occurrences");

    assert_eq!(occurrences.len(), 2);
    assert_eq!(occurrences[0].service.as_deref(), Some("api"));
    assert_eq!(occurrences[0].environment.as_deref(), Some("prod"));
    assert_eq!(
        occurrences[0].attributes.get("release").map(String::as_str),
        Some("1.0.0")
    );
    assert_eq!(occurrences[1].service.as_deref(), Some("worker"));
    assert_eq!(occurrences[1].environment.as_deref(), Some("staging"));
    assert_eq!(
        occurrences[1].attributes.get("release").map(String::as_str),
        Some("1.0.1")
    );
}

#[tokio::test]
#[serial]
async fn comments_on_repeat_bug_before_reaching_escalation_threshold() {
    let service = test_service().await;
    let account = service
        .repository
        .create_account(CreateAccountRequest {
            organization_id: None,
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
    let second_ticket = second.ticket.clone().expect("ticket");
    assert_eq!(second_ticket.priority, TicketPriority::Medium);

    let ticket_events = service
        .repository
        .list_ticket_events_for_bug(first.bug_id)
        .await
        .expect("ticket events");
    let notification_events = service
        .repository
        .list_notification_events_for_bug(first.bug_id)
        .await
        .expect("notification events");

    assert_eq!(ticket_events.len(), 2);
    assert_eq!(ticket_events[0].action, TicketAction::Created);
    assert_eq!(ticket_events[0].next_priority, Some(TicketPriority::Medium));
    assert_eq!(ticket_events[1].action, TicketAction::Commented);
    assert_eq!(
        ticket_events[1].previous_priority,
        Some(TicketPriority::Medium)
    );
    assert_eq!(ticket_events[1].next_priority, Some(TicketPriority::Medium));
    assert_eq!(ticket_events[1].occurred_at, second_ticket.updated_at);

    assert_eq!(notification_events.len(), 2);
    assert_eq!(notification_events[0].status, NotificationEventStatus::Sent);
    assert_eq!(notification_events[0].ticket_action, TicketAction::Created);
    assert_eq!(
        notification_events[1].status,
        NotificationEventStatus::Skipped
    );
    assert_eq!(notification_events[1].reason, "policy_denied");
    assert_eq!(
        notification_events[1].ticket_action,
        TicketAction::Commented
    );
}

#[tokio::test]
#[serial]
async fn comments_on_repeat_bug_when_ticket_priority_is_already_critical() {
    let service = test_service().await;
    let account = service
        .repository
        .create_account(CreateAccountRequest {
            organization_id: None,
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
            organization_id: None,
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

#[tokio::test]
#[serial]
async fn deduplicates_go_stacktraces_with_different_goroutine_ids() {
    let service = test_service().await;
    let account = service
        .repository
        .create_account(CreateAccountRequest {
            organization_id: None,
            name: "Mu".to_string(),
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
    let occurred_at = Utc::now();

    let first = service
        .ingest(StacktraceEvent {
            agent_key: agent.api_key.clone(),
            agent_secret: Some(agent.api_secret.clone()),
            language: "go".to_string(),
            stacktrace: "goroutine 18 [running]:\npanic: worker crashed\nmain.run()".to_string(),
            level: Severity::Error,
            occurred_at: Some(occurred_at),
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
            stacktrace: "goroutine 42 [running]:\npanic: worker crashed\nmain.run()".to_string(),
            level: Severity::Error,
            occurred_at: Some(occurred_at + chrono::Duration::minutes(1)),
            service: None,
            environment: None,
            attributes: Default::default(),
        })
        .await
        .expect("second ingest");

    assert!(first.is_new_bug);
    assert!(!second.is_new_bug);
    assert_eq!(first.stacktrace_hash, second.stacktrace_hash);
}

#[test]
fn normalizes_equivalent_go_fixture_stacktraces() {
    let first = super::normalize_stacktrace("go", GO_EQUIVALENT_TRACE_A);
    let second = super::normalize_stacktrace("go", GO_EQUIVALENT_TRACE_B);

    assert_eq!(first, second);
    assert!(first.contains("goroutine N [running]:"));
    assert!(first.contains("/srv/app/worker.go:87 +0xOFFSET"));
}

#[test]
fn normalizes_equivalent_rust_fixture_stacktraces() {
    let first = super::normalize_stacktrace("rust", RUST_EQUIVALENT_TRACE_A);
    let second = super::normalize_stacktrace("rust", RUST_EQUIVALENT_TRACE_B);

    assert_eq!(first, second);
    assert!(first.contains("0xADDR - my_app::worker::run::hHASH"));
    assert!(first.contains("at /rustc/RUSTC/library/std/src/sys/backtrace.rs:158:18"));
}

#[test]
fn keeps_distinct_rust_fixture_stacktraces_separate() {
    let first = super::normalize_stacktrace("rust", RUST_DISTINCT_TRACE_A);
    let second = super::normalize_stacktrace("rust", RUST_DISTINCT_TRACE_B);

    assert_ne!(first, second);
}

#[tokio::test]
#[serial]
async fn stores_logs_without_creating_bug_records() {
    let service = test_service().await;
    let account = service
        .repository
        .create_account(CreateAccountRequest {
            organization_id: None,
            name: "Nu".to_string(),
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

    let outcome = service
        .ingest_log(LogEvent {
            agent_key: agent.api_key,
            agent_secret: Some(agent.api_secret),
            language: "rust".to_string(),
            message: "db timeout".to_string(),
            stacktrace: Some("frame_one".to_string()),
            level: Severity::Error,
            occurred_at: Some(Utc::now()),
            service: Some("api".to_string()),
            environment: Some("prod".to_string()),
            attributes: std::collections::HashMap::from([(
                "source".to_string(),
                "agent_log".to_string(),
            )]),
        })
        .await
        .expect("ingest log");

    assert!(outcome.stored);

    let logs = service
        .repository
        .list_logs_for_account(account.id)
        .await
        .expect("logs");

    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].message, "db timeout");
    assert_eq!(logs[0].stacktrace.as_deref(), Some("frame_one"));
    assert_eq!(logs[0].service.as_deref(), Some("api"));

    let bug = service
        .repository
        .find_bug_by_hash(account.id, "non-existent")
        .await
        .expect("bug lookup");
    assert!(bug.is_none());
}

#[tokio::test]
#[serial]
async fn archives_and_trims_logs_older_than_retention_window() {
    let service = test_service().await;
    let account = service
        .repository
        .create_account(CreateAccountRequest {
            organization_id: None,
            name: "Xi".to_string(),
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
    let now = Utc::now();

    service
        .ingest_log(LogEvent {
            agent_key: agent.api_key.clone(),
            agent_secret: Some(agent.api_secret.clone()),
            language: "rust".to_string(),
            message: "old log".to_string(),
            stacktrace: None,
            level: Severity::Warn,
            occurred_at: Some(now - chrono::Duration::days(31)),
            service: None,
            environment: None,
            attributes: Default::default(),
        })
        .await
        .expect("old log");
    service
        .ingest_log(LogEvent {
            agent_key: agent.api_key,
            agent_secret: Some(agent.api_secret),
            language: "rust".to_string(),
            message: "fresh log".to_string(),
            stacktrace: None,
            level: Severity::Warn,
            occurred_at: Some(now - chrono::Duration::days(1)),
            service: None,
            environment: None,
            attributes: Default::default(),
        })
        .await
        .expect("fresh log");

    let outcome = service
        .run_log_retention(now)
        .await
        .expect("retention outcome");

    assert_eq!(outcome.archived_batches, 1);
    assert_eq!(outcome.archived_logs, 1);
    assert_eq!(outcome.deleted_logs, 1);

    let logs = service
        .repository
        .list_logs_for_account(account.id)
        .await
        .expect("logs");
    let archives = service
        .repository
        .list_log_archives()
        .await
        .expect("archives");

    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].message, "fresh log");
    assert_eq!(archives.len(), 1);
    assert_eq!(archives[0].log_count, 1);
}

#[tokio::test]
#[serial]
async fn suppresses_repeat_notifications_inside_cooldown_window() {
    let service = test_service_with_cooldown(60).await;
    let account = service
        .repository
        .create_account(CreateAccountRequest {
            organization_id: None,
            name: "Omicron".to_string(),
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

    let first = service
        .ingest(StacktraceEvent {
            agent_key: agent.api_key.clone(),
            agent_secret: Some(agent.api_secret.clone()),
            language: "rust".to_string(),
            stacktrace: "panic: retry storm".to_string(),
            level: Severity::Error,
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
            language: "rust".to_string(),
            stacktrace: "panic: retry storm".to_string(),
            level: Severity::Error,
            occurred_at: Some(now + chrono::Duration::minutes(10)),
            service: None,
            environment: None,
            attributes: Default::default(),
        })
        .await
        .expect("second ingest");

    assert!(first.notification.sent);
    assert_eq!(second.ticket_action, TicketAction::Escalated);
    assert!(!second.notification.sent);

    let notification_events = service
        .repository
        .list_notification_events_for_bug(first.bug_id)
        .await
        .expect("notification events");

    assert_eq!(notification_events.len(), 2);
    assert_eq!(
        notification_events[1].status,
        NotificationEventStatus::Skipped
    );
    assert_eq!(notification_events[1].reason, "cooldown_active");
}
