use std::{collections::HashSet, env};

use crate::{AppError, AppResult};

#[derive(Debug, Clone)]
pub struct Config {
    pub bind_address: String,
    pub database_url: String,
    pub feature_flags_provider: String,
    pub policy_provider: String,
    pub policy2_engine_url: String,
    pub notification_cooldown_minutes: i64,
    pub log_retention_days: i64,
    pub flagsgg_project_id: Option<String>,
    pub flagsgg_agent_id: Option<String>,
    pub flagsgg_environment_id: Option<String>,
    pub disabled_features: HashSet<String>,
}

impl Config {
    pub fn from_env() -> AppResult<Self> {
        let _ = dotenvy::dotenv();
        let bind_address =
            env::var("BUGFIXES_BIND_ADDRESS").unwrap_or_else(|_| "127.0.0.1:3000".to_string());
        let database_url = env::var("BUGFIXES_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://postgres:postgres@127.0.0.1:5432/bugfixes".to_string());
        let feature_flags_provider =
            env::var("BUGFIXES_FEATURE_FLAGS_PROVIDER").unwrap_or_else(|_| "local".to_string());
        let policy_provider =
            env::var("BUGFIXES_POLICY_PROVIDER").unwrap_or_else(|_| "local".to_string());
        let policy2_engine_url = env::var("BUGFIXES_POLICY2_ENGINE_URL")
            .unwrap_or_else(|_| "https://api.policy2.net/run".to_string());
        let notification_cooldown_minutes =
            parse_i64_env("BUGFIXES_NOTIFICATION_COOLDOWN_MINUTES", 0)?;
        let log_retention_days = parse_i64_env("BUGFIXES_LOG_RETENTION_DAYS", 30)?;
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
        if !matches!(policy_provider.as_str(), "local" | "policy2") {
            return Err(AppError::Validation(
                "BUGFIXES_POLICY_PROVIDER must be one of: local, policy2".to_string(),
            ));
        }
        if policy2_engine_url.trim().is_empty() {
            return Err(AppError::Validation(
                "BUGFIXES_POLICY2_ENGINE_URL cannot be empty".to_string(),
            ));
        }
        if notification_cooldown_minutes < 0 {
            return Err(AppError::Validation(
                "BUGFIXES_NOTIFICATION_COOLDOWN_MINUTES must be zero or greater".to_string(),
            ));
        }
        if log_retention_days <= 0 {
            return Err(AppError::Validation(
                "BUGFIXES_LOG_RETENTION_DAYS must be greater than zero".to_string(),
            ));
        }

        Ok(Self {
            bind_address,
            database_url,
            feature_flags_provider,
            policy_provider,
            policy2_engine_url,
            notification_cooldown_minutes,
            log_retention_days,
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

fn parse_i64_env(key: &str, default: i64) -> AppResult<i64> {
    match env::var(key) {
        Ok(value) => value
            .trim()
            .parse::<i64>()
            .map_err(|_| AppError::Validation(format!("{key} must be a valid integer value"))),
        Err(_) => Ok(default),
    }
}
