use async_trait::async_trait;

use crate::{AppResult, domain::TicketProvider};

use super::{
    RemoteTicket, TicketCommentRequest, TicketCreateRequest, TicketPriorityRequest,
    TicketingProviderClient,
};

pub struct JiraProvider;

#[async_trait]
impl TicketingProviderClient for JiraProvider {
    fn kind(&self) -> TicketProvider {
        TicketProvider::Jira
    }

    async fn create_ticket(&self, request: TicketCreateRequest) -> AppResult<RemoteTicket> {
        let remote_id = format!("JIRA-{}", request.bug.id.simple());
        let remote_url = format!("https://stub.{}/issues/{remote_id}", self.kind());

        tracing::info!(
            provider = %self.kind(),
            bug_id = %request.bug.id,
            priority = %request.priority,
            "created stub remote ticket"
        );

        Ok(RemoteTicket {
            remote_id,
            remote_url,
            status: "open".to_string(),
        })
    }

    async fn add_comment(&self, request: TicketCommentRequest) -> AppResult<()> {
        tracing::info!(
            provider = %self.kind(),
            ticket_id = %request.ticket.id,
            remote_id = %request.ticket.remote_id,
            comment = %request.comment,
            "added stub ticket comment"
        );
        Ok(())
    }

    async fn update_priority(&self, request: TicketPriorityRequest) -> AppResult<()> {
        tracing::info!(
            provider = %self.kind(),
            ticket_id = %request.ticket.id,
            remote_id = %request.ticket.remote_id,
            priority = %request.priority,
            "updated stub ticket priority"
        );
        Ok(())
    }
}
