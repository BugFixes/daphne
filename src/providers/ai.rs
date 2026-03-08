use async_trait::async_trait;

use crate::{AppResult, domain::Bug};

use super::AiAdvisor;

pub struct HeuristicAiAdvisor;

#[async_trait]
impl AiAdvisor for HeuristicAiAdvisor {
    async fn recommend_fix(&self, bug: &Bug, source_stacktrace: &str) -> AppResult<String> {
        let lower = source_stacktrace.to_ascii_lowercase();
        let recommendation = if lower.contains("nullpointer")
            || lower.contains("nil pointer")
            || lower.contains("none type")
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
                "Start with the top non-framework frame in the {} stacktrace, reproduce it with the same inputs, and add structured context around the failing path before patching.",
                bug.language
            )
        };

        Ok(recommendation)
    }
}
