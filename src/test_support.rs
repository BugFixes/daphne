use std::{collections::HashSet, sync::Arc, time::Duration};

use axum::Router;
use sqlx::{PgPool, postgres::PgPoolOptions};
use testcontainers::runners::AsyncRunner;
use testcontainers_modules::postgres::Postgres;
use tokio::sync::OnceCell;

use crate::{
    ai::AiRegistry,
    api,
    config::Config,
    feature_flags::build_feature_flags,
    notifications::NotificationRegistry,
    policy::build_policy_engine,
    repository::Repository,
    service::{IntakeService, IntakeServiceSettings},
    ticketing::TicketingRegistry,
};

static TEST_DATABASE_URL: OnceCell<String> = OnceCell::const_new();

pub(crate) async fn test_config_with_disabled_features(
    disabled_features: HashSet<String>,
) -> Config {
    Config {
        http_port: "0".to_string(),
        database_url: test_database_url().await.to_string(),
        policy2_engine_url: "https://api.policy2.net/run".to_string(),
        notification_cooldown_minutes: 0,
        log_retention_days: 30,
        flagsgg_project_id: None,
        flagsgg_agent_id: None,
        flagsgg_environment_id: None,
        disabled_features,
    }
}

pub(crate) async fn build_test_app() -> (Router, Arc<Repository>) {
    let config = test_config_with_disabled_features(HashSet::new()).await;
    let repository = Arc::new(Repository::connect(&config).await.expect("repository"));
    let ticketing = Arc::new(TicketingRegistry::default());
    let notifications = Arc::new(NotificationRegistry::default());
    let ai = Arc::new(AiRegistry::default());
    let feature_flags = build_feature_flags(&config).expect("feature flags");
    let policy_engine = build_policy_engine(&config).expect("policy engine");
    let intake_service = Arc::new(IntakeService::new(
        repository.clone(),
        ticketing,
        notifications,
        ai,
        feature_flags,
        policy_engine,
        IntakeServiceSettings {
            notification_cooldown_minutes: 0,
            log_retention_days: 30,
        },
    ));
    let app = api::router(repository.clone(), intake_service);
    (app, repository)
}

pub(crate) async fn reset_database() {
    let pool: PgPool = PgPoolOptions::new()
        .max_connections(1)
        .connect(test_database_url().await)
        .await
        .expect("test database pool");

    sqlx::query(
        "TRUNCATE TABLE api_keys, log_archives, logs, ticket_events, notification_events, account_provider_configs, ticket_comments, tickets, notifications, occurrences, bugs, environments, subprojects, projects, agents, memberships, users, accounts, organizations RESTART IDENTITY CASCADE",
    )
    .execute(&pool)
    .await
    .expect("truncate tables");

    pool.close().await;
}

async fn test_database_url() -> &'static str {
    TEST_DATABASE_URL
        .get_or_init(|| async {
            let container = Box::new(
                tokio::time::timeout(Duration::from_secs(120), Postgres::default().start())
                    .await
                    .expect("start postgres testcontainer timed out")
                    .expect("start postgres testcontainer"),
            );
            let host = container
                .get_host()
                .await
                .expect("postgres host")
                .to_string();
            let port = container
                .get_host_port_ipv4(5432)
                .await
                .expect("postgres host port");

            // Leak the container for the duration of the test process so the shared
            // database stays alive across serial DB-backed tests.
            let _ = Box::leak(container);

            format!("postgres://postgres:postgres@{host}:{port}/postgres")
        })
        .await
        .as_str()
}
