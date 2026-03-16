use std::collections::HashSet;

use serial_test::serial;

use crate::{
    domain::{
        AccountProviderKind, AddOrganizationMemberRequest, CreateAccountRequest,
        CreateOrganizationRequest, NotificationProvider, OrganizationRole, Severity,
        TicketProvider, UpdateOrganizationMembershipRequest,
    },
    test_support::{reset_database, test_config_with_disabled_features},
};

use super::Repository;

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
