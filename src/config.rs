use std::{collections::HashSet, env};

use crate::{AppError, AppResult};

#[derive(Debug, Clone)]
pub struct Config {
    pub bind_address: String,
    pub database_url: String,
    pub feature_flags_provider: String,
    pub flagsgg_project_id: Option<String>,
    pub flagsgg_agent_id: Option<String>,
    pub flagsgg_environment_id: Option<String>,
    pub disabled_features: HashSet<String>,
}

impl Config {
    pub fn from_env() -> AppResult<Self> {
        let bind_address =
            env::var("BUGFIXES_BIND_ADDRESS").unwrap_or_else(|_| "127.0.0.1:3000".to_string());
        let database_url = env::var("BUGFIXES_DATABASE_URL")
            .unwrap_or_else(|_| "sqlite://bugfixes.db".to_string());
        let feature_flags_provider =
            env::var("BUGFIXES_FEATURE_FLAGS_PROVIDER").unwrap_or_else(|_| "local".to_string());
        let flagsgg_project_id = non_empty_env("BUGFIXES_FLAGSGG_PROJECT_ID");
        let flagsgg_agent_id = non_empty_env("BUGFIXES_FLAGSGG_AGENT_ID");
        let flagsgg_environment_id = non_empty_env("BUGFIXES_FLAGSGG_ENVIRONMENT_ID");
        let disabled_features = env::var("BUGFIXES_DISABLED_FEATURES")
            .unwrap_or_default()
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .collect();

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

        if !matches!(feature_flags_provider.as_str(), "local" | "flagsgg") {
            return Err(AppError::Validation(
                "BUGFIXES_FEATURE_FLAGS_PROVIDER must be one of: local, flagsgg".to_string(),
            ));
        }

        Ok(Self {
            bind_address,
            database_url,
            feature_flags_provider,
            flagsgg_project_id,
            flagsgg_agent_id,
            flagsgg_environment_id,
            disabled_features,
        })
    }
}

fn non_empty_env(key: &str) -> Option<String> {
    env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}
