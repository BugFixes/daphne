use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;

use crate::{AppError, AppResult, domain::Bug};

pub mod claude;
pub mod codex;
pub mod kimi;

#[cfg(test)]
mod tests;

#[async_trait]
pub trait AiAdvisor: Send + Sync {
    async fn recommend_fix(&self, bug: &Bug, source_stacktrace: &str) -> AppResult<String>;
}

pub use claude::ClaudeAdvisor;
pub use codex::CodexAdvisor;
pub use kimi::KimiAdvisor;

pub struct AiRegistry {
    advisors: HashMap<&'static str, Arc<dyn AiAdvisor>>,
    default_advisor: &'static str,
}

impl AiRegistry {
    pub fn default_advisor(&self) -> AppResult<Arc<dyn AiAdvisor>> {
        self.advisors
            .get(self.default_advisor)
            .cloned()
            .ok_or_else(|| AppError::Internal("default ai advisor not registered".to_string()))
    }

    pub fn default_advisor_key(&self) -> &'static str {
        self.default_advisor
    }
}

impl Default for AiRegistry {
    fn default() -> Self {
        let codex = Arc::new(CodexAdvisor);
        let claude = Arc::new(ClaudeAdvisor);
        let kimi = Arc::new(KimiAdvisor);

        Self {
            advisors: HashMap::from([
                ("codex", codex as Arc<dyn AiAdvisor>),
                ("claude", claude as Arc<dyn AiAdvisor>),
                ("kimi", kimi as Arc<dyn AiAdvisor>),
            ]),
            default_advisor: "codex",
        }
    }
}

fn heuristic_recommendation(bug: &Bug, source_stacktrace: &str, fallback_prefix: &str) -> String {
    let lower = source_stacktrace.to_ascii_lowercase();
    if lower.contains("nullpointer") || lower.contains("nil pointer") || lower.contains("none type")
    {
        "Check the failing call site for missing null or nil guards and capture the unexpected input that reaches this code path.".to_string()
    } else if lower.contains("timeout") {
        "Inspect upstream latency, retry policy, and connection pool saturation around the failing dependency before changing application logic.".to_string()
    } else if lower.contains("connection refused") || lower.contains("econnrefused") {
        "Verify dependency availability and configuration first; this stacktrace suggests a network or service boot-order failure rather than a code defect.".to_string()
    } else if lower.contains("index out of bounds") || lower.contains("outofrange") {
        "Validate collection bounds before indexing and capture the input size that triggers the failing branch.".to_string()
    } else {
        format!(
            "{fallback_prefix} Start with the top non-framework frame in the {} stacktrace, reproduce it with the same inputs, and add structured context around the failing path before patching.",
            bug.language
        )
    }
}
