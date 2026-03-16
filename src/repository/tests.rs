use std::collections::{HashMap, HashSet};

use chrono::Utc;
use serial_test::serial;

use crate::{
    domain::{
        AccountProviderKind, AddOrganizationMemberRequest, CreateAccountRequest,
        CreateAgentRequest, CreateOrganizationRequest, NotificationProvider, OrganizationRole,
        Severity, TicketAction, TicketProvider, UpdateOrganizationMembershipRequest,
    },
    test_support::{reset_database, test_config_with_disabled_features},
};

use super::{CreateBugRecord, RecordOccurrence, Repository};

#[tokio::test]
#[serial]
async fn connects_to_postgres_after_running_migrations() {
    let config = test_config_with_disabled_features(HashSet::new()).await;
    let repository = Repository::connect(&config).await.expect("repository");
    let migration_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM refinery_schema_history")
        .fetch_one(&repository.pool)
        .await
        .expect("migration count");

    assert!(migration_count >= 5);
}

#[tokio::test]
#[serial]
async fn persists_account_provider_config_snapshots() {
    let config = test_config_with_disabled_features(HashSet::new()).await;
    let repository = Repository::connect(&config).await.expect("repository");
    reset_database().await;

    let account = repository
        .create_account(CreateAccountRequest {
            organization_id: None,
            name: "Acme".to_string(),
            create_tickets: true,
            ticket_provider: TicketProvider::Jira,
            ticketing_api_key: Some("jira_test_key".to_string()),
            notification_provider: NotificationProvider::Slack,
            notification_api_key: Some("slack_test_key".to_string()),
            ai_enabled: true,
            use_managed_ai: false,
            ai_api_key: Some("openai_test_key".to_string()),
            notify_min_level: Severity::Warn,
            rapid_occurrence_window_minutes: 30,
            rapid_occurrence_threshold: 2,
        })
        .await
        .expect("account");

    let configs = repository
        .list_account_provider_configs(account.id)
        .await
        .expect("provider configs");

    assert_eq!(configs.len(), 3);
    assert!(configs.iter().any(|config| {
        config.kind == AccountProviderKind::Ticketing
            && config.provider == "jira"
            && config.api_key.as_deref() == Some("jira_test_key")
    }));
    assert!(configs.iter().any(|config| {
        config.kind == AccountProviderKind::Notification
            && config.provider == "slack"
            && config.settings["notify_min_level"] == "warn"
    }));
    assert!(configs.iter().any(|config| {
        config.kind == AccountProviderKind::Ai
            && config.provider == "customer_managed"
            && config.api_key.as_deref() == Some("openai_test_key")
            && config.settings["enabled"] == true
    }));
    assert_ne!(account.organization_id, account.id);
}

#[tokio::test]
#[serial]
async fn manages_organization_memberships() {
    let config = test_config_with_disabled_features(HashSet::new()).await;
    let repository = Repository::connect(&config).await.expect("repository");
    reset_database().await;

    let organization = repository
        .create_organization(CreateOrganizationRequest {
            name: "Acme".to_string(),
            clerk_org_id: Some("org_test_acme".to_string()),
            owner_clerk_user_id: "user_owner_test".to_string(),
            owner_name: "Owner".to_string(),
        })
        .await
        .expect("organization");

    let organizations = repository
        .list_organizations_for_user("user_owner_test")
        .await
        .expect("organizations");
    assert_eq!(organizations.len(), 1);
    assert_eq!(
        organizations[0].organization.id,
        organization.organization.id
    );
    assert_eq!(organizations[0].membership.role, OrganizationRole::Owner);

    let membership = repository
        .add_organization_member(
            organization.organization.id,
            "user_owner_test",
            AddOrganizationMemberRequest {
                clerk_user_id: "user_member_test".to_string(),
                name: "Member".to_string(),
                role: OrganizationRole::Member,
            },
        )
        .await
        .expect("membership");
    assert_eq!(membership.user.clerk_user_id, "user_member_test");
    assert_eq!(membership.membership.role, OrganizationRole::Member);

    let memberships = repository
        .list_organization_memberships(organization.organization.id, "user_member_test")
        .await
        .expect("memberships");
    assert_eq!(memberships.len(), 2);

    let updated = repository
        .update_organization_membership(
            organization.organization.id,
            membership.membership.id,
            "user_owner_test",
            UpdateOrganizationMembershipRequest {
                role: OrganizationRole::Admin,
            },
        )
        .await
        .expect("updated membership");
    assert_eq!(updated.membership.role, OrganizationRole::Admin);
}

#[tokio::test]
#[serial]
async fn scopes_dashboard_repository_queries_to_clerk_org() {
    let config = test_config_with_disabled_features(HashSet::new()).await;
    let repository = Repository::connect(&config).await.expect("repository");
    reset_database().await;

    let org_one = repository
        .create_organization(CreateOrganizationRequest {
            name: "Acme".to_string(),
            clerk_org_id: Some("org_acme".to_string()),
            owner_clerk_user_id: "user_owner_acme".to_string(),
            owner_name: "Acme Owner".to_string(),
        })
        .await
        .expect("org one");
    let org_two = repository
        .create_organization(CreateOrganizationRequest {
            name: "Globex".to_string(),
            clerk_org_id: Some("org_globex".to_string()),
            owner_clerk_user_id: "user_owner_globex".to_string(),
            owner_name: "Globex Owner".to_string(),
        })
        .await
        .expect("org two");

    let account_one = repository
        .create_account(CreateAccountRequest {
            organization_id: Some(org_one.organization.id),
            name: "Acme Account".to_string(),
            create_tickets: false,
            ticket_provider: TicketProvider::Github,
            ticketing_api_key: None,
            notification_provider: NotificationProvider::Slack,
            notification_api_key: Some("slack_test_key".to_string()),
            ai_enabled: false,
            use_managed_ai: true,
            ai_api_key: None,
            notify_min_level: Severity::Warn,
            rapid_occurrence_window_minutes: 30,
            rapid_occurrence_threshold: 2,
        })
        .await
        .expect("account one");
    let account_two = repository
        .create_account(CreateAccountRequest {
            organization_id: Some(org_two.organization.id),
            name: "Globex Account".to_string(),
            create_tickets: false,
            ticket_provider: TicketProvider::Github,
            ticketing_api_key: None,
            notification_provider: NotificationProvider::Slack,
            notification_api_key: Some("slack_test_key".to_string()),
            ai_enabled: false,
            use_managed_ai: true,
            ai_api_key: None,
            notify_min_level: Severity::Warn,
            rapid_occurrence_window_minutes: 30,
            rapid_occurrence_threshold: 2,
        })
        .await
        .expect("account two");

    let agent_one = repository
        .create_agent(CreateAgentRequest {
            account_id: account_one.id,
            name: "acme-agent".to_string(),
        })
        .await
        .expect("agent one");
    let agent_two = repository
        .create_agent(CreateAgentRequest {
            account_id: account_two.id,
            name: "globex-agent".to_string(),
        })
        .await
        .expect("agent two");

    let bug_one = repository
        .create_bug(CreateBugRecord {
            account_id: account_one.id,
            agent_id: agent_one.id,
            language: "rust",
            severity: Severity::Error,
            stacktrace_hash: "hash_acme",
            normalized_stacktrace: "\n\npanic: acme exploded",
            latest_stacktrace: "panic: acme exploded",
            occurred_at: Utc::now(),
        })
        .await
        .expect("bug one");
    let bug_two = repository
        .create_bug(CreateBugRecord {
            account_id: account_two.id,
            agent_id: agent_two.id,
            language: "go",
            severity: Severity::Warn,
            stacktrace_hash: "hash_globex",
            normalized_stacktrace: "warn: globex slowed",
            latest_stacktrace: "warn: globex slowed",
            occurred_at: Utc::now(),
        })
        .await
        .expect("bug two");

    let attributes = HashMap::new();
    repository
        .record_occurrence(RecordOccurrence {
            bug_id: bug_one.id,
            severity: Severity::Error,
            stacktrace: "panic: acme exploded",
            occurred_at: Utc::now(),
            service: Some("api"),
            environment: Some("prod"),
            attributes: &attributes,
        })
        .await
        .expect("occurrence");
    repository
        .record_notification(
            bug_one.id,
            NotificationProvider::Slack,
            "Acme bug notification",
            Severity::Error,
            TicketAction::Unchanged,
            Utc::now(),
        )
        .await
        .expect("notification");

    let bugs = repository
        .list_bugs("org_acme")
        .await
        .expect("scoped bug list");
    assert_eq!(bugs.len(), 1);
    assert_eq!(bugs[0].id, bug_one.id.to_string());
    assert_eq!(bugs[0].account_name, "Acme Account");
    assert_eq!(bugs[0].agent_name, "acme-agent");
    assert_eq!(bugs[0].occurrence_count, 1);
    assert_eq!(bugs[0].notification_status, "sent");

    let found_bug = repository
        .find_bug_by_id_scoped(bug_one.id, "org_acme")
        .await
        .expect("scoped bug")
        .expect("bug present");
    assert_eq!(found_bug.id, bug_one.id);

    let missing_bug = repository
        .find_bug_by_id_scoped(bug_two.id, "org_acme")
        .await
        .expect("bug lookup");
    assert!(missing_bug.is_none());

    let account = repository
        .find_account_by_id(account_one.id)
        .await
        .expect("account lookup")
        .expect("account present");
    assert_eq!(account.id, account_one.id);

    let agent = repository
        .find_agent_by_id(agent_one.id)
        .await
        .expect("agent lookup")
        .expect("agent present");
    assert_eq!(agent.id, agent_one.id);

    let notifications = repository
        .list_notifications_for_bug(bug_one.id)
        .await
        .expect("notifications");
    assert_eq!(notifications.len(), 1);
    assert_eq!(notifications[0].message, "Acme bug notification");
}
