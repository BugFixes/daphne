use async_trait::async_trait;

use crate::{AppResult, domain::NotificationProvider};

use super::{NotificationProviderClient, NotificationRequest};

pub struct SlackProvider;

#[async_trait]
impl NotificationProviderClient for SlackProvider {
    fn kind(&self) -> NotificationProvider {
        NotificationProvider::Slack
    }

    async fn send(&self, request: NotificationRequest) -> AppResult<()> {
        tracing::info!(
            provider = %self.kind(),
            account_id = %request.account.id,
            bug_id = %request.bug.id,
            severity = %request.severity,
            "sent stub notification: {}",
            request.message
        );
        Ok(())
    }
}
