use std::sync::Arc;

use bugfixes::{
    ai::AiRegistry, api, config::Config, notifications::NotificationRegistry,
    repository::Repository, service::IntakeService, ticketing::TicketingRegistry,
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
    let intake_service = Arc::new(IntakeService::new(
        repository.clone(),
        ticketing,
        notifications,
        ai,
    ));

    let app = api::router(repository, intake_service);
    let listener = tokio::net::TcpListener::bind(&config.bind_address).await?;

    tracing::info!("bugfix.es listening on {}", config.bind_address);

    axum::serve(listener, app).await?;
    Ok(())
}
