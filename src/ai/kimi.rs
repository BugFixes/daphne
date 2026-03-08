use async_trait::async_trait;

use crate::{AppResult, domain::Bug};

use super::{AiAdvisor, heuristic_recommendation};

pub struct KimiAdvisor;

#[async_trait]
impl AiAdvisor for KimiAdvisor {
    async fn recommend_fix(&self, bug: &Bug, source_stacktrace: &str) -> AppResult<String> {
        Ok(heuristic_recommendation(
            bug,
            source_stacktrace,
            "Kimi suggestion.",
        ))
    }
}
