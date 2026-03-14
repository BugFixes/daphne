use std::{collections::HashSet, time::Duration};

use sqlx::{PgPool, postgres::PgPoolOptions};
use testcontainers::runners::AsyncRunner;
use testcontainers_modules::postgres::Postgres;
use tokio::sync::OnceCell;

use crate::config::Config;

static TEST_DATABASE_URL: OnceCell<String> = OnceCell::const_new();

pub(crate) async fn test_config_with_disabled_features(
    disabled_features: HashSet<String>,
) -> Config {
    Config {
        bind_address: "127.0.0.1:0".to_string(),
        database_url: test_database_url().await.to_string(),
        feature_flags_provider: "local".to_string(),
        policy_provider: "local".to_string(),
        policy2_engine_url: "https://api.policy2.net/run".to_string(),
        flagsgg_project_id: None,
        flagsgg_agent_id: None,
        flagsgg_environment_id: None,
        disabled_features,
    }
}

pub(crate) async fn reset_database() {
    let pool: PgPool = PgPoolOptions::new()
        .max_connections(1)
        .connect(test_database_url().await)
        .await
        .expect("test database pool");

    sqlx::query(
        "TRUNCATE TABLE ticket_comments, tickets, notifications, occurrences, bugs, agents, accounts RESTART IDENTITY CASCADE",
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
