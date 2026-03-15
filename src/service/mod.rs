use std::{sync::Arc, time::Duration};

use chrono::{DateTime, Utc};
use regex::Regex;
use sha2::{Digest, Sha256};

use crate::{
    AppResult,
    ai::AiRegistry,
    domain::{
        Account, Bug, NotificationOutcome, Severity, StacktraceEvent, Ticket, TicketAction,
        TicketPriority,
    },
    feature_flags::FeatureFlagsClient,
    notifications::{NotificationRegistry, NotificationRequest, build_notification_message},
    policy::{
        CreateTicketAccountPolicyInput, CreateTicketPolicyInput, CreateTicketProviderPolicyInput,
        CreateTicketStackPolicyInput, EscalateRepeatBugPolicyInput, EscalateRepeatPolicyInput,
        EscalateRepeatTicketPolicyInput, PolicyEngine, SendNotificationAccountPolicyInput,
        SendNotificationEventPolicyInput, SendNotificationPolicyInput,
        SendNotificationProviderPolicyInput, SendNotificationTicketPolicyInput,
        UseAiAccountPolicyInput, UseAiAdvisorPolicyInput, UseAiPolicyInput,
    },
    repository::{CreateBugRecord, CreateTicketRecord, RecordOccurrence, Repository},
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
    feature_flags: Arc<dyn FeatureFlagsClient>,
    policy_engine: Arc<dyn PolicyEngine>,
}

impl IntakeService {
    pub fn new(
        repository: Arc<Repository>,
        ticketing: Arc<TicketingRegistry>,
        notifications: Arc<NotificationRegistry>,
        ai: Arc<AiRegistry>,
        feature_flags: Arc<dyn FeatureFlagsClient>,
        policy_engine: Arc<dyn PolicyEngine>,
    ) -> Self {
        Self {
            repository,
            ticketing,
            notifications,
            ai,
            feature_flags,
            policy_engine,
        }
    }

    pub async fn ingest(&self, event: StacktraceEvent) -> AppResult<crate::domain::IntakeOutcome> {
        event.validate()?;

        let agent = match event.agent_secret.as_deref() {
            Some(agent_secret) => {
                self.repository
                    .find_agent_by_credentials(&event.agent_key, agent_secret)
                    .await?
            }
            None => self.repository.find_agent_by_key(&event.agent_key).await?,
        };
        let account = self.repository.find_account(agent.account_id).await?;
        let occurred_at = event.occurred_at.unwrap_or_else(Utc::now);
        let normalized_stacktrace = normalize_stacktrace(&event.stacktrace);
        let stacktrace_hash = hash_stacktrace(&normalized_stacktrace);

        if let Some(existing_bug) = self
            .repository
            .find_bug_by_hash(account.id, &stacktrace_hash)
            .await?
        {
            self.handle_repeat_bug(
                account,
                existing_bug,
                event,
                normalized_stacktrace,
                stacktrace_hash,
                occurred_at,
            )
            .await
        } else {
            self.handle_new_bug(
                account,
                agent.id,
                event,
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
        event: StacktraceEvent,
        normalized_stacktrace: String,
        stacktrace_hash: String,
        occurred_at: DateTime<Utc>,
    ) -> AppResult<crate::domain::IntakeOutcome> {
        let bug = self
            .repository
            .create_bug(CreateBugRecord {
                account_id: account.id,
                agent_id,
                language: &event.language,
                severity: event.level,
                stacktrace_hash: &stacktrace_hash,
                normalized_stacktrace: &normalized_stacktrace,
                latest_stacktrace: &event.stacktrace,
                occurred_at,
            })
            .await?;
        self.repository
            .record_occurrence(RecordOccurrence {
                bug_id: bug.id,
                severity: event.level,
                stacktrace: &event.stacktrace,
                occurred_at,
                service: event.service.as_deref(),
                environment: event.environment.as_deref(),
                attributes: &event.attributes,
            })
            .await?;

        let (ticket_action, ticket, ai_recommendation) = if self
            .policy_engine
            .should_create_ticket(&CreateTicketPolicyInput {
                stack: CreateTicketStackPolicyInput { hash_exists: false },
                account: CreateTicketAccountPolicyInput {
                    ticketing_enabled: account.create_tickets,
                    api_key: account.ticketing_api_key.clone(),
                },
                ticketing: CreateTicketProviderPolicyInput {
                    provider: account.ticket_provider.to_string(),
                    enabled: self
                        .feature_flags
                        .is_enabled(&account.ticket_provider.to_string())
                        .await?,
                },
            })
            .await?
        {
            let recommendation = self
                .recommendation_for(&account, &bug, &event.stacktrace)
                .await?;
            let ticket = self
                .create_ticket(
                    &account,
                    &bug,
                    event.level,
                    &recommendation,
                    &event.stacktrace,
                    occurred_at,
                )
                .await?;
            (TicketAction::Created, Some(ticket), Some(recommendation))
        } else {
            (TicketAction::Skipped, None, None)
        };

        let notification = self
            .maybe_notify(&account, &bug, event.level, ticket_action, occurred_at)
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
        event: StacktraceEvent,
        _normalized_stacktrace: String,
        stacktrace_hash: String,
        occurred_at: DateTime<Utc>,
    ) -> AppResult<crate::domain::IntakeOutcome> {
        self.repository
            .record_occurrence(RecordOccurrence {
                bug_id: existing_bug.id,
                severity: event.level,
                stacktrace: &event.stacktrace,
                occurred_at,
                service: event.service.as_deref(),
                environment: event.environment.as_deref(),
                attributes: &event.attributes,
            })
            .await?;
        let bug = self
            .repository
            .update_bug_on_repeat(&existing_bug, event.level, &event.stacktrace, occurred_at)
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

        if ticket.is_none()
            && self
                .policy_engine
                .should_create_ticket(&CreateTicketPolicyInput {
                    stack: CreateTicketStackPolicyInput { hash_exists: false },
                    account: CreateTicketAccountPolicyInput {
                        ticketing_enabled: account.create_tickets,
                        api_key: account.ticketing_api_key.clone(),
                    },
                    ticketing: CreateTicketProviderPolicyInput {
                        provider: account.ticket_provider.to_string(),
                        enabled: self
                            .feature_flags
                            .is_enabled(&account.ticket_provider.to_string())
                            .await?,
                    },
                })
                .await?
        {
            let recommendation = self
                .recommendation_for(&account, &bug, &event.stacktrace)
                .await?;
            let created_ticket = self
                .create_ticket(
                    &account,
                    &bug,
                    event.level,
                    &recommendation,
                    &event.stacktrace,
                    occurred_at,
                )
                .await?;
            ai_recommendation = Some(recommendation);
            ticket_action = TicketAction::Created;
            ticket = Some(created_ticket);
        } else if let Some(existing_ticket) = ticket.clone() {
            let next_priority = existing_ticket.priority.escalated();
            if self
                .policy_engine
                .should_escalate_repeat(&EscalateRepeatPolicyInput {
                    bug: EscalateRepeatBugPolicyInput {
                        has_ticket: true,
                        recent_count,
                        rapid_occurrence_threshold: account.rapid_occurrence_threshold,
                    },
                    ticket: EscalateRepeatTicketPolicyInput {
                        current_priority_rank: existing_ticket.priority.rank(),
                        next_priority_rank: next_priority.rank(),
                    },
                })
                .await?
            {
                let provider = self.ticketing.get(existing_ticket.provider)?;
                provider
                    .update_priority(TicketPriorityRequest {
                        ticket: existing_ticket.clone(),
                        priority: next_priority,
                    })
                    .await?;
                let comment =
                    build_escalation_comment(recent_count, account.rapid_occurrence_window_minutes);
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
                ticket = Some(
                    self.repository
                        .escalate_ticket(&existing_ticket, next_priority, &comment, occurred_at)
                        .await?,
                );
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
                ticket = Some(
                    self.repository
                        .comment_on_ticket(&existing_ticket, &comment, occurred_at)
                        .await?,
                );
                ticket_action = TicketAction::Commented;
            }
        }

        let notification = self
            .maybe_notify(&account, &bug, event.level, ticket_action, occurred_at)
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

    async fn recommendation_for(
        &self,
        account: &Account,
        bug: &Bug,
        stacktrace: &str,
    ) -> AppResult<String> {
        let advisor_key = self.ai.default_advisor_key();
        if !self
            .policy_engine
            .should_use_ai(&UseAiPolicyInput {
                account: UseAiAccountPolicyInput {
                    enabled: account.ai_enabled,
                    use_managed: account.use_managed_ai,
                    api_key: account.ai_api_key.clone(),
                },
                ai: UseAiAdvisorPolicyInput {
                    advisor: advisor_key.to_string(),
                    enabled: self
                        .feature_flags
                        .is_enabled(&format!("ai/{advisor_key}"))
                        .await?,
                },
            })
            .await?
        {
            return Ok("AI recommendation skipped by policy.".to_string());
        }

        self.ai
            .default_advisor()?
            .recommend_fix(bug, stacktrace)
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
        let message = build_notification_message(account, bug, occurred_at);
        if !self
            .policy_engine
            .should_send_notification(&SendNotificationPolicyInput {
                event: SendNotificationEventPolicyInput { rank: level.rank() },
                account: SendNotificationAccountPolicyInput {
                    notify_min_rank: account.notify_min_level.rank(),
                    api_key: account.notification_api_key.clone(),
                },
                notification: SendNotificationProviderPolicyInput {
                    provider: account.notification_provider.to_string(),
                    enabled: self
                        .feature_flags
                        .is_enabled(&account.notification_provider.to_string())
                        .await?,
                },
                ticket: SendNotificationTicketPolicyInput {
                    action: ticket_action.to_string(),
                },
            })
            .await?
        {
            self.repository
                .record_notification_skip(
                    bug.id,
                    account.notification_provider,
                    level,
                    ticket_action,
                    "policy_denied",
                    occurred_at,
                )
                .await?;
            return Ok(NotificationOutcome {
                sent: false,
                provider: None,
                message: None,
            });
        }
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
            .record_notification(
                bug.id,
                account.notification_provider,
                &message,
                level,
                ticket_action,
                occurred_at,
            )
            .await?;

        Ok(NotificationOutcome {
            sent: true,
            provider: Some(account.notification_provider),
            message: Some(message),
        })
    }
}

/// Canonicalize a raw stacktrace before hashing by trimming lines, dropping empties,
/// and masking unstable memory addresses.
fn normalize_stacktrace(input: &str) -> String {
    let address_pattern = Regex::new(r"0x[0-9a-fA-F]+").expect("valid address regex");
    let goroutine_pattern = Regex::new(r"\bgoroutine \d+\b").expect("valid goroutine regex");
    input
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| {
            let line = address_pattern.replace_all(line, "0xADDR");
            goroutine_pattern
                .replace_all(line.as_ref(), "goroutine N")
                .into_owned()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Hash the canonicalized stacktrace used in the `(account_id, stacktrace_hash)` bug key.
fn hash_stacktrace(normalized_stacktrace: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(normalized_stacktrace.as_bytes());
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests;
