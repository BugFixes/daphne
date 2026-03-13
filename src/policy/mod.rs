use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use reqwest::Client;
use serde::Serialize;
use serde_json::Value;

use crate::{AppError, AppResult, config::Config};

#[cfg(test)]
mod tests;

const CREATE_TICKET_RULE: &str = include_str!("../../policies/create_ticket.policy");
const ESCALATE_REPEAT_RULE: &str = include_str!("../../policies/escalate_repeat.policy");
const SEND_NOTIFICATION_RULE: &str = include_str!("../../policies/send_notification.policy");
const USE_AI_RULE: &str = include_str!("../../policies/use_ai.policy");

#[derive(Debug, Clone, Serialize)]
pub struct CreateTicketPolicyInput {
    pub stack: CreateTicketStackPolicyInput,
    pub account: CreateTicketAccountPolicyInput,
    pub ticketing: CreateTicketProviderPolicyInput,
}

#[derive(Debug, Clone, Serialize)]
pub struct CreateTicketStackPolicyInput {
    pub hash_exists: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct CreateTicketAccountPolicyInput {
    pub ticketing_enabled: bool,
    pub api_key: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CreateTicketProviderPolicyInput {
    pub provider: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct EscalateRepeatPolicyInput {
    pub bug: EscalateRepeatBugPolicyInput,
    pub ticket: EscalateRepeatTicketPolicyInput,
}

#[derive(Debug, Clone, Serialize)]
pub struct EscalateRepeatBugPolicyInput {
    pub has_ticket: bool,
    pub recent_count: i64,
    pub rapid_occurrence_threshold: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct EscalateRepeatTicketPolicyInput {
    pub current_priority_rank: u8,
    pub next_priority_rank: u8,
}

#[derive(Debug, Clone, Serialize)]
pub struct SendNotificationPolicyInput {
    pub event: SendNotificationEventPolicyInput,
    pub account: SendNotificationAccountPolicyInput,
    pub notification: SendNotificationProviderPolicyInput,
    pub ticket: SendNotificationTicketPolicyInput,
}

#[derive(Debug, Clone, Serialize)]
pub struct SendNotificationEventPolicyInput {
    pub rank: u8,
}

#[derive(Debug, Clone, Serialize)]
pub struct SendNotificationAccountPolicyInput {
    pub notify_min_rank: u8,
    pub api_key: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SendNotificationProviderPolicyInput {
    pub provider: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct SendNotificationTicketPolicyInput {
    pub action: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct UseAiPolicyInput {
    pub account: UseAiAccountPolicyInput,
    pub ai: UseAiAdvisorPolicyInput,
}

#[derive(Debug, Clone, Serialize)]
pub struct UseAiAccountPolicyInput {
    pub enabled: bool,
    pub use_managed: bool,
    pub api_key: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UseAiAdvisorPolicyInput {
    pub advisor: String,
    pub enabled: bool,
}

#[async_trait]
pub trait PolicyEngine: Send + Sync {
    async fn should_create_ticket(&self, input: &CreateTicketPolicyInput) -> AppResult<bool>;
    async fn should_escalate_repeat(&self, input: &EscalateRepeatPolicyInput) -> AppResult<bool>;
    async fn should_send_notification(
        &self,
        input: &SendNotificationPolicyInput,
    ) -> AppResult<bool>;
    async fn should_use_ai(&self, input: &UseAiPolicyInput) -> AppResult<bool>;
}

pub fn build_policy_engine(config: &Config) -> AppResult<Arc<dyn PolicyEngine>> {
    match config.policy_provider.as_str() {
        "local" => Ok(Arc::new(LocalPolicyEngine)),
        "policy2" => Ok(Arc::new(Policy2PolicyEngine::from_config(config)?)),
        _ => Err(AppError::Validation(
            "BUGFIXES_POLICY_PROVIDER must be one of: local, policy2".to_string(),
        )),
    }
}

pub struct LocalPolicyEngine;

#[async_trait]
impl PolicyEngine for LocalPolicyEngine {
    async fn should_create_ticket(&self, input: &CreateTicketPolicyInput) -> AppResult<bool> {
        Ok(!input.stack.hash_exists
            && input.account.ticketing_enabled
            && input.ticketing.enabled
            && input
                .account
                .api_key
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty()))
    }

    async fn should_escalate_repeat(&self, input: &EscalateRepeatPolicyInput) -> AppResult<bool> {
        Ok(input.bug.has_ticket
            && input.bug.recent_count >= input.bug.rapid_occurrence_threshold
            && input.ticket.next_priority_rank > input.ticket.current_priority_rank)
    }

    async fn should_send_notification(
        &self,
        input: &SendNotificationPolicyInput,
    ) -> AppResult<bool> {
        Ok(input.notification.enabled
            && input
                .account
                .api_key
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty())
            && input.event.rank >= input.account.notify_min_rank
            && matches!(input.ticket.action.as_str(), "created" | "escalated"))
    }

    async fn should_use_ai(&self, input: &UseAiPolicyInput) -> AppResult<bool> {
        Ok(input.account.enabled
            && input.ai.enabled
            && (input.account.use_managed
                || input
                    .account
                    .api_key
                    .as_deref()
                    .is_some_and(|value| !value.trim().is_empty())))
    }
}

pub struct Policy2PolicyEngine {
    client: Client,
    endpoint: String,
}

impl Policy2PolicyEngine {
    pub fn from_config(config: &Config) -> AppResult<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .map_err(|error| AppError::Internal(format!("policy2 client build failed: {error}")))?;

        Ok(Self {
            client,
            endpoint: config.policy2_engine_url.clone(),
        })
    }

    async fn evaluate<T>(&self, rule: &str, data: &T) -> AppResult<bool>
    where
        T: Serialize + Sync,
    {
        let payload = serde_json::json!({
            "rule": rule,
            "data": data,
        });

        let response = match self.client.post(&self.endpoint).json(&payload).send().await {
            Ok(response) => response,
            Err(error) => {
                tracing::warn!(%error, "policy2 request failed, falling back to local policy");
                return Err(AppError::Internal(format!(
                    "policy2 request failed: {error}"
                )));
            }
        };

        let status = response.status();
        let body = response
            .json::<Policy2EngineResponse>()
            .await
            .map_err(|error| {
                AppError::Internal(format!("policy2 response decode failed: {error}"))
            })?;

        if !status.is_success() {
            return Err(AppError::Internal(format!(
                "policy2 engine returned {status}: {}",
                body.error
                    .unwrap_or_else(|| "unknown engine error".to_string())
            )));
        }

        if let Some(error) = body.error {
            return Err(AppError::Internal(format!("policy2 engine error: {error}")));
        }

        Ok(body.result)
    }
}

#[async_trait]
impl PolicyEngine for Policy2PolicyEngine {
    async fn should_create_ticket(&self, input: &CreateTicketPolicyInput) -> AppResult<bool> {
        self.evaluate(CREATE_TICKET_RULE, input).await
    }

    async fn should_escalate_repeat(&self, input: &EscalateRepeatPolicyInput) -> AppResult<bool> {
        self.evaluate(ESCALATE_REPEAT_RULE, input).await
    }

    async fn should_send_notification(
        &self,
        input: &SendNotificationPolicyInput,
    ) -> AppResult<bool> {
        self.evaluate(SEND_NOTIFICATION_RULE, input).await
    }

    async fn should_use_ai(&self, input: &UseAiPolicyInput) -> AppResult<bool> {
        self.evaluate(USE_AI_RULE, input).await
    }
}

#[derive(Debug, serde::Deserialize)]
struct Policy2EngineResponse {
    result: bool,
    error: Option<String>,
    #[allow(dead_code)]
    trace: Option<Value>,
}
