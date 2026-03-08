use std::env;

use crate::{AppError, AppResult};

#[derive(Debug, Clone)]
pub struct Config {
    pub bind_address: String,
    pub database_url: String,
}

impl Config {
    pub fn from_env() -> AppResult<Self> {
        let bind_address =
            env::var("BUGFIXES_BIND_ADDRESS").unwrap_or_else(|_| "127.0.0.1:3000".to_string());
        let database_url = env::var("BUGFIXES_DATABASE_URL")
            .unwrap_or_else(|_| "sqlite://bugfixes.db".to_string());

        if bind_address.trim().is_empty() {
            return Err(AppError::Validation(
                "BUGFIXES_BIND_ADDRESS cannot be empty".to_string(),
            ));
        }
        if database_url.trim().is_empty() {
            return Err(AppError::Validation(
                "BUGFIXES_DATABASE_URL cannot be empty".to_string(),
            ));
        }

        Ok(Self {
            bind_address,
            database_url,
        })
    }
}
