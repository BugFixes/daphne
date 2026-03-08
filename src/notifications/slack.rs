use async_trait::async_trait;

use crate::{AppResult, domain::NotificationProvider};

use super::{NotificationProviderClient, NotificationRequest, log_stub_notification};

pub struct SlackProvider;

#[async_trait]
impl NotificationProviderClient for SlackProvider {
    fn kind(&self) -> NotificationProvider {
        NotificationProvider::Slack
    }

    async fn send(&self, request: NotificationRequest) -> AppResult<()> {
        log_stub_notification(self.kind(), &request);
        Ok(())
    }
}
