mod chat;
mod history;
mod persistence;
mod providers;
mod streaming;
mod system_prompt;
mod tools;

use anyhow::Result;
use rig::client::{CompletionClient, Nothing};
use rig::providers::{anthropic, ollama, openai};
use sqlx::{Pool, Sqlite};
use std::path::PathBuf;
use std::sync::Arc;
use tauri::AppHandle;
use tracing::Instrument;
use tracing_opentelemetry::OpenTelemetrySpanExt;

use crate::ai_instances::{AIInstance, APIKeyStorage, LLMProvider};
use crate::memory::{
    fact_extraction, working_memory::Message, ContextBuilder, FactExtractionResponse,
    LongTermMemory, SharedLongTermMemory, SummarizationAgent, SummaryResponse, WorkingMemory,
};
use crate::tools::planning::{self, SharedTodoList};
use crate::tools::registry::RhaiToolRegistry;
use crate::tools::rhai_bridge_tool::SharedRegistry;
use crate::tools::subagents::ClientProvider;
use crate::utils::paths;

use providers::{AgentProvider, FactExtractorProvider, SummaryExtractorProvider};
use tools::create_tools;

/// ownAI Agent with Memory, Tools, and LLM integration
pub struct OwnAIAgent {
    pub(crate) agent: AgentProvider,
    pub(crate) fact_extractor: Arc<FactExtractorProvider>,
    pub(crate) context_builder: ContextBuilder,
    pub(crate) db: Pool<Sqlite>,
    #[allow(dead_code)]
    pub(crate) todo_list: SharedTodoList,
    pub(crate) tool_registry: SharedRegistry,
    pub(crate) instance_id: String,
    pub(crate) instance_name: String,
    pub(crate) provider_name: String,
    pub(crate) model: String,
    pub(crate) system_prompt: String,
}

/// Maximum number of multi-turn iterations for tool calling
const MAX_TOOL_TURNS: usize = 50;

impl OwnAIAgent {
    /// Create a new ownAI Agent with tools.
    /// `max_tokens` allows overriding the default working memory budget (50k tokens).
    pub async fn new(
        instance: &AIInstance,
        db: Pool<Sqlite>,
        max_tokens: Option<usize>,
        app_handle: Option<AppHandle>,
    ) -> Result<Self> {
        // Initialize Memory System components
        let mut working_memory = WorkingMemory::new(max_tokens.unwrap_or(50_000));
        let long_term_memory = LongTermMemory::new(db.clone()).await?;
        let shared_long_term_memory: SharedLongTermMemory =
            std::sync::Arc::new(tokio::sync::Mutex::new(long_term_memory));
        let summarization_agent = SummarizationAgent::new(db.clone());

        // Load recent messages from database into working memory
        let recent_messages = Self::load_recent_messages_from_db(&db, 100).await?;
        if !recent_messages.is_empty() {
            working_memory.load_from_messages(recent_messages);
        }

        let mut context_builder = ContextBuilder::new(
            working_memory,
            shared_long_term_memory.clone(),
            summarization_agent,
        );

        // Create shared TODO list state and register with context builder
        let todo_list = planning::create_shared_todo_list();
        context_builder.set_todo_list(todo_list.clone());

        // Load API key from keychain if needed
        let api_key = if instance.provider.needs_api_key() {
            APIKeyStorage::load(&instance.provider)?.ok_or_else(|| {
                anyhow::anyhow!("API key not found for provider: {}", instance.provider)
            })?
        } else {
            String::new()
        };

        let system_prompt = Self::system_prompt(&instance.name);
        let summary_preamble = "Extract a structured summary from the conversation below. \
            Identify the key facts discussed, any tools that were used or mentioned, \
            and the main topics covered. Be concise but thorough.";
        let fact_preamble = "Extract important, long-term relevant facts from this conversation turn. \
            Focus on: user preferences, skills they mention, factual information about the user, \
            important context for future conversations, and successful tool usage patterns. \
            Ignore temporary context or trivial details. Each fact should be concise and self-contained.";

        // Initialize Rhai Tool Registry for dynamic tools
        let workspace =
            paths::get_instance_workspace_path(&instance.id).unwrap_or_else(|_| PathBuf::from("."));
        let rhai_registry = RhaiToolRegistry::new(
            db.clone(),
            workspace,
            app_handle.clone(),
            Some(instance.name.clone()),
        );
        let available_dynamic_tools = rhai_registry.tool_summary().await.unwrap_or_default();
        let tool_registry: SharedRegistry =
            std::sync::Arc::new(tokio::sync::RwLock::new(rhai_registry));

        // Resolve programs root for canvas tools
        let programs_root = paths::get_instance_programs_path(&instance.id)
            .unwrap_or_else(|_| PathBuf::from("./programs"));

        // Create provider-specific agent with tools, summary extractor, and fact extractor
        let (agent, summary_extractor, fact_extractor) = match instance.provider {
            LLMProvider::Anthropic => {
                let client = anthropic::Client::builder().api_key(&api_key).build()?;
                let client_provider = ClientProvider::Anthropic(client.clone());

                let tools = create_tools(
                    &instance.id,
                    &instance.name,
                    todo_list.clone(),
                    tool_registry.clone(),
                    available_dynamic_tools.clone(),
                    db.clone(),
                    programs_root.clone(),
                    shared_long_term_memory.clone(),
                    client_provider,
                    instance.model.clone(),
                    app_handle.clone(),
                );

                let agent = client
                    .agent(&instance.model)
                    .preamble(&system_prompt)
                    .max_tokens(32768)
                    .temperature(0.7)
                    .name(&instance.name)
                    .tools(tools)
                    .build();

                let summary_extractor = client
                    .extractor::<SummaryResponse>(&instance.model)
                    .preamble(summary_preamble)
                    .max_tokens(8192)
                    .build();

                let fact_extractor = client
                    .extractor::<FactExtractionResponse>(&instance.model)
                    .preamble(fact_preamble)
                    .max_tokens(4096)
                    .build();

                (
                    AgentProvider::Anthropic(agent),
                    SummaryExtractorProvider::Anthropic(summary_extractor),
                    FactExtractorProvider::Anthropic(fact_extractor),
                )
            }

            LLMProvider::OpenAI => {
                let openai_client = openai::Client::builder().api_key(&api_key).build()?;
                let client_provider = ClientProvider::OpenAI(openai_client.clone());

                let tools = create_tools(
                    &instance.id,
                    &instance.name,
                    todo_list.clone(),
                    tool_registry.clone(),
                    available_dynamic_tools.clone(),
                    db.clone(),
                    programs_root.clone(),
                    shared_long_term_memory.clone(),
                    client_provider,
                    instance.model.clone(),
                    app_handle.clone(),
                );

                let agent = openai_client
                    .clone()
                    .completions_api()
                    .agent(&instance.model)
                    .preamble(&system_prompt)
                    .temperature(0.7)
                    .name(&instance.name)
                    .tools(tools)
                    .build();

                let summary_extractor = openai_client
                    .clone()
                    .completions_api()
                    .extractor::<SummaryResponse>(&instance.model)
                    .preamble(summary_preamble)
                    .build();

                let fact_extractor = openai_client
                    .completions_api()
                    .extractor::<FactExtractionResponse>(&instance.model)
                    .preamble(fact_preamble)
                    .build();

                (
                    AgentProvider::OpenAI(agent),
                    SummaryExtractorProvider::OpenAI(summary_extractor),
                    FactExtractorProvider::OpenAI(fact_extractor),
                )
            }

            LLMProvider::Ollama => {
                let ollama_client = if let Some(url) = &instance.api_base_url {
                    ollama::Client::builder()
                        .api_key(Nothing)
                        .base_url(url)
                        .build()?
                } else {
                    ollama::Client::new(Nothing)?
                };
                let client_provider = ClientProvider::Ollama(ollama_client.clone());

                let tools = create_tools(
                    &instance.id,
                    &instance.name,
                    todo_list.clone(),
                    tool_registry.clone(),
                    available_dynamic_tools.clone(),
                    db.clone(),
                    programs_root.clone(),
                    shared_long_term_memory.clone(),
                    client_provider,
                    instance.model.clone(),
                    app_handle,
                );

                let agent = ollama_client
                    .clone()
                    .agent(&instance.model)
                    .preamble(&system_prompt)
                    .name(&instance.name)
                    .tools(tools)
                    .build();

                let summary_extractor = ollama_client
                    .clone()
                    .extractor::<SummaryResponse>(&instance.model)
                    .preamble(summary_preamble)
                    .build();

                let fact_extractor = ollama_client
                    .extractor::<FactExtractionResponse>(&instance.model)
                    .preamble(fact_preamble)
                    .build();

                (
                    AgentProvider::Ollama(agent),
                    SummaryExtractorProvider::Ollama(summary_extractor),
                    FactExtractorProvider::Ollama(fact_extractor),
                )
            }
        };

        // Set the summary extractor and long-term memory on the SummarizationAgent
        context_builder
            .summarization_agent_mut()
            .set_extractor(Box::new(summary_extractor));
        context_builder
            .summarization_agent_mut()
            .set_long_term_memory(shared_long_term_memory.clone());

        Ok(OwnAIAgent {
            agent,
            fact_extractor: Arc::new(fact_extractor),
            context_builder,
            db,
            todo_list,
            tool_registry,
            instance_id: instance.id.clone(),
            instance_name: instance.name.clone(),
            provider_name: instance.provider.to_string(),
            model: instance.model.clone(),
            system_prompt,
        })
    }

    /// Public accessor for context builder (used by memory stats command)
    pub fn context_builder(&self) -> &ContextBuilder {
        &self.context_builder
    }

    /// Public mutable accessor for context builder (used by memory commands)
    pub fn context_builder_mut(&mut self) -> &mut ContextBuilder {
        &mut self.context_builder
    }

    /// Public accessor for the Rhai tool registry (used by tool commands)
    pub fn tool_registry(&self) -> &SharedRegistry {
        &self.tool_registry
    }

    /// Attach Langfuse context attributes (session, tags, metadata) to a tracing span.
    ///
    /// Uses `OpenTelemetrySpanExt` to set the attributes from `LangfuseContext`
    /// on the given span. No-op if Langfuse is not configured.
    pub(crate) fn attach_langfuse_context(&self, span: &tracing::Span) {
        if let Some(ctx) =
            crate::observability::langfuse_context(&self.instance_id, &self.instance_name)
        {
            for attr in ctx.get_attributes() {
                span.set_attribute(attr.key, attr.value);
            }
        }
    }

    /// Summarize evicted messages using the LLM extractor and save to database.
    /// Delegates to `SummarizationAgent::summarize_and_save` which handles
    /// LLM extraction, persistence, and message linking.
    ///
    /// Creates an instrumented tracing span so Langfuse can display the
    /// summarization as a named trace with Input/Output.
    async fn summarize_evicted(&self, evicted: Vec<Message>) -> Result<()> {
        if evicted.is_empty() {
            return Ok(());
        }

        // Create span with Langfuse context and GenAI attributes
        let summarize_span = tracing::info_span!(
            "ownai.summarization",
            instance_id = %self.instance_id,
            instance_name = %self.instance_name,
        );
        self.attach_langfuse_context(&summarize_span);

        let input_text = evicted
            .iter()
            .map(|m| format!("{}: {}", m.role, m.content))
            .collect::<Vec<_>>()
            .join("\n");
        summarize_span.set_attribute("gen_ai.operation.name", "summarization");
        summarize_span.set_attribute("gen_ai.prompt.0.role", "user");
        summarize_span.set_attribute("gen_ai.prompt.0.content", input_text);

        let summary = self
            .context_builder
            .summarization_agent()
            .summarize_and_save(&evicted)
            .instrument(summarize_span.clone())
            .await?;

        summarize_span.set_attribute("gen_ai.completion.0.role", "assistant");
        summarize_span.set_attribute("gen_ai.completion.0.content", summary.summary_text);

        Ok(())
    }

    /// Add a message to working memory; if eviction occurs, summarize in background
    pub(crate) async fn add_to_working_memory(&mut self, msg: Message) {
        if let Some(evicted) = self.context_builder.working_memory_mut().add_message(msg) {
            if let Err(e) = self.summarize_evicted(evicted).await {
                tracing::warn!("Failed to summarize evicted messages: {}", e);
            }
        }
    }

    /// Spawn fact extraction as a background task so it does not block
    /// the completion of chat/streaming. The task runs independently and
    /// logs any errors via tracing.
    ///
    /// After extraction, sets the `importance_score` on the user message
    /// to the maximum importance of all extracted facts.
    ///
    /// Creates an instrumented tracing span so Langfuse can display the
    /// fact extraction as a named trace with Input/Output.
    pub(crate) fn spawn_fact_extraction(
        &self,
        user_message: &str,
        agent_response: &str,
        user_msg_id: &str,
        agent_msg_id: &str,
    ) {
        let fact_extractor = self.fact_extractor.clone();
        let long_term_memory = self.context_builder.long_term_memory().clone();
        let db = self.db.clone();
        let conversation_turn = format!("User: {}\nAgent: {}", user_message, agent_response);
        let user_msg_id = user_msg_id.to_string();
        let agent_msg_id = agent_msg_id.to_string();

        // Create span before spawning so Langfuse context is attached
        let extraction_span = tracing::info_span!(
            "ownai.fact_extraction",
            instance_id = %self.instance_id,
            instance_name = %self.instance_name,
        );
        self.attach_langfuse_context(&extraction_span);
        extraction_span.set_attribute("gen_ai.operation.name", "fact_extraction");
        extraction_span.set_attribute("gen_ai.prompt.0.role", "user");
        extraction_span.set_attribute("gen_ai.prompt.0.content", conversation_turn.clone());

        tokio::spawn(
            async move {
                // Extract facts using LLM
                match fact_extractor.extract(&conversation_turn).await {
                    Ok(extraction) => {
                        let current_span = tracing::Span::current();

                        if extraction.facts.is_empty() {
                            tracing::debug!("No facts extracted from conversation turn");
                            current_span.set_attribute("gen_ai.completion.0.role", "assistant");
                            current_span.set_attribute(
                                "gen_ai.completion.0.content",
                                "No facts extracted".to_string(),
                            );
                            return;
                        }

                        tracing::info!(
                            "Extracted {} facts from conversation",
                            extraction.facts.len()
                        );

                        // Set output on span
                        let facts_output: Vec<String> =
                            extraction.facts.iter().map(|f| f.content.clone()).collect();
                        current_span.set_attribute("gen_ai.completion.0.role", "assistant");
                        current_span
                            .set_attribute("gen_ai.completion.0.content", facts_output.join("\n"));

                        // Compute max importance from extracted facts for the user message
                        let max_importance = extraction
                            .facts
                            .iter()
                            .map(|f| f.importance)
                            .fold(0.0_f32, f32::max);

                        // Update importance_score on the user message
                        Self::update_importance_score(&db, &user_msg_id, max_importance).await;

                        // Convert extracted facts to memory entries and store them
                        let mut mem = long_term_memory.lock().await;

                        for fact_item in extraction.facts {
                            let entry = fact_extraction::to_memory_entry(fact_item, &agent_msg_id);

                            if let Err(e) = mem.store(entry).await {
                                tracing::warn!("Failed to store extracted fact: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        // Don't fail the chat if fact extraction fails - just log it
                        tracing::warn!("Failed to extract facts from conversation: {}", e);
                    }
                }
            }
            .instrument(extraction_span),
        );
    }
}
