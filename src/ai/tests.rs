use chrono::Utc;
use uuid::Uuid;

use crate::domain::{Bug, Severity};

use super::AiRegistry;

#[tokio::test]
async fn default_ai_advisor_returns_timeout_guidance() {
    let registry = AiRegistry::default();
    let recommendation = registry
        .default_advisor()
        .expect("default advisor")
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
