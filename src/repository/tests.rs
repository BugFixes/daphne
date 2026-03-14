use std::collections::HashSet;

use serial_test::serial;

use crate::test_support::test_config_with_disabled_features;

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

    assert!(migration_count >= 2);
}
