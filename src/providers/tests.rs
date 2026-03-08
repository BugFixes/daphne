use chrono::Utc;
use uuid::Uuid;

use crate::domain::{Bug, NotificationProvider, Severity, TicketProvider};

use super::ProviderRegistry;

#[test]
fn registry_returns_expected_ticketing_provider() {
    let registry = ProviderRegistry::default();
    let provider = registry
        .ticketing(TicketProvider::Jira)
        .expect("jira provider");

    assert_eq!(provider.kind(), TicketProvider::Jira);
}

#[test]
fn registry_returns_expected_notification_provider() {
    let registry = ProviderRegistry::default();
    let provider = registry
        .notifications(NotificationProvider::Slack)
        .expect("slack provider");

    assert_eq!(provider.kind(), NotificationProvider::Slack);
}

#[tokio::test]
async fn ai_advisor_returns_timeout_guidance() {
    let registry = ProviderRegistry::default();
    let recommendation = registry
        .ai()
        .recommend_fix(
            &Bug {
                id: Uuid::new_v4(),
                account_id: Uuid::new_v4(),
                agent_id: Uuid::new_v4(),
                language: "rust".to_string(),
                severity: Severity::Error,
                stacktrace_hash: "hash".to_string(),
                normalized_stacktrace: "timeout".to_string(),
                latest_stacktrace: "timeout".to_string(),
                first_seen_at: Utc::now(),
                last_seen_at: Utc::now(),
                occurrence_count: 1,
            },
            "request timeout while calling upstream",
        )
        .await
        .expect("recommendation");

    assert!(recommendation.contains("latency"));
}
