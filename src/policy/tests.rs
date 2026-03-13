use crate::policy::{
    CreateTicketAccountPolicyInput, CreateTicketPolicyInput, CreateTicketProviderPolicyInput,
    CreateTicketStackPolicyInput, EscalateRepeatBugPolicyInput, EscalateRepeatPolicyInput,
    EscalateRepeatTicketPolicyInput, LocalPolicyEngine, Policy2PolicyEngine, PolicyEngine,
    SendNotificationAccountPolicyInput, SendNotificationEventPolicyInput,
    SendNotificationPolicyInput, SendNotificationProviderPolicyInput,
    SendNotificationTicketPolicyInput, UseAiAccountPolicyInput, UseAiAdvisorPolicyInput,
    UseAiPolicyInput,
};

#[tokio::test]
async fn local_policy_engine_matches_ticket_creation_rule() {
    let engine = LocalPolicyEngine;
    let allowed = engine
        .should_create_ticket(&CreateTicketPolicyInput {
            stack: CreateTicketStackPolicyInput { hash_exists: false },
            account: CreateTicketAccountPolicyInput {
                ticketing_enabled: true,
                api_key: Some("jira_key".to_string()),
            },
            ticketing: CreateTicketProviderPolicyInput {
                provider: "jira".to_string(),
                enabled: true,
            },
        })
        .await
        .expect("result");
    let denied = engine
        .should_create_ticket(&CreateTicketPolicyInput {
            stack: CreateTicketStackPolicyInput { hash_exists: false },
            account: CreateTicketAccountPolicyInput {
                ticketing_enabled: true,
                api_key: Some("jira_key".to_string()),
            },
            ticketing: CreateTicketProviderPolicyInput {
                provider: "jira".to_string(),
                enabled: false,
            },
        })
        .await
        .expect("result");

    assert!(allowed);
    assert!(!denied);
}

#[tokio::test]
async fn local_policy_engine_matches_notification_rule() {
    let engine = LocalPolicyEngine;
    let allowed = engine
        .should_send_notification(&SendNotificationPolicyInput {
            event: SendNotificationEventPolicyInput { rank: 4 },
            account: SendNotificationAccountPolicyInput {
                notify_min_rank: 4,
                api_key: Some("slack_key".to_string()),
            },
            notification: SendNotificationProviderPolicyInput {
                provider: "slack".to_string(),
                enabled: true,
            },
            ticket: SendNotificationTicketPolicyInput {
                action: "created".to_string(),
            },
        })
        .await
        .expect("result");
    let denied = engine
        .should_send_notification(&SendNotificationPolicyInput {
            event: SendNotificationEventPolicyInput { rank: 3 },
            account: SendNotificationAccountPolicyInput {
                notify_min_rank: 4,
                api_key: Some("slack_key".to_string()),
            },
            notification: SendNotificationProviderPolicyInput {
                provider: "slack".to_string(),
                enabled: true,
            },
            ticket: SendNotificationTicketPolicyInput {
                action: "commented".to_string(),
            },
        })
        .await
        .expect("result");

    assert!(allowed);
    assert!(!denied);
}

#[tokio::test]
async fn local_policy_engine_matches_repeat_escalation_rule() {
    let engine = LocalPolicyEngine;
    let allowed = engine
        .should_escalate_repeat(&EscalateRepeatPolicyInput {
            bug: EscalateRepeatBugPolicyInput {
                has_ticket: true,
                recent_count: 3,
                rapid_occurrence_threshold: 2,
            },
            ticket: EscalateRepeatTicketPolicyInput {
                current_priority_rank: 2,
                next_priority_rank: 3,
            },
        })
        .await
        .expect("result");
    let denied = engine
        .should_escalate_repeat(&EscalateRepeatPolicyInput {
            bug: EscalateRepeatBugPolicyInput {
                has_ticket: true,
                recent_count: 1,
                rapid_occurrence_threshold: 2,
            },
            ticket: EscalateRepeatTicketPolicyInput {
                current_priority_rank: 2,
                next_priority_rank: 3,
            },
        })
        .await
        .expect("result");

    assert!(allowed);
    assert!(!denied);
}

#[tokio::test]
async fn local_policy_engine_matches_ai_rule() {
    let engine = LocalPolicyEngine;
    let allowed = engine
        .should_use_ai(&UseAiPolicyInput {
            account: UseAiAccountPolicyInput {
                enabled: true,
                use_managed: true,
                api_key: None,
            },
            ai: UseAiAdvisorPolicyInput {
                advisor: "codex".to_string(),
                enabled: true,
            },
        })
        .await
        .expect("result");
    let denied = engine
        .should_use_ai(&UseAiPolicyInput {
            account: UseAiAccountPolicyInput {
                enabled: true,
                use_managed: false,
                api_key: None,
            },
            ai: UseAiAdvisorPolicyInput {
                advisor: "codex".to_string(),
                enabled: false,
            },
        })
        .await
        .expect("result");

    assert!(allowed);
    assert!(!denied);
}

#[tokio::test]
async fn local_policy_engine_blocks_ticket_creation_when_stack_hash_already_exists() {
    let engine = LocalPolicyEngine;
    let denied = engine
        .should_create_ticket(&CreateTicketPolicyInput {
            stack: CreateTicketStackPolicyInput { hash_exists: true },
            account: CreateTicketAccountPolicyInput {
                ticketing_enabled: true,
                api_key: Some("jira_key".to_string()),
            },
            ticketing: CreateTicketProviderPolicyInput {
                provider: "jira".to_string(),
                enabled: true,
            },
        })
        .await
        .expect("result");

    assert!(!denied);
}

#[tokio::test]
async fn local_policy_engine_blocks_ticket_creation_when_api_key_is_missing() {
    let engine = LocalPolicyEngine;
    let denied = engine
        .should_create_ticket(&CreateTicketPolicyInput {
            stack: CreateTicketStackPolicyInput { hash_exists: false },
            account: CreateTicketAccountPolicyInput {
                ticketing_enabled: true,
                api_key: None,
            },
            ticketing: CreateTicketProviderPolicyInput {
                provider: "jira".to_string(),
                enabled: true,
            },
        })
        .await
        .expect("result");

    assert!(!denied);
}

#[tokio::test]
async fn policy2_engine_falls_back_to_local_policy_on_request_failure() {
    let engine = Policy2PolicyEngine {
        client: reqwest::Client::new(),
        endpoint: "http://127.0.0.1:9/run".to_string(),
        fallback: LocalPolicyEngine,
    };

    let allowed = engine
        .should_create_ticket(&CreateTicketPolicyInput {
            stack: CreateTicketStackPolicyInput { hash_exists: false },
            account: CreateTicketAccountPolicyInput {
                ticketing_enabled: true,
                api_key: Some("jira_key".to_string()),
            },
            ticketing: CreateTicketProviderPolicyInput {
                provider: "jira".to_string(),
                enabled: true,
            },
        })
        .await
        .expect("fallback result");

    assert!(allowed);
}
