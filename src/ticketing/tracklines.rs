use async_trait::async_trait;

use crate::{AppResult, domain::TicketProvider};

use super::{
    RemoteTicket, TicketCommentRequest, TicketCreateRequest, TicketPriorityRequest,
    TicketingProviderClient, build_stub_remote_ticket, log_stub_ticket_comment,
    log_stub_ticket_creation, log_stub_ticket_priority,
};

pub struct TracklinesProvider;

#[async_trait]
impl TicketingProviderClient for TracklinesProvider {
    fn kind(&self) -> TicketProvider {
        TicketProvider::Tracklines
    }

    async fn create_ticket(&self, request: TicketCreateRequest) -> AppResult<RemoteTicket> {
        log_stub_ticket_creation(self.kind(), &request);
        Ok(build_stub_remote_ticket(self.kind(), request.bug.id))
    }

    async fn add_comment(&self, request: TicketCommentRequest) -> AppResult<()> {
        log_stub_ticket_comment(self.kind(), &request);
        Ok(())
    }

    async fn update_priority(&self, request: TicketPriorityRequest) -> AppResult<()> {
        log_stub_ticket_priority(self.kind(), &request);
        Ok(())
    }
}
