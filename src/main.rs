use std::sync::Arc;

use bugfixes::{
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

#[tokio::main]
async fn main() -> bugfixes::AppResult<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let config = Config::from_env()?;
    let repository = Arc::new(Repository::connect(&config).await?);
    let ticketing = Arc::new(TicketingRegistry::default());
    let notifications = Arc::new(NotificationRegistry::default());
    let ai = Arc::new(AiRegistry::default());
    let feature_flags = build_feature_flags(&config)?;
    let policy_engine = build_policy_engine(&config)?;
    let intake_service = Arc::new(IntakeService::new(
        repository.clone(),
        ticketing,
        notifications,
        ai,
        feature_flags,
        policy_engine,
        IntakeServiceSettings {
            notification_cooldown_minutes: config.notification_cooldown_minutes,
            log_retention_days: config.log_retention_days,
        },
    ));

    let app = api::router(repository, intake_service);
    let listener = tokio::net::TcpListener::bind(&config.bind_address).await?;

    tracing::info!("bugfix.es listening on {}", config.bind_address);

    axum::serve(listener, app).await?;
    Ok(())
}
