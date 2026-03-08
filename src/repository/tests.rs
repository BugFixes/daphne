use std::env;

use uuid::Uuid;

use crate::config::Config;

use super::Repository;

#[tokio::test]
async fn enables_foreign_keys_for_every_sqlite_pool_connection() {
    let database_path = env::temp_dir().join(format!("bugfixes-repository-{}.db", Uuid::new_v4()));
    let database_url = format!("sqlite://{}", database_path.display());
    let config = Config {
        bind_address: "127.0.0.1:0".to_string(),
        database_url,
        feature_flags_provider: "local".to_string(),
        flagsgg_project_id: None,
        flagsgg_agent_id: None,
        flagsgg_environment_id: None,
        disabled_features: Default::default(),
    };

    let repository = Repository::connect(&config).await.expect("repository");
    let mut connections = Vec::new();

    for _ in 0..3 {
        connections.push(repository.pool.acquire().await.expect("pool connection"));
    }

    for connection in &mut connections {
        let enabled: i64 = sqlx::query_scalar("PRAGMA foreign_keys")
            .fetch_one(&mut **connection)
            .await
            .expect("foreign_keys pragma");
        assert_eq!(enabled, 1);
    }

    drop(connections);
    drop(repository);
    let _ = std::fs::remove_file(database_path);
}
