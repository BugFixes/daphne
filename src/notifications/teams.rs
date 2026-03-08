use async_trait::async_trait;

use crate::{AppResult, domain::NotificationProvider};

use super::{NotificationProviderClient, NotificationRequest, log_stub_notification};

pub struct TeamsProvider;

#[async_trait]
impl NotificationProviderClient for TeamsProvider {
    fn kind(&self) -> NotificationProvider {
        NotificationProvider::Teams
    }

    async fn send(&self, request: NotificationRequest) -> AppResult<()> {
        log_stub_notification(self.kind(), &request);
        Ok(())
    }
}
