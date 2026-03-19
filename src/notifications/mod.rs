use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::{
    AppError, AppResult,
    domain::{Account, Bug, NotificationProvider, Severity},
    logging,
};

pub mod resend;
pub mod slack;
pub mod teams;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone)]
pub struct NotificationRequest {
    pub account: Account,
    pub bug: Bug,
    pub severity: Severity,
    pub message: String,
}

#[async_trait]
pub trait NotificationProviderClient: Send + Sync {
    fn kind(&self) -> NotificationProvider;
    async fn send(&self, request: NotificationRequest) -> AppResult<()>;
}

pub use resend::ResendProvider;
pub use slack::SlackProvider;
pub use teams::TeamsProvider;

pub struct NotificationRegistry {
    providers: HashMap<NotificationProvider, Arc<dyn NotificationProviderClient>>,
}

impl NotificationRegistry {
    pub fn get(
        &self,
        kind: NotificationProvider,
    ) -> AppResult<Arc<dyn NotificationProviderClient>> {
        self.providers.get(&kind).cloned().ok_or_else(|| {
            AppError::Internal(format!("notification provider not registered: {kind}"))
        })
    }
}

impl Default for NotificationRegistry {
    fn default() -> Self {
        let slack = Arc::new(SlackProvider);
        let teams = Arc::new(TeamsProvider);
        let resend = Arc::new(ResendProvider);

        Self {
            providers: HashMap::from([
                (
                    NotificationProvider::Slack,
                    slack as Arc<dyn NotificationProviderClient>,
                ),
                (
                    NotificationProvider::Teams,
                    teams as Arc<dyn NotificationProviderClient>,
                ),
                (
                    NotificationProvider::Resend,
                    resend as Arc<dyn NotificationProviderClient>,
                ),
            ]),
        }
    }
}

pub fn build_notification_message(account: &Account, bug: &Bug, when: DateTime<Utc>) -> String {
    format!(
        "[{}] {} bug {} reached {} occurrences as of {}",
        account.name,
        bug.language,
        bug.id,
        bug.occurrence_count,
        when.to_rfc3339()
    )
}

fn log_stub_notification(provider: NotificationProvider, request: &NotificationRequest) {
    logging::info(format!(
        "sent stub notification provider={} account_id={} bug_id={} severity={} message={}",
        provider, request.account.id, request.bug.id, request.severity, request.message
    ));
}
