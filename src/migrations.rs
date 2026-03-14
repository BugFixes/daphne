use refinery::embed_migrations;
use tokio_postgres::NoTls;

use crate::{AppError, AppResult};

mod embedded {
    use super::embed_migrations;

    embed_migrations!("migrations");
}

pub async fn run(database_url: &str) -> AppResult<()> {
    if is_postgres_url(database_url) {
        let (mut client, connection) =
            tokio_postgres::connect(database_url, NoTls)
                .await
                .map_err(|error| {
                    AppError::Internal(format!(
                        "failed to connect to postgres for migrations: {error}"
                    ))
                })?;

        tokio::spawn(async move {
            if let Err(error) = connection.await {
                tracing::error!(%error, "postgres migration connection error");
            }
        });

        let report = embedded::migrations::runner()
            .run_async(&mut client)
            .await
            .map_err(|error| AppError::Internal(format!("failed to run migrations: {error}")))?;
        log_report(&report);
        return Ok(());
    }

    Err(AppError::Validation(format!(
        "unsupported database url for migrations: {database_url}"
    )))
}

fn is_postgres_url(database_url: &str) -> bool {
    database_url.starts_with("postgres://") || database_url.starts_with("postgresql://")
}

fn log_report(report: &refinery::Report) {
    if report.applied_migrations().is_empty() {
        tracing::info!("database migrations already up to date");
    } else {
        tracing::info!(
            applied = report.applied_migrations().len(),
            "applied database migrations"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::is_postgres_url;

    #[test]
    fn recognizes_postgres_urls() {
        assert!(is_postgres_url(
            "postgres://bugfixes:secret@localhost:5432/bugfixes"
        ));
        assert!(is_postgres_url(
            "postgresql://bugfixes:secret@localhost:5432/bugfixes"
        ));
    }
}
