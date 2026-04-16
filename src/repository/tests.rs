use std::collections::{HashMap, HashSet};

use chrono::Utc;
use serial_test::serial;

use crate::{
    domain::{
        AccountProviderKind, AddOrganizationMemberRequest, ApiKeyScope, ApiKeyType,
        CreateAccountRequest, CreateAgentRequest, CreateApiKeyRequest, CreateEnvironmentRequest,
        CreateOrganizationRequest, CreateProjectRequest, CreateSubprojectRequest,
        NotificationProvider, OrganizationRole, Severity, TicketAction, TicketProvider,
        UpdateOrganizationMembershipRequest,
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
async fn create_organization_reuses_clerk_org_and_adds_missing_owner_membership() {
    let config = test_config_with_disabled_features(HashSet::new()).await;
    let repository = Repository::connect(&config).await.expect("repository");
    reset_database().await;

    let first = repository
        .create_organization(CreateOrganizationRequest {
            name: "Acme".to_string(),
            clerk_org_id: Some("org_shared_acme".to_string()),
            owner_clerk_user_id: "user_owner_one".to_string(),
            owner_name: "Owner One".to_string(),
        })
        .await
        .expect("first organization");

    let second = repository
        .create_organization(CreateOrganizationRequest {
            name: "Acme Duplicate".to_string(),
            clerk_org_id: Some("org_shared_acme".to_string()),
            owner_clerk_user_id: "user_owner_two".to_string(),
            owner_name: "Owner Two".to_string(),
        })
        .await
        .expect("second organization");

    assert_eq!(first.organization.id, second.organization.id);

    let first_user_orgs = repository
        .list_organizations_for_user("user_owner_one")
        .await
        .expect("first user orgs");
    let second_user_orgs = repository
        .list_organizations_for_user("user_owner_two")
        .await
        .expect("second user orgs");

    assert_eq!(first_user_orgs.len(), 1);
    assert_eq!(second_user_orgs.len(), 1);
    assert_eq!(first_user_orgs[0].organization.id, second.organization.id);
    assert_eq!(second_user_orgs[0].organization.id, second.organization.id);
    assert_eq!(second_user_orgs[0].membership.role, OrganizationRole::Owner);
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

#[tokio::test]
#[serial]
async fn creates_and_lists_api_keys() {
    let config = test_config_with_disabled_features(HashSet::new()).await;
    let repository = Repository::connect(&config).await.expect("repository");
    reset_database().await;

    let org = repository
        .create_organization(CreateOrganizationRequest {
            name: "KeyOrg".to_string(),
            clerk_org_id: Some("org_key_test".to_string()),
            owner_clerk_user_id: "user_key_owner".to_string(),
            owner_name: "Owner".to_string(),
        })
        .await
        .expect("org");

    let account = repository
        .create_account(CreateAccountRequest {
            organization_id: Some(org.organization.id),
            name: "KeyAccount".to_string(),
            create_tickets: false,
            ticket_provider: TicketProvider::None,
            ticketing_api_key: None,
            notification_provider: NotificationProvider::None,
            notification_api_key: None,
            ai_enabled: false,
            use_managed_ai: false,
            ai_api_key: None,
            notify_min_level: Severity::Error,
            rapid_occurrence_window_minutes: 30,
            rapid_occurrence_threshold: 3,
        })
        .await
        .expect("account");

    // Create a dev key
    let dev_key = repository
        .create_api_key(
            org.organization.id,
            Some("user_key_owner"),
            CreateApiKeyRequest {
                name: "My Dev Key".to_string(),
                key_type: ApiKeyType::Dev,
                scope: None,
                account_id: Some(account.id),
                environment: None,
                expires_at: None,
            },
        )
        .await
        .expect("dev key");
    assert_eq!(dev_key.api_key.key_type, ApiKeyType::Dev);
    assert_eq!(dev_key.api_key.scope, ApiKeyScope::Ingest);
    assert!(!dev_key.api_secret.is_empty());

    // Create a system key
    let system_key = repository
        .create_api_key(
            org.organization.id,
            None,
            CreateApiKeyRequest {
                name: "Prod Ingest".to_string(),
                key_type: ApiKeyType::System,
                scope: Some(ApiKeyScope::Ingest),
                account_id: Some(account.id),
                environment: Some("production".to_string()),
                expires_at: None,
            },
        )
        .await
        .expect("system key");
    assert_eq!(system_key.api_key.key_type, ApiKeyType::System);
    assert_eq!(system_key.api_key.scope, ApiKeyScope::Ingest);

    // List for org returns both
    let org_keys = repository
        .list_api_keys_for_org(org.organization.id)
        .await
        .expect("org keys");
    assert_eq!(org_keys.len(), 2);

    // List for user returns only dev key
    let user_keys = repository
        .list_api_keys_for_user("user_key_owner")
        .await
        .expect("user keys");
    assert_eq!(user_keys.len(), 1);
    assert_eq!(user_keys[0].name, "My Dev Key");
}

#[tokio::test]
#[serial]
async fn revokes_api_key() {
    let config = test_config_with_disabled_features(HashSet::new()).await;
    let repository = Repository::connect(&config).await.expect("repository");
    reset_database().await;

    let org = repository
        .create_organization(CreateOrganizationRequest {
            name: "RevokeOrg".to_string(),
            clerk_org_id: Some("org_revoke_test".to_string()),
            owner_clerk_user_id: "user_revoke_owner".to_string(),
            owner_name: "Owner".to_string(),
        })
        .await
        .expect("org");

    let account = repository
        .create_account(CreateAccountRequest {
            organization_id: Some(org.organization.id),
            name: "RevokeAccount".to_string(),
            create_tickets: false,
            ticket_provider: TicketProvider::None,
            ticketing_api_key: None,
            notification_provider: NotificationProvider::None,
            notification_api_key: None,
            ai_enabled: false,
            use_managed_ai: false,
            ai_api_key: None,
            notify_min_level: Severity::Error,
            rapid_occurrence_window_minutes: 30,
            rapid_occurrence_threshold: 3,
        })
        .await
        .expect("account");

    let key = repository
        .create_api_key(
            org.organization.id,
            Some("user_revoke_owner"),
            CreateApiKeyRequest {
                name: "Revocable Key".to_string(),
                key_type: ApiKeyType::Dev,
                scope: None,
                account_id: Some(account.id),
                environment: None,
                expires_at: None,
            },
        )
        .await
        .expect("key");

    let revoked = repository
        .revoke_api_key(key.api_key.id, org.organization.id)
        .await
        .expect("revoke");
    assert!(revoked.revoked_at.is_some());

    // Should not appear in list
    let keys = repository
        .list_api_keys_for_org(org.organization.id)
        .await
        .expect("keys");
    assert!(keys.is_empty());
}

#[tokio::test]
#[serial]
async fn finds_api_key_by_credentials() {
    let config = test_config_with_disabled_features(HashSet::new()).await;
    let repository = Repository::connect(&config).await.expect("repository");
    reset_database().await;

    let org = repository
        .create_organization(CreateOrganizationRequest {
            name: "CredOrg".to_string(),
            clerk_org_id: Some("org_cred_test".to_string()),
            owner_clerk_user_id: "user_cred_owner".to_string(),
            owner_name: "Owner".to_string(),
        })
        .await
        .expect("org");

    let account = repository
        .create_account(CreateAccountRequest {
            organization_id: Some(org.organization.id),
            name: "CredAccount".to_string(),
            create_tickets: false,
            ticket_provider: TicketProvider::None,
            ticketing_api_key: None,
            notification_provider: NotificationProvider::None,
            notification_api_key: None,
            ai_enabled: false,
            use_managed_ai: false,
            ai_api_key: None,
            notify_min_level: Severity::Error,
            rapid_occurrence_window_minutes: 30,
            rapid_occurrence_threshold: 3,
        })
        .await
        .expect("account");

    let key = repository
        .create_api_key(
            org.organization.id,
            Some("user_cred_owner"),
            CreateApiKeyRequest {
                name: "Ingest Key".to_string(),
                key_type: ApiKeyType::Dev,
                scope: None,
                account_id: Some(account.id),
                environment: None,
                expires_at: None,
            },
        )
        .await
        .expect("key");

    // Valid credentials should find the key
    let found = repository
        .find_api_key_by_credentials(&key.api_key.api_key, &key.api_secret)
        .await
        .expect("found key");
    assert_eq!(found.id, key.api_key.id);

    // Wrong secret should not find the key
    let not_found = repository
        .find_api_key_by_credentials(&key.api_key.api_key, "wrong_secret")
        .await;
    assert!(not_found.is_err());
}

#[tokio::test]
#[serial]
async fn creates_project_hierarchy_and_environment_account() {
    let config = test_config_with_disabled_features(HashSet::new()).await;
    let repository = Repository::connect(&config).await.expect("repository");
    reset_database().await;

    let org = repository
        .create_organization(CreateOrganizationRequest {
            name: "Bugfixes".to_string(),
            clerk_org_id: Some("org_hierarchy".to_string()),
            owner_clerk_user_id: "user_hierarchy_owner".to_string(),
            owner_name: "Owner".to_string(),
        })
        .await
        .expect("org");

    let project = repository
        .create_project(
            org.organization.id,
            CreateProjectRequest {
                name: "bugfixes".to_string(),
            },
        )
        .await
        .expect("project");
    let subproject = repository
        .create_subproject(
            org.organization.id,
            project.id,
            CreateSubprojectRequest {
                name: "daphne".to_string(),
            },
        )
        .await
        .expect("subproject");
    let provisioning = repository
        .create_environment(
            org.organization.id,
            subproject.id,
            CreateEnvironmentRequest {
                name: "production".to_string(),
                create_tickets: false,
                ticket_provider: TicketProvider::None,
                ticketing_api_key: None,
                notification_provider: NotificationProvider::None,
                notification_api_key: None,
                ai_enabled: false,
                use_managed_ai: false,
                ai_api_key: None,
                notify_min_level: Severity::Error,
                rapid_occurrence_window_minutes: 30,
                rapid_occurrence_threshold: 3,
            },
        )
        .await
        .expect("environment");

    assert_eq!(provisioning.environment.subproject_id, subproject.id);
    assert_eq!(provisioning.environment.name, "production");
    assert_eq!(provisioning.account.organization_id, org.organization.id);
    assert_eq!(provisioning.account.name, "bugfixes / daphne / production");

    let environment = repository
        .find_environment_by_account_id(provisioning.account.id)
        .await
        .expect("environment lookup")
        .expect("environment");
    assert_eq!(environment.id, provisioning.environment.id);
}

#[tokio::test]
#[serial]
async fn single_plan_limits_projects_accounts_and_agents() {
    let config = test_config_with_disabled_features(HashSet::new()).await;
    let repository = Repository::connect(&config).await.expect("repository");
    reset_database().await;

    let org = repository
        .create_organization(CreateOrganizationRequest {
            name: "Single Tier".to_string(),
            clerk_org_id: Some("org_single_tier".to_string()),
            owner_clerk_user_id: "user_single_owner".to_string(),
            owner_name: "Owner".to_string(),
        })
        .await
        .expect("org");

    let project = repository
        .create_project(
            org.organization.id,
            CreateProjectRequest {
                name: "bugfixes".to_string(),
            },
        )
        .await
        .expect("project");
    let duplicate_project = repository
        .create_project(
            org.organization.id,
            CreateProjectRequest {
                name: "dashboard".to_string(),
            },
        )
        .await;
    assert!(matches!(
        duplicate_project,
        Err(crate::AppError::Validation(_))
    ));

    let subproject = repository
        .create_subproject(
            org.organization.id,
            project.id,
            CreateSubprojectRequest {
                name: "daphne".to_string(),
            },
        )
        .await
        .expect("subproject");
    let duplicate_subproject = repository
        .create_subproject(
            org.organization.id,
            project.id,
            CreateSubprojectRequest {
                name: "dashboard".to_string(),
            },
        )
        .await;
    assert!(matches!(
        duplicate_subproject,
        Err(crate::AppError::Validation(_))
    ));

    let provisioning = repository
        .create_environment(
            org.organization.id,
            subproject.id,
            CreateEnvironmentRequest {
                name: "production".to_string(),
                create_tickets: false,
                ticket_provider: TicketProvider::None,
                ticketing_api_key: None,
                notification_provider: NotificationProvider::None,
                notification_api_key: None,
                ai_enabled: false,
                use_managed_ai: false,
                ai_api_key: None,
                notify_min_level: Severity::Error,
                rapid_occurrence_window_minutes: 30,
                rapid_occurrence_threshold: 3,
            },
        )
        .await
        .expect("environment");

    let duplicate_environment = repository
        .create_environment(
            org.organization.id,
            subproject.id,
            CreateEnvironmentRequest {
                name: "staging".to_string(),
                create_tickets: false,
                ticket_provider: TicketProvider::None,
                ticketing_api_key: None,
                notification_provider: NotificationProvider::None,
                notification_api_key: None,
                ai_enabled: false,
                use_managed_ai: false,
                ai_api_key: None,
                notify_min_level: Severity::Error,
                rapid_occurrence_window_minutes: 30,
                rapid_occurrence_threshold: 3,
            },
        )
        .await;
    assert!(matches!(
        duplicate_environment,
        Err(crate::AppError::Validation(_))
    ));

    let duplicate_account = repository
        .create_account(CreateAccountRequest {
            organization_id: Some(org.organization.id),
            name: "Second Account".to_string(),
            create_tickets: false,
            ticket_provider: TicketProvider::None,
            ticketing_api_key: None,
            notification_provider: NotificationProvider::None,
            notification_api_key: None,
            ai_enabled: false,
            use_managed_ai: false,
            ai_api_key: None,
            notify_min_level: Severity::Error,
            rapid_occurrence_window_minutes: 30,
            rapid_occurrence_threshold: 3,
        })
        .await;
    assert!(matches!(
        duplicate_account,
        Err(crate::AppError::Validation(_))
    ));

    repository
        .create_agent(CreateAgentRequest {
            account_id: provisioning.account.id,
            name: "primary-agent".to_string(),
        })
        .await
        .expect("agent");
    let duplicate_agent = repository
        .create_agent(CreateAgentRequest {
            account_id: provisioning.account.id,
            name: "secondary-agent".to_string(),
        })
        .await;
    assert!(matches!(
        duplicate_agent,
        Err(crate::AppError::Validation(_))
    ));
}
