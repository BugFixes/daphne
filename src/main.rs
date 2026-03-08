use std::sync::Arc;

use bugfixes::{
    api, config::Config, providers::ProviderRegistry, repository::Repository,
    service::IntakeService,
};

#[tokio::main]
async fn main() -> bugfixes::AppResult<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let config = Config::from_env()?;
    let repository = Arc::new(Repository::connect(&config).await?);
    let providers = Arc::new(ProviderRegistry::default());
    let intake_service = Arc::new(IntakeService::new(repository.clone(), providers));

    let app = api::router(repository, intake_service);
    let listener = tokio::net::TcpListener::bind(&config.bind_address).await?;

    tracing::info!("bugfix.es listening on {}", config.bind_address);

    axum::serve(listener, app).await?;
    Ok(())
}
