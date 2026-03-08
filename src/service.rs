use std::{sync::Arc, time::Duration};

use chrono::{DateTime, Utc};
use regex::Regex;
use sha2::{Digest, Sha256};

use crate::{
    AppResult,
    domain::{
        Account, Bug, NotificationOutcome, Severity, StacktraceEventRequest, Ticket, TicketAction,
        TicketPriority,
    },
    providers::{
        NotificationRequest, ProviderRegistry, TicketCommentRequest, TicketCreateRequest,
        TicketPriorityRequest, build_escalation_comment, build_notification_message,
        build_repeat_comment,
    },
    repository::Repository,
};

#[derive(Clone)]
pub struct IntakeService {
    repository: Arc<Repository>,
    providers: Arc<ProviderRegistry>,
}

impl IntakeService {
    pub fn new(repository: Arc<Repository>, providers: Arc<ProviderRegistry>) -> Self {
        Self {
            repository,
            providers,
        }
    }

    pub async fn ingest(
        &self,
        request: StacktraceEventRequest,
    ) -> AppResult<crate::domain::IntakeOutcome> {
        request.validate()?;

        let agent = match request.agent_secret.as_deref() {
            Some(agent_secret) => {
                self.repository
                    .find_agent_by_credentials(&request.agent_key, agent_secret)
                    .await?
            }
            None => {
                self.repository
                    .find_agent_by_key(&request.agent_key)
                    .await?
            }
        };
        let account = self.repository.find_account(agent.account_id).await?;
        let occurred_at = request.occurred_at.unwrap_or_else(Utc::now);
        let normalized_stacktrace = normalize_stacktrace(&request.stacktrace);
        let stacktrace_hash = hash_stacktrace(&normalized_stacktrace);

        if let Some(existing_bug) = self
            .repository
            .find_bug_by_hash(account.id, &stacktrace_hash)
            .await?
        {
            self.handle_repeat_bug(
                account,
                existing_bug,
                request,
                normalized_stacktrace,
                stacktrace_hash,
                occurred_at,
            )
            .await
        } else {
            self.handle_new_bug(
                account,
                agent.id,
                request,
                normalized_stacktrace,
                stacktrace_hash,
                occurred_at,
            )
            .await
        }
    }

    async fn handle_new_bug(
        &self,
        account: Account,
        agent_id: uuid::Uuid,
        request: StacktraceEventRequest,
        normalized_stacktrace: String,
        stacktrace_hash: String,
        occurred_at: DateTime<Utc>,
    ) -> AppResult<crate::domain::IntakeOutcome> {
        let bug = self
            .repository
            .create_bug(
                account.id,
                agent_id,
                &request.language,
                request.level,
                &stacktrace_hash,
                &normalized_stacktrace,
                &request.stacktrace,
                occurred_at,
            )
            .await?;
        self.repository
            .record_occurrence(bug.id, request.level, &request.stacktrace, occurred_at)
            .await?;

        let (ticket_action, ticket, ai_recommendation) = if account.create_tickets {
            let recommendation = self
                .providers
                .ai()
                .recommend_fix(&bug, &request.stacktrace)
                .await?;
            let ticket = self
                .create_ticket(
                    &account,
                    &bug,
                    request.level,
                    &recommendation,
                    &request.stacktrace,
                    occurred_at,
                )
                .await?;
            (TicketAction::Created, Some(ticket), Some(recommendation))
        } else {
            (TicketAction::Skipped, None, None)
        };

        let notification = self
            .maybe_notify(&account, &bug, request.level, ticket_action, occurred_at)
            .await?;

        Ok(crate::domain::IntakeOutcome {
            bug_id: bug.id,
            stacktrace_hash,
            occurrence_count: bug.occurrence_count,
            is_new_bug: true,
            ticket_action,
            ticket,
            ai_recommendation,
            notification,
        })
    }

    async fn handle_repeat_bug(
        &self,
        account: Account,
        existing_bug: Bug,
        request: StacktraceEventRequest,
        _normalized_stacktrace: String,
        stacktrace_hash: String,
        occurred_at: DateTime<Utc>,
    ) -> AppResult<crate::domain::IntakeOutcome> {
        self.repository
            .record_occurrence(
                existing_bug.id,
                request.level,
                &request.stacktrace,
                occurred_at,
            )
            .await?;
        let bug = self
            .repository
            .update_bug_on_repeat(
                &existing_bug,
                request.level,
                &request.stacktrace,
                occurred_at,
            )
            .await?;
        let recent_window = chrono::Duration::from_std(Duration::from_secs(
            (account.rapid_occurrence_window_minutes as u64) * 60,
        ))
        .expect("valid duration");
        let recent_count = self
            .repository
            .count_recent_occurrences(bug.id, occurred_at - recent_window)
            .await?;

        let mut ticket_action = TicketAction::Skipped;
        let mut ticket = self.repository.find_ticket_for_bug(bug.id).await?;
        let mut ai_recommendation = ticket.as_ref().map(|record| record.recommendation.clone());

        if ticket.is_none() && account.create_tickets {
            let recommendation = self
                .providers
                .ai()
                .recommend_fix(&bug, &request.stacktrace)
                .await?;
            let created_ticket = self
                .create_ticket(
                    &account,
                    &bug,
                    request.level,
                    &recommendation,
                    &request.stacktrace,
                    occurred_at,
                )
                .await?;
            ai_recommendation = Some(recommendation);
            ticket_action = TicketAction::Created;
            ticket = Some(created_ticket);
        } else if let Some(existing_ticket) = ticket.clone() {
            if recent_count >= account.rapid_occurrence_threshold {
                let next_priority = existing_ticket.priority.escalated();
                if next_priority != existing_ticket.priority {
                    let provider = self.providers.ticketing(existing_ticket.provider)?;
                    provider
                        .update_priority(TicketPriorityRequest {
                            ticket: existing_ticket.clone(),
                            priority: next_priority,
                        })
                        .await?;
                    self.repository
                        .update_ticket_priority(existing_ticket.id, next_priority, occurred_at)
                        .await?;
                    let comment = build_escalation_comment(
                        recent_count,
                        account.rapid_occurrence_window_minutes,
                    );
                    provider
                        .add_comment(TicketCommentRequest {
                            ticket: Ticket {
                                priority: next_priority,
                                updated_at: occurred_at,
                                ..existing_ticket.clone()
                            },
                            comment: comment.clone(),
                        })
                        .await?;
                    self.repository
                        .add_ticket_comment(existing_ticket.id, &comment, occurred_at)
                        .await?;
                    ticket_action = TicketAction::Escalated;
                } else {
                    let comment = build_repeat_comment(occurred_at);
                    let provider = self.providers.ticketing(existing_ticket.provider)?;
                    provider
                        .add_comment(TicketCommentRequest {
                            ticket: existing_ticket.clone(),
                            comment: comment.clone(),
                        })
                        .await?;
                    self.repository
                        .add_ticket_comment(existing_ticket.id, &comment, occurred_at)
                        .await?;
                    ticket_action = TicketAction::Commented;
                }
            } else {
                let comment = build_repeat_comment(occurred_at);
                let provider = self.providers.ticketing(existing_ticket.provider)?;
                provider
                    .add_comment(TicketCommentRequest {
                        ticket: existing_ticket.clone(),
                        comment: comment.clone(),
                    })
                    .await?;
                self.repository
                    .add_ticket_comment(existing_ticket.id, &comment, occurred_at)
                    .await?;
                ticket_action = TicketAction::Commented;
            }

            ticket = self.repository.find_ticket_for_bug(bug.id).await?;
        }

        let notification = self
            .maybe_notify(&account, &bug, request.level, ticket_action, occurred_at)
            .await?;

        Ok(crate::domain::IntakeOutcome {
            bug_id: bug.id,
            stacktrace_hash,
            occurrence_count: bug.occurrence_count,
            is_new_bug: false,
            ticket_action,
            ticket,
            ai_recommendation,
            notification,
        })
    }

    async fn create_ticket(
        &self,
        account: &Account,
        bug: &Bug,
        level: Severity,
        recommendation: &str,
        stacktrace: &str,
        occurred_at: DateTime<Utc>,
    ) -> AppResult<Ticket> {
        let priority = TicketPriority::from_severity(level);
        let remote_ticket = self
            .providers
            .ticketing(account.ticket_provider)?
            .create_ticket(TicketCreateRequest {
                bug: bug.clone(),
                account: account.clone(),
                priority,
                recommendation: recommendation.to_string(),
                source_stacktrace: stacktrace.to_string(),
            })
            .await?;

        self.repository
            .create_ticket(
                bug.id,
                account.ticket_provider,
                &remote_ticket.remote_id,
                &remote_ticket.remote_url,
                priority,
                recommendation,
                &remote_ticket.status,
                occurred_at,
            )
            .await
    }

    async fn maybe_notify(
        &self,
        account: &Account,
        bug: &Bug,
        level: Severity,
        ticket_action: TicketAction,
        occurred_at: DateTime<Utc>,
    ) -> AppResult<crate::domain::NotificationOutcome> {
        if !level.should_notify(account.notify_min_level) {
            return Ok(NotificationOutcome {
                sent: false,
                provider: None,
                message: None,
            });
        }

        if !matches!(
            ticket_action,
            TicketAction::Created | TicketAction::Escalated
        ) {
            return Ok(NotificationOutcome {
                sent: false,
                provider: None,
                message: None,
            });
        }

        let message = build_notification_message(account, bug, occurred_at);
        self.providers
            .notifications(account.notification_provider)?
            .send(NotificationRequest {
                account: account.clone(),
                bug: bug.clone(),
                severity: level,
                message: message.clone(),
            })
            .await?;
        self.repository
            .record_notification(bug.id, account.notification_provider, &message, occurred_at)
            .await?;

        Ok(NotificationOutcome {
            sent: true,
            provider: Some(account.notification_provider),
            message: Some(message),
        })
    }
}

fn normalize_stacktrace(input: &str) -> String {
    let address_pattern = Regex::new(r"0x[0-9a-fA-F]+").expect("valid address regex");
    input
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| address_pattern.replace_all(line, "0xADDR").into_owned())
        .collect::<Vec<_>>()
        .join("\n")
}

fn hash_stacktrace(normalized_stacktrace: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(normalized_stacktrace.as_bytes());
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use chrono::Utc;

    use crate::{
        config::Config,
        domain::{
            CreateAccountRequest, CreateAgentRequest, NotificationProvider, Severity,
            StacktraceEventRequest, TicketAction, TicketPriority, TicketProvider,
        },
        providers::ProviderRegistry,
        repository::Repository,
    };

    use super::IntakeService;

    async fn test_service() -> IntakeService {
        let repository = Arc::new(
            Repository::connect(&Config {
                bind_address: "127.0.0.1:0".to_string(),
                database_url: "sqlite::memory:".to_string(),
            })
            .await
            .expect("repository"),
        );
        let providers = Arc::new(ProviderRegistry::default());
        IntakeService::new(repository, providers)
    }

    #[tokio::test]
    async fn creates_ticket_and_notification_for_new_bug() {
        let service = test_service().await;
        let account = service
            .repository
            .create_account(CreateAccountRequest {
                name: "Acme".to_string(),
                create_tickets: true,
                ticket_provider: TicketProvider::Linear,
                notification_provider: NotificationProvider::Slack,
                notify_min_level: Severity::Error,
                rapid_occurrence_window_minutes: 30,
                rapid_occurrence_threshold: 2,
            })
            .await
            .expect("account");
        let agent = service
            .repository
            .create_agent(CreateAgentRequest {
                account_id: account.id,
                name: "api".to_string(),
            })
            .await
            .expect("agent");

        let response = service
            .ingest(StacktraceEventRequest {
                agent_key: agent.api_key,
                agent_secret: Some(agent.api_secret),
                language: "rust".to_string(),
                stacktrace: "panic: nil pointer dereference".to_string(),
                level: Severity::Error,
                occurred_at: Some(Utc::now()),
                service: None,
                environment: None,
                attributes: Default::default(),
            })
            .await
            .expect("ingest");

        assert!(response.is_new_bug);
        assert_eq!(response.ticket_action, TicketAction::Created);
        assert!(response.notification.sent);
        assert_eq!(
            response.ticket.expect("ticket").priority,
            TicketPriority::High
        );
        assert!(
            response
                .ai_recommendation
                .expect("recommendation")
                .contains("null")
        );
    }

    #[tokio::test]
    async fn escalates_existing_bug_when_it_repeats_rapidly() {
        let service = test_service().await;
        let account = service
            .repository
            .create_account(CreateAccountRequest {
                name: "Beta".to_string(),
                create_tickets: true,
                ticket_provider: TicketProvider::Jira,
                notification_provider: NotificationProvider::Teams,
                notify_min_level: Severity::Warn,
                rapid_occurrence_window_minutes: 60,
                rapid_occurrence_threshold: 2,
            })
            .await
            .expect("account");
        let agent = service
            .repository
            .create_agent(CreateAgentRequest {
                account_id: account.id,
                name: "worker".to_string(),
            })
            .await
            .expect("agent");
        let now = Utc::now();

        service
            .ingest(StacktraceEventRequest {
                agent_key: agent.api_key.clone(),
                agent_secret: Some(agent.api_secret.clone()),
                language: "go".to_string(),
                stacktrace: "panic: index out of bounds".to_string(),
                level: Severity::Warn,
                occurred_at: Some(now),
                service: None,
                environment: None,
                attributes: Default::default(),
            })
            .await
            .expect("first ingest");
        let second = service
            .ingest(StacktraceEventRequest {
                agent_key: agent.api_key,
                agent_secret: Some(agent.api_secret),
                language: "go".to_string(),
                stacktrace: "panic: index out of bounds".to_string(),
                level: Severity::Warn,
                occurred_at: Some(now + chrono::Duration::minutes(10)),
                service: None,
                environment: None,
                attributes: Default::default(),
            })
            .await
            .expect("second ingest");

        assert!(!second.is_new_bug);
        assert_eq!(second.ticket_action, TicketAction::Escalated);
        assert!(second.notification.sent);
        assert_eq!(
            second.ticket.expect("ticket").priority,
            TicketPriority::High
        );
    }

    #[tokio::test]
    async fn suppresses_debug_notification() {
        let service = test_service().await;
        let account = service
            .repository
            .create_account(CreateAccountRequest {
                name: "Gamma".to_string(),
                create_tickets: true,
                ticket_provider: TicketProvider::Tracklines,
                notification_provider: NotificationProvider::Resend,
                notify_min_level: Severity::Error,
                rapid_occurrence_window_minutes: 15,
                rapid_occurrence_threshold: 2,
            })
            .await
            .expect("account");
        let agent = service
            .repository
            .create_agent(CreateAgentRequest {
                account_id: account.id,
                name: "frontend".to_string(),
            })
            .await
            .expect("agent");

        let response = service
            .ingest(StacktraceEventRequest {
                agent_key: agent.api_key,
                agent_secret: Some(agent.api_secret),
                language: "javascript".to_string(),
                stacktrace: "TypeError: Cannot read properties of undefined".to_string(),
                level: Severity::Debug,
                occurred_at: Some(Utc::now()),
                service: None,
                environment: None,
                attributes: Default::default(),
            })
            .await
            .expect("ingest");

        assert_eq!(response.ticket_action, TicketAction::Created);
        assert!(!response.notification.sent);
    }
}
