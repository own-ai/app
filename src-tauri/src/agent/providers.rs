use anyhow::Result;
use rig::agent::Agent;
use rig::extractor::Extractor;
use rig::providers::{anthropic, ollama, openai};
use std::future::Future;
use std::pin::Pin;

use crate::memory::{FactExtractionResponse, SummaryExtractor, SummaryResponse};

/// Provider-specific agent wrapper.
/// Each variant holds a fully-built Agent with tools registered.
pub(crate) enum AgentProvider {
    Anthropic(Agent<anthropic::completion::CompletionModel>),
    OpenAI(Agent<openai::CompletionModel>),
    Ollama(Agent<ollama::CompletionModel>),
}

/// Provider-specific extractor for structured summary extraction from LLM.
/// Uses rig Extractors for type-safe structured output via tool-based extraction.
pub(crate) enum SummaryExtractorProvider {
    Anthropic(Extractor<anthropic::completion::CompletionModel, SummaryResponse>),
    OpenAI(Extractor<openai::CompletionModel, SummaryResponse>),
    Ollama(Extractor<ollama::CompletionModel, SummaryResponse>),
}

impl SummaryExtractor for SummaryExtractorProvider {
    fn extract_summary<'a>(
        &'a self,
        text: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<SummaryResponse>> + Send + 'a>> {
        Box::pin(async move {
            match self {
                Self::Anthropic(e) => Ok(e.extract(text).await?),
                Self::OpenAI(e) => Ok(e.extract(text).await?),
                Self::Ollama(e) => Ok(e.extract(text).await?),
            }
        })
    }
}

/// Provider-specific extractor for fact extraction from conversations.
/// Uses rig Extractors for type-safe structured output via tool-based extraction.
pub(crate) enum FactExtractorProvider {
    Anthropic(Extractor<anthropic::completion::CompletionModel, FactExtractionResponse>),
    OpenAI(Extractor<openai::CompletionModel, FactExtractionResponse>),
    Ollama(Extractor<ollama::CompletionModel, FactExtractionResponse>),
}

impl FactExtractorProvider {
    /// Extract facts from a conversation turn
    pub(crate) async fn extract(&self, text: &str) -> Result<FactExtractionResponse> {
        match self {
            Self::Anthropic(e) => Ok(e.extract(text).await?),
            Self::OpenAI(e) => Ok(e.extract(text).await?),
            Self::Ollama(e) => Ok(e.extract(text).await?),
        }
    }
}
