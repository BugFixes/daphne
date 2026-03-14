use serial_test::serial;

use crate::config::Config;

use super::Repository;

#[tokio::test]
#[serial]
async fn connects_to_postgres_after_running_migrations() {
    let config = Config::from_env().expect("config");
    let repository = Repository::connect(&config).await.expect("repository");
    let migration_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM refinery_schema_history")
            .fetch_one(&repository.pool)
            .await
            .expect("migration count");

    assert!(migration_count >= 2);
}
