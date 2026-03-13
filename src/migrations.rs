use refinery::embed_migrations;
use rusqlite::Connection;

use crate::{AppError, AppResult};

mod embedded {
    use super::embed_migrations;

    embed_migrations!("migrations");
}

pub fn run(database_url: &str) -> AppResult<()> {
    let mut connection = open_connection(database_url)?;
    let report = embedded::migrations::runner()
        .run(&mut connection)
        .map_err(|error| AppError::Internal(format!("failed to run migrations: {error}")))?;

    if report.applied_migrations().is_empty() {
        tracing::info!("database migrations already up to date");
    } else {
        tracing::info!(
            applied = report.applied_migrations().len(),
            "applied database migrations"
        );
    }

    Ok(())
}

fn open_connection(database_url: &str) -> AppResult<Connection> {
    match sqlite_target(database_url)? {
        SqliteTarget::InMemory => {
            let connection = Connection::open_in_memory().map_err(map_rusqlite_error)?;
            configure_sqlite_connection(connection)
        }
        SqliteTarget::File(path) => {
            let connection = Connection::open(path).map_err(map_rusqlite_error)?;
            configure_sqlite_connection(connection)
        }
    }
}

fn sqlite_target(database_url: &str) -> AppResult<SqliteTarget> {
    if database_url == "sqlite::memory:" {
        return Ok(SqliteTarget::InMemory);
    }

    let path = database_url.strip_prefix("sqlite://").ok_or_else(|| {
        AppError::Validation(format!("unsupported sqlite database url: {database_url}"))
    })?;
    let path = path.split('?').next().unwrap_or(path);

    if path.trim().is_empty() {
        return Err(AppError::Validation(
            "sqlite database url must include a file path".to_string(),
        ));
    }

    Ok(SqliteTarget::File(path.to_string()))
}

fn map_rusqlite_error(error: rusqlite::Error) -> AppError {
    AppError::Internal(format!(
        "failed to open sqlite database for migrations: {error}"
    ))
}

fn configure_sqlite_connection(connection: Connection) -> AppResult<Connection> {
    connection
        .pragma_update(None, "foreign_keys", "ON")
        .map_err(map_rusqlite_error)?;
    Ok(connection)
}

enum SqliteTarget {
    InMemory,
    File(String),
}
