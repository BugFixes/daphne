use std::{sync::Arc, time::Duration};

use chrono::{DateTime, Utc};
use regex::Regex;
use sha2::{Digest, Sha256};

use crate::{
    AppResult,
    ai::AiRegistry,
    domain::{
        Account, Bug, NotificationOutcome, Severity, StacktraceEventRequest, Ticket, TicketAction,
        TicketPriority,
    },
    notifications::{NotificationRegistry, NotificationRequest, build_notification_message},
    repository::{CreateBugRecord, CreateTicketRecord, Repository},
    ticketing::{
        TicketCommentRequest, TicketCreateRequest, TicketPriorityRequest, TicketingRegistry,
        build_escalation_comment, build_repeat_comment,
    },
};

#[derive(Clone)]
pub struct IntakeService {
    repository: Arc<Repository>,
    ticketing: Arc<TicketingRegistry>,
    notifications: Arc<NotificationRegistry>,
    ai: Arc<AiRegistry>,
}

impl IntakeService {
    pub fn new(
        repository: Arc<Repository>,
        ticketing: Arc<TicketingRegistry>,
        notifications: Arc<NotificationRegistry>,
        ai: Arc<AiRegistry>,
    ) -> Self {
        Self {
            repository,
            ticketing,
            notifications,
            ai,
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
            .create_bug(CreateBugRecord {
                account_id: account.id,
                agent_id,
                language: &request.language,
                severity: request.level,
                stacktrace_hash: &stacktrace_hash,
                normalized_stacktrace: &normalized_stacktrace,
                latest_stacktrace: &request.stacktrace,
                occurred_at,
            })
            .await?;
        self.repository
            .record_occurrence(bug.id, request.level, &request.stacktrace, occurred_at)
            .await?;

        let (ticket_action, ticket, ai_recommendation) = if account.create_tickets {
            let recommendation = self
                .ai
                .default_advisor()?
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
                .ai
                .default_advisor()?
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
                    let provider = self.ticketing.get(existing_ticket.provider)?;
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
                    let provider = self.ticketing.get(existing_ticket.provider)?;
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
                let provider = self.ticketing.get(existing_ticket.provider)?;
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
            .ticketing
            .get(account.ticket_provider)?
            .create_ticket(TicketCreateRequest {
                bug: bug.clone(),
                account: account.clone(),
                priority,
                recommendation: recommendation.to_string(),
                source_stacktrace: stacktrace.to_string(),
            })
            .await?;

        self.repository
            .create_ticket(CreateTicketRecord {
                bug_id: bug.id,
                provider: account.ticket_provider,
                remote_id: &remote_ticket.remote_id,
                remote_url: &remote_ticket.remote_url,
                priority,
                recommendation,
                status: &remote_ticket.status,
                now: occurred_at,
            })
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
        self.notifications
            .get(account.notification_provider)?
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
mod tests;
