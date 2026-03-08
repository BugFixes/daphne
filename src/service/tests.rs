use std::sync::Arc;

use chrono::Utc;

use crate::{
    config::Config,
    domain::{
        CreateAccountRequest, CreateAgentRequest, NotificationProvider, Severity,
        StacktraceEventRequest, TicketAction, TicketPriority, TicketProvider,
    },
    providers::ProviderRegistry,
    repository::Repository,
};

use super::IntakeService;

async fn test_service() -> IntakeService {
    let repository = Arc::new(
        Repository::connect(&Config {
            bind_address: "127.0.0.1:0".to_string(),
            database_url: "sqlite::memory:".to_string(),
        })
        .await
        .expect("repository"),
    );
    let providers = Arc::new(ProviderRegistry::default());
    IntakeService::new(repository, providers)
}

#[tokio::test]
async fn creates_ticket_and_notification_for_new_bug() {
    let service = test_service().await;
    let account = service
        .repository
        .create_account(CreateAccountRequest {
            name: "Acme".to_string(),
            create_tickets: true,
            ticket_provider: TicketProvider::Linear,
            notification_provider: NotificationProvider::Slack,
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
        .ingest(StacktraceEventRequest {
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
async fn escalates_existing_bug_when_it_repeats_rapidly() {
    let service = test_service().await;
    let account = service
        .repository
        .create_account(CreateAccountRequest {
            name: "Beta".to_string(),
            create_tickets: true,
            ticket_provider: TicketProvider::Jira,
            notification_provider: NotificationProvider::Teams,
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
        .ingest(StacktraceEventRequest {
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
        .ingest(StacktraceEventRequest {
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
async fn suppresses_debug_notification() {
    let service = test_service().await;
    let account = service
        .repository
        .create_account(CreateAccountRequest {
            name: "Gamma".to_string(),
            create_tickets: true,
            ticket_provider: TicketProvider::Tracklines,
            notification_provider: NotificationProvider::Resend,
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
        .ingest(StacktraceEventRequest {
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
