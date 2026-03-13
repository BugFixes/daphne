use std::{collections::HashSet, sync::Arc};

use async_trait::async_trait;

use crate::{AppError, AppResult, config::Config};

#[async_trait]
pub trait FeatureFlagsClient: Send + Sync {
    async fn is_enabled(&self, key: &str) -> AppResult<bool>;
}

pub struct LocalFeatureFlags {
    disabled: HashSet<String>,
}

impl LocalFeatureFlags {
    pub fn new(disabled: HashSet<String>) -> Self {
        Self { disabled }
    }
}

#[async_trait]
impl FeatureFlagsClient for LocalFeatureFlags {
    async fn is_enabled(&self, key: &str) -> AppResult<bool> {
        Ok(!is_disabled(&self.disabled, key))
    }
}

#[cfg(feature = "flagsgg")]
pub struct FlagsGgFeatureFlags {
    client: flags_rs::Client,
    disabled: HashSet<String>,
}

#[cfg(feature = "flagsgg")]
impl FlagsGgFeatureFlags {
    pub fn from_config(config: &Config) -> AppResult<Self> {
        let project_id = config.flagsgg_project_id.clone().ok_or_else(|| {
            AppError::Validation("BUGFIXES_FLAGSGG_PROJECT_ID is required".to_string())
        })?;
        let agent_id = config.flagsgg_agent_id.clone().ok_or_else(|| {
            AppError::Validation("BUGFIXES_FLAGSGG_AGENT_ID is required".to_string())
        })?;
        let environment_id = config.flagsgg_environment_id.clone().ok_or_else(|| {
            AppError::Validation("BUGFIXES_FLAGSGG_ENVIRONMENT_ID is required".to_string())
        })?;

        let client = flags_rs::Client::builder()
            .with_auth(flags_rs::Auth {
                project_id,
                agent_id,
                environment_id,
            })
            .with_memory_cache()
            .build()
            .map_err(|error| {
                AppError::Internal(format!("flags.gg client build failed: {error}"))
            })?;

        Ok(Self {
            client,
            disabled: config.disabled_features.clone(),
        })
    }
}

#[cfg(feature = "flagsgg")]
#[async_trait]
impl FeatureFlagsClient for FlagsGgFeatureFlags {
    async fn is_enabled(&self, key: &str) -> AppResult<bool> {
        if is_disabled(&self.disabled, key) {
            return Ok(false);
        }

        Ok(self.client.is(key).enabled().await)
    }
}

pub fn build_feature_flags(config: &Config) -> AppResult<Arc<dyn FeatureFlagsClient>> {
    match config.feature_flags_provider.as_str() {
        "local" => Ok(Arc::new(LocalFeatureFlags::new(
            config.disabled_features.clone(),
        ))),
        "flagsgg" => build_flagsgg_feature_flags(config),
        _ => Err(AppError::Validation(
            "BUGFIXES_FEATURE_FLAGS_PROVIDER must be one of: local, flagsgg".to_string(),
        )),
    }
}

#[cfg(feature = "flagsgg")]
fn build_flagsgg_feature_flags(config: &Config) -> AppResult<Arc<dyn FeatureFlagsClient>> {
    Ok(Arc::new(FlagsGgFeatureFlags::from_config(config)?))
}

#[cfg(not(feature = "flagsgg"))]
fn build_flagsgg_feature_flags(_config: &Config) -> AppResult<Arc<dyn FeatureFlagsClient>> {
    Err(AppError::Validation(
        "BUGFIXES_FEATURE_FLAGS_PROVIDER=flagsgg requires the `flagsgg` cargo feature".to_string(),
    ))
}

fn is_disabled(disabled: &HashSet<String>, key: &str) -> bool {
    if disabled.contains(key) {
        return true;
    }

    let short_key = key.rsplit('/').next().unwrap_or(key);
    if disabled.contains(short_key) {
        return true;
    }

    if key.contains('/') {
        return false;
    }

    ["ticketing", "notifications", "ai"]
        .iter()
        .map(|prefix| format!("{prefix}/{key}"))
        .any(|legacy_key| disabled.contains(&legacy_key))
}
