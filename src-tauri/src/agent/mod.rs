use anyhow::{Context, Result};
use chrono::Utc;
use futures::StreamExt;
use rig::agent::{Agent, MultiTurnStreamItem};
use rig::client::{CompletionClient, Nothing};
use rig::completion::Prompt;
use rig::extractor::Extractor;
use rig::message::{
    AssistantContent, Message as RigMessage, Text as RigText, ToolCall as RigToolCall,
    ToolFunction as RigToolFunction, ToolResult as RigToolResult,
    ToolResultContent as RigToolResultContent, UserContent,
};
use rig::one_or_many::OneOrMany;
use rig::providers::{anthropic, ollama, openai};
use rig::streaming::{StreamedAssistantContent, StreamedUserContent, StreamingChat};
use rig::tool::ToolDyn;
use sqlx::{Pool, Row, Sqlite};
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use tauri::{AppHandle, Manager};
use tracing::Instrument;
use tracing_opentelemetry::OpenTelemetrySpanExt;

use crate::ai_instances::{AIInstance, APIKeyStorage, LLMProvider};
use crate::canvas::tools::{
    CreateProgramTool, ListProgramsTool, OpenProgramTool, ProgramEditFileTool, ProgramLsTool,
    ProgramReadFileTool, ProgramWriteFileTool,
};
use crate::memory::{
    fact_extraction,
    working_memory::{Message, MessageMetadata, ToolCallData, ToolResultData},
    ContextBuilder, FactExtractionResponse, LongTermMemory, SharedLongTermMemory,
    SummarizationAgent, SummaryExtractor, SummaryResponse, WorkingMemory,
};
use crate::scheduler::{
    CreateScheduledTaskTool, DeleteScheduledTaskTool, ListScheduledTasksTool, SharedScheduler,
};
use crate::tools::code_generation::{CreateToolTool, ReadToolTool, UpdateToolTool};
use crate::tools::collection_tools::{
    CreateKnowledgeCollectionTool, DeleteKnowledgeCollectionTool, IngestDocumentTool,
    ListKnowledgeCollectionsTool,
};
use crate::tools::filesystem::{EditFileTool, GrepTool, LsTool, ReadFileTool, WriteFileTool};
use crate::tools::memory_tools::{AddMemoryTool, DeleteMemoryTool, SearchMemoryTool};
use crate::tools::planning::{self, ReadTodosTool, SharedTodoList, WriteTodosTool};
use crate::tools::registry::RhaiToolRegistry;
use crate::tools::rhai_bridge_tool::{RhaiExecuteTool, SharedRegistry};
use crate::tools::subagents::{base_tools_prompt, ClientProvider, DelegateTaskTool};
use crate::utils::paths;

/// Macro to process streaming responses uniformly across providers.
/// Handles text chunks, tool calls, tool results, and multi-turn items.
/// Captures intermediate tool messages for DB persistence and the
/// `FinalResponse` (if any) so callers can extract token usage.
macro_rules! process_stream {
    ($stream:expr, $callback:expr, $full_response:expr, $final_response:expr, $intermediate_messages:expr) => {
        {
            let mut _current_turn_text = String::new();
            let mut _current_turn_tool_calls: Vec<crate::memory::working_memory::ToolCallData> = Vec::new();
            // Buffer tool results so they are pushed AFTER the agent+tool_calls
            // message on the Final event. Without this buffer, tool results would
            // be saved before the corresponding tool_use message because rig
            // delivers ToolResult events before the Final event that closes the
            // assistant turn.
            let mut _pending_tool_results: Vec<crate::memory::working_memory::Message> = Vec::new();

            while let Some(result) = $stream.next().await {
                match result {
                    Ok(item) => match item {
                        MultiTurnStreamItem::StreamAssistantItem(content) => match content {
                            StreamedAssistantContent::Text(text) => {
                                $callback(text.text.clone());
                                $full_response.push_str(&text.text);
                                _current_turn_text.push_str(&text.text);
                            }
                            StreamedAssistantContent::ToolCall { tool_call, .. } => {
                                _current_turn_tool_calls.push(crate::memory::working_memory::ToolCallData {
                                    id: tool_call.id.clone(),
                                    call_id: tool_call.call_id.clone(),
                                    name: tool_call.function.name.clone(),
                                    arguments: tool_call.function.arguments.clone(),
                                });
                            }
                            StreamedAssistantContent::Final(_) => {
                                // End of assistant turn: if tool calls were made, save as intermediate.
                                // Push agent+tool_calls FIRST, then any buffered tool results,
                                // so the DB ordering matches the expected sequence:
                                //   assistant(tool_use) -> user(tool_result)
                                if !_current_turn_tool_calls.is_empty() {
                                    $intermediate_messages.push(crate::memory::working_memory::Message {
                                        id: uuid::Uuid::new_v4().to_string(),
                                        role: "agent".to_string(),
                                        content: _current_turn_text.clone(),
                                        timestamp: chrono::Utc::now(),
                                        importance_score: None,
                                        metadata: Some(crate::memory::working_memory::MessageMetadata::ToolCalls {
                                            calls: _current_turn_tool_calls.drain(..).collect(),
                                        }),
                                    });
                                    // Now flush buffered tool results after the agent message
                                    for mut tr in _pending_tool_results.drain(..) {
                                        // Ensure tool result timestamp is after agent message
                                        tr.timestamp = chrono::Utc::now();
                                        $intermediate_messages.push(tr);
                                    }
                                    // Remove intermediate turn text from full_response so only
                                    // the final turn's text remains as the agent message.
                                    if $full_response.ends_with(&_current_turn_text) {
                                        let new_len = $full_response.len() - _current_turn_text.len();
                                        $full_response.truncate(new_len);
                                    }
                                    _current_turn_text.clear();
                                }
                            }
                            _ => {} // Ignore other content types
                        },
                        MultiTurnStreamItem::StreamUserItem(user_content) => {
                            let StreamedUserContent::ToolResult { tool_result, .. } = user_content;
                            {
                                let result_text = tool_result.content.iter().map(|c| match c {
                                    RigToolResultContent::Text(t) => t.text.clone(),
                                    _ => String::new(),
                                }).collect::<Vec<_>>().join("");

                                // Buffer tool results instead of pushing directly.
                                // They will be flushed in the correct order (after
                                // the agent+tool_calls message) on the Final event.
                                _pending_tool_results.push(crate::memory::working_memory::Message {
                                    id: uuid::Uuid::new_v4().to_string(),
                                    role: "tool_result".to_string(),
                                    content: result_text,
                                    timestamp: chrono::Utc::now(),
                                    importance_score: None,
                                    metadata: Some(crate::memory::working_memory::MessageMetadata::ToolResult(
                                        crate::memory::working_memory::ToolResultData {
                                            tool_call_id: tool_result.id.clone(),
                                            call_id: tool_result.call_id.clone(),
                                        }
                                    )),
                                });
                            }
                        }
                        MultiTurnStreamItem::FinalResponse(res) => {
                            $final_response = Some(res);
                        }
                        _ => {} // Future variants
                    },
                    Err(e) => {
                        return Err(anyhow::anyhow!("Streaming error: {}", e));
                    }
                }
            }
        }
    };
}

/// Helper: Create the set of tools for an instance.
/// Includes all tools: filesystem, planning, dynamic tools, self-programming,
/// canvas, memory, and task delegation (sub-agents).
#[allow(clippy::too_many_arguments)]
fn create_tools(
    instance_id: &str,
    instance_name: &str,
    todo_list: SharedTodoList,
    registry: SharedRegistry,
    available_dynamic_tools: Vec<(String, String)>,
    db: Pool<Sqlite>,
    programs_root: PathBuf,
    long_term_memory: SharedLongTermMemory,
    client_provider: ClientProvider,
    model: String,
    app_handle: Option<AppHandle>,
) -> Vec<Box<dyn ToolDyn>> {
    let workspace =
        paths::get_instance_workspace_path(instance_id).unwrap_or_else(|_| PathBuf::from("."));

    let mut tools: Vec<Box<dyn ToolDyn>> = vec![
        // Filesystem tools
        Box::new(LsTool::new(workspace.clone())),
        Box::new(ReadFileTool::new(workspace.clone())),
        Box::new(WriteFileTool::new(workspace.clone())),
        Box::new(EditFileTool::new(workspace.clone())),
        Box::new(GrepTool::new(workspace.clone())),
        // Planning tools
        Box::new(ReadTodosTool::new(todo_list.clone())),
        Box::new(WriteTodosTool::new(todo_list)),
        // Dynamic Rhai tool executor
        Box::new(RhaiExecuteTool::new(
            registry.clone(),
            available_dynamic_tools,
        )),
        // Self-programming: create, read, and update dynamic tools
        Box::new(CreateToolTool::new(registry.clone(), workspace.clone())),
        Box::new(ReadToolTool::new(registry.clone())),
        Box::new(UpdateToolTool::new(registry.clone(), workspace.clone())),
        // Canvas program tools
        Box::new(CreateProgramTool::new(
            db.clone(),
            instance_id.to_string(),
            programs_root.clone(),
            app_handle.clone(),
        )),
        Box::new(ListProgramsTool::new(db.clone(), instance_id.to_string())),
        Box::new(OpenProgramTool::new(
            db.clone(),
            instance_id.to_string(),
            app_handle.clone(),
        )),
        Box::new(ProgramLsTool::new(programs_root.clone())),
        Box::new(ProgramReadFileTool::new(programs_root.clone())),
        Box::new(ProgramWriteFileTool::new(
            db.clone(),
            instance_id.to_string(),
            programs_root.clone(),
            app_handle.clone(),
        )),
        Box::new(ProgramEditFileTool::new(
            db.clone(),
            instance_id.to_string(),
            programs_root.clone(),
            app_handle.clone(),
        )),
        // Memory tools (long-term vector store)
        Box::new(SearchMemoryTool::new(long_term_memory.clone(), db.clone())),
        Box::new(AddMemoryTool::new(long_term_memory.clone(), db.clone())),
        Box::new(DeleteMemoryTool::new(long_term_memory.clone())),
        // Task delegation (sub-agents)
        Box::new(DelegateTaskTool::new(
            client_provider,
            model,
            instance_id.to_string(),
            instance_name.to_string(),
            registry,
            db.clone(),
            programs_root,
            long_term_memory.clone(),
            app_handle.clone(),
        )),
        // Knowledge collection tools (document ingestion & organization)
        Box::new(CreateKnowledgeCollectionTool::new(db.clone())),
        Box::new(ListKnowledgeCollectionsTool::new(db.clone())),
        Box::new(DeleteKnowledgeCollectionTool::new(db.clone())),
        Box::new(IngestDocumentTool::new(
            db.clone(),
            long_term_memory,
            workspace,
        )),
    ];

    // Scheduled task tools (only available when scheduler is initialized)
    if let Some(ref handle) = app_handle {
        if let Some(scheduler_state) = handle.try_state::<SharedScheduler>() {
            let scheduler = scheduler_state.inner().clone();
            if let Some(manager_state) =
                handle.try_state::<std::sync::Arc<tokio::sync::Mutex<crate::ai_instances::AIInstanceManager>>>()
            {
                let manager = manager_state.inner().clone();
                tools.push(Box::new(CreateScheduledTaskTool::new(
                    db.clone(),
                    instance_id.to_string(),
                    scheduler.clone(),
                    manager,
                    app_handle.clone(),
                )));
                tools.push(Box::new(ListScheduledTasksTool::new(
                    db.clone(),
                    instance_id.to_string(),
                )));
                tools.push(Box::new(DeleteScheduledTaskTool::new(db, scheduler)));
            }
        }
    }

    tools
}

/// Provider-specific agent wrapper.
/// Each variant holds a fully-built Agent with tools registered.
enum AgentProvider {
    Anthropic(Agent<anthropic::completion::CompletionModel>),
    OpenAI(Agent<openai::CompletionModel>),
    Ollama(Agent<ollama::CompletionModel>),
}

/// Provider-specific extractor for structured summary extraction from LLM.
/// Uses rig Extractors for type-safe structured output via tool-based extraction.
enum SummaryExtractorProvider {
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
enum FactExtractorProvider {
    Anthropic(Extractor<anthropic::completion::CompletionModel, FactExtractionResponse>),
    OpenAI(Extractor<openai::CompletionModel, FactExtractionResponse>),
    Ollama(Extractor<ollama::CompletionModel, FactExtractionResponse>),
}

impl FactExtractorProvider {
    /// Extract facts from a conversation turn
    async fn extract(&self, text: &str) -> Result<FactExtractionResponse> {
        match self {
            Self::Anthropic(e) => Ok(e.extract(text).await?),
            Self::OpenAI(e) => Ok(e.extract(text).await?),
            Self::Ollama(e) => Ok(e.extract(text).await?),
        }
    }
}

/// OwnAI Agent with Memory, Tools, and LLM integration
pub struct OwnAIAgent {
    agent: AgentProvider,
    fact_extractor: Arc<FactExtractorProvider>,
    context_builder: ContextBuilder,
    db: Pool<Sqlite>,
    #[allow(dead_code)]
    todo_list: SharedTodoList,
    tool_registry: SharedRegistry,
    instance_id: String,
    instance_name: String,
    provider_name: String,
    model: String,
    system_prompt: String,
}

/// Maximum number of multi-turn iterations for tool calling
const MAX_TOOL_TURNS: usize = 50;

impl OwnAIAgent {
    /// Create a new OwnAI Agent with tools.
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

    /// Attach Langfuse context attributes (session, tags, metadata) to a tracing span.
    ///
    /// Uses `OpenTelemetrySpanExt` to set the attributes from `LangfuseContext`
    /// on the given span. No-op if Langfuse is not configured.
    fn attach_langfuse_context(&self, span: &tracing::Span) {
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

    /// Build chat history from working memory messages, inserting time gap markers
    /// between messages that have significant temporal gaps (>= 4 hours).
    /// The markers are prepended to the following message's content so the LLM
    /// can see where temporal breaks occurred without altering the user/assistant
    /// alternation pattern. The last message in working memory (the current user
    /// message) is excluded since it becomes the prompt.
    fn build_history_with_time_markers(&self) -> Vec<RigMessage> {
        let messages = self.context_builder.working_memory().get_context();
        let msg_count = messages.len();
        if msg_count <= 1 {
            return Vec::new();
        }

        let history_messages = &messages[..msg_count - 1];
        let mut history = Vec::with_capacity(history_messages.len());
        let mut i = 0;

        while i < history_messages.len() {
            let msg = &history_messages[i];

            // Check time gap from previous message
            let time_marker = if i > 0 {
                let gap = msg
                    .timestamp
                    .signed_duration_since(history_messages[i - 1].timestamp);
                crate::memory::ContextBuilder::format_history_time_marker(gap)
            } else {
                None
            };

            match msg.role.as_str() {
                "tool_result" => {
                    // Group consecutive tool_result messages into one User message
                    // with multiple ToolResult items (as rig expects).
                    let mut tool_results: Vec<UserContent> = Vec::new();
                    while i < history_messages.len() && history_messages[i].role == "tool_result" {
                        let tr_msg = &history_messages[i];
                        if let Some(MessageMetadata::ToolResult(ref tr_data)) = tr_msg.metadata {
                            tool_results.push(UserContent::ToolResult(RigToolResult {
                                id: tr_data.tool_call_id.clone(),
                                call_id: tr_data.call_id.clone(),
                                content: OneOrMany::one(RigToolResultContent::Text(RigText {
                                    text: tr_msg.content.clone(),
                                })),
                            }));
                        }
                        i += 1;
                    }
                    if !tool_results.is_empty() {
                        let mut content = OneOrMany::one(tool_results.remove(0));
                        for tr in tool_results {
                            content.push(tr);
                        }
                        history.push(RigMessage::User { content });
                    }
                    continue; // i already advanced in the inner loop
                }
                "agent" => {
                    if msg.metadata.is_some() {
                        // Agent message with tool calls: reconstruct structured rig message
                        history.push(Self::db_message_to_rig_message(msg));
                    } else if !msg.content.is_empty() {
                        // Plain text agent message
                        let content = if let Some(marker) = time_marker {
                            format!("{}\n{}", marker, msg.content)
                        } else {
                            msg.content.clone()
                        };
                        history.push(RigMessage::assistant(&content));
                    }
                }
                _ => {
                    // "user" or other roles
                    let content = if let Some(marker) = time_marker {
                        format!("{}\n{}", marker, msg.content)
                    } else {
                        msg.content.clone()
                    };
                    history.push(RigMessage::user(&content));
                }
            }

            i += 1;
        }

        // Sanitize: ensure every tool_result has a matching tool_use in the
        // immediately preceding assistant message.
        Self::sanitize_tool_history(&mut history);

        history
    }

    /// Validate and fix tool_use / tool_result pairing in the history.
    ///
    /// Anthropic requires that every `tool_result` block references a `tool_use`
    /// block in the **immediately preceding** assistant message. This can be
    /// violated when:
    /// - DB messages were saved in wrong order (fixed in process_stream! but
    ///   older data may still have the issue)
    /// - Working memory truncation/eviction split a tool_use/tool_result pair
    ///
    /// For any orphaned tool_result (no matching tool_use in preceding message),
    /// we convert it to a plain text user message to preserve the information
    /// without violating the API protocol.
    fn sanitize_tool_history(history: &mut Vec<RigMessage>) {
        let mut i = 0;
        while i < history.len() {
            let needs_fix = if let RigMessage::User { ref content } = history[i] {
                // Check if this User message contains ToolResult items
                let tool_result_ids: Vec<String> = content
                    .iter()
                    .filter_map(|uc| {
                        if let UserContent::ToolResult(ref tr) = uc {
                            Some(tr.id.clone())
                        } else {
                            None
                        }
                    })
                    .collect();

                if tool_result_ids.is_empty() {
                    false // Not a tool result message, skip
                } else if i == 0 {
                    true // No preceding message at all
                } else {
                    // Check if preceding message is an Assistant with matching tool_use ids
                    if let RigMessage::Assistant { ref content, .. } = history[i - 1] {
                        let tool_use_ids: std::collections::HashSet<String> = content
                            .iter()
                            .filter_map(|ac| {
                                if let AssistantContent::ToolCall(ref tc) = ac {
                                    Some(tc.id.clone())
                                } else {
                                    None
                                }
                            })
                            .collect();

                        // Check if ALL tool_result ids have a matching tool_use
                        !tool_result_ids.iter().all(|id| tool_use_ids.contains(id))
                    } else {
                        true // Preceding message is not an Assistant
                    }
                }
            } else {
                false
            };

            if needs_fix {
                // Convert tool_result to plain text user message
                let text = if let RigMessage::User { ref content } = history[i] {
                    content
                        .iter()
                        .filter_map(|uc| {
                            if let UserContent::ToolResult(ref tr) = uc {
                                let result_text = tr
                                    .content
                                    .iter()
                                    .map(|c| match c {
                                        RigToolResultContent::Text(ref t) => t.text.clone(),
                                        _ => String::new(),
                                    })
                                    .collect::<Vec<_>>()
                                    .join("");
                                Some(format!("[Previous tool result: {}]", result_text))
                            } else {
                                None
                            }
                        })
                        .collect::<Vec<_>>()
                        .join("\n")
                } else {
                    String::new()
                };

                if text.is_empty() {
                    history.remove(i);
                    // Don't increment i since we removed the element
                } else {
                    tracing::warn!(
                        "Sanitizing orphaned tool_result at history index {} -> plain text",
                        i
                    );
                    history[i] = RigMessage::user(&text);
                    i += 1;
                }
            } else {
                i += 1;
            }
        }
    }

    /// Add a message to working memory; if eviction occurs, summarize in background
    async fn add_to_working_memory(&mut self, msg: Message) {
        if let Some(evicted) = self.context_builder.working_memory_mut().add_message(msg) {
            if let Err(e) = self.summarize_evicted(evicted).await {
                tracing::warn!("Failed to summarize evicted messages: {}", e);
            }
        }
    }

    /// Main chat method (non-streaming) - combines Memory + Tools + LLM.
    ///
    /// Creates an instrumented tracing span with Langfuse context attributes
    /// so that all LLM calls within this chat turn are associated with the
    /// correct session, tags, and metadata in Langfuse.
    pub async fn chat(&mut self, user_message: &str) -> Result<String> {
        let chat_span = tracing::info_span!(
            "ownai.chat",
            instance_id = %self.instance_id,
            instance_name = %self.instance_name,
        );
        self.attach_langfuse_context(&chat_span);
        self.chat_inner(user_message).instrument(chat_span).await
    }

    /// Inner implementation of `chat()`, executed within an instrumented span.
    async fn chat_inner(&mut self, user_message: &str) -> Result<String> {
        // Set GenAI semantic convention attributes on the parent span so that
        // Langfuse can display Input/Output and render the flow diagram.
        let current_span = tracing::Span::current();
        current_span.set_attribute("gen_ai.operation.name", "chat");
        current_span.set_attribute("gen_ai.provider.name", self.provider_name.clone());
        current_span.set_attribute("gen_ai.request.model", self.model.clone());
        current_span.set_attribute("gen_ai.system_instructions", self.system_prompt.clone());
        current_span.set_attribute("gen_ai.prompt.0.role", "user");
        current_span.set_attribute("gen_ai.prompt.0.content", user_message.to_string());

        // 1. Create user message, save to DB first (so FK constraints work for summaries)
        let user_msg = Message {
            id: uuid::Uuid::new_v4().to_string(),
            role: "user".to_string(),
            content: user_message.to_string(),
            timestamp: Utc::now(),
            importance_score: None,
            metadata: None,
        };
        let user_msg_id = user_msg.id.clone();
        self.save_message_to_db(&user_msg).await?;

        // 2. Add to working memory (may trigger eviction -> summarization)
        self.add_to_working_memory(user_msg).await;

        // 3. Build context from all memory layers
        let context = self.context_builder.build_context(user_message).await?;

        // 4. Build chat history from working memory (with time gap markers)
        let mut history = self.build_history_with_time_markers();
        let history_len_before = history.len();

        // 5. Prepare prompt with memory context
        let prompt = if !context.is_empty() {
            format!("[Context from memory]\n{}\n\n{}", context, user_message)
        } else {
            user_message.to_string()
        };

        // 6. Call LLM with multi-turn tool support
        let response = match &self.agent {
            AgentProvider::Anthropic(agent) => {
                agent
                    .prompt(&prompt)
                    .with_history(&mut history)
                    .max_turns(MAX_TOOL_TURNS)
                    .await?
            }
            AgentProvider::OpenAI(agent) => {
                agent
                    .prompt(&prompt)
                    .with_history(&mut history)
                    .max_turns(MAX_TOOL_TURNS)
                    .await?
            }
            AgentProvider::Ollama(agent) => {
                agent
                    .prompt(&prompt)
                    .with_history(&mut history)
                    .max_turns(MAX_TOOL_TURNS)
                    .await?
            }
        };

        // 7. Extract intermediate tool messages from rig's modified history.
        //    rig appends: [prompt, assistant+tool_calls, user+tool_results, ..., final_assistant]
        //    We skip the prompt (already saved) and the final assistant (handled below).
        let new_messages = &history[history_len_before..];
        // Skip first (prompt added by rig) and last (final assistant response)
        if new_messages.len() > 2 {
            let intermediate = &new_messages[1..new_messages.len() - 1];
            for rig_msg in intermediate {
                let our_msgs = Self::rig_message_to_db_messages(rig_msg);
                for msg in our_msgs {
                    self.save_message_to_db(&msg).await?;
                    self.add_to_working_memory(msg).await;
                }
            }
        }

        // 8. Save final agent response to DB, then add to working memory.
        //    Skip if the response is empty (can happen when the agent's last
        //    turn was purely tool calls with no accompanying text).
        if !response.is_empty() {
            let agent_msg = Message {
                id: uuid::Uuid::new_v4().to_string(),
                role: "agent".to_string(),
                content: response.clone(),
                timestamp: Utc::now(),
                importance_score: None,
                metadata: None,
            };
            let agent_msg_id = agent_msg.id.clone();
            self.save_message_to_db(&agent_msg).await?;
            self.add_to_working_memory(agent_msg).await;

            // 9. Extract and store facts in long-term memory (background task)
            self.spawn_fact_extraction(user_message, &response, &user_msg_id, &agent_msg_id);
        } else {
            tracing::debug!("Skipping empty final agent message (all text was in tool-call turns)");
        }

        // Set completion attributes on parent span for Langfuse Input/Output display
        current_span.set_attribute("gen_ai.completion.0.role", "assistant");
        current_span.set_attribute("gen_ai.completion.0.content", response.clone());

        Ok(response)
    }

    /// Stream chat response with tool support.
    ///
    /// Creates an instrumented tracing span with Langfuse context attributes
    /// so that all LLM calls within this streaming turn are associated with the
    /// correct session, tags, and metadata in Langfuse.
    pub async fn stream_chat(
        &mut self,
        user_message: &str,
        callback: impl FnMut(String) + Send + 'static,
    ) -> Result<String> {
        let stream_span = tracing::info_span!(
            "ownai.stream_chat",
            instance_id = %self.instance_id,
            instance_name = %self.instance_name,
        );
        self.attach_langfuse_context(&stream_span);
        self.stream_chat_inner(user_message, callback)
            .instrument(stream_span)
            .await
    }

    /// Inner implementation of `stream_chat()`, executed within an instrumented span.
    async fn stream_chat_inner(
        &mut self,
        user_message: &str,
        mut callback: impl FnMut(String) + Send + 'static,
    ) -> Result<String> {
        // Set GenAI semantic convention attributes on the parent span so that
        // Langfuse can display Input/Output and render the flow diagram.
        let current_span = tracing::Span::current();
        current_span.set_attribute("gen_ai.operation.name", "chat");
        current_span.set_attribute("gen_ai.provider.name", self.provider_name.clone());
        current_span.set_attribute("gen_ai.request.model", self.model.clone());
        current_span.set_attribute("gen_ai.system_instructions", self.system_prompt.clone());
        current_span.set_attribute("gen_ai.prompt.0.role", "user");
        current_span.set_attribute("gen_ai.prompt.0.content", user_message.to_string());

        // 1. Save user message to DB first, then add to working memory
        let user_msg = Message {
            id: uuid::Uuid::new_v4().to_string(),
            role: "user".to_string(),
            content: user_message.to_string(),
            timestamp: Utc::now(),
            importance_score: None,
            metadata: None,
        };
        let user_msg_id = user_msg.id.clone();
        self.save_message_to_db(&user_msg).await?;
        self.add_to_working_memory(user_msg).await;

        // 2. Build context
        let context = self.context_builder.build_context(user_message).await?;

        // 3. Build chat history from working memory (with time gap markers)
        let history = self.build_history_with_time_markers();

        // 4. Prepare prompt with memory context
        let prompt = if !context.is_empty() {
            format!("[Context from memory]\n{}\n\n{}", context, user_message)
        } else {
            user_message.to_string()
        };

        // 5. Stream with multi-turn tool calling support
        let mut full_response = String::new();
        let mut final_response: Option<rig::agent::FinalResponse> = None;
        let mut intermediate_messages: Vec<Message> = Vec::new();

        match &self.agent {
            AgentProvider::Anthropic(agent) => {
                let mut stream = agent
                    .stream_chat(&prompt, history)
                    .multi_turn(MAX_TOOL_TURNS)
                    .await;
                process_stream!(
                    stream,
                    callback,
                    full_response,
                    final_response,
                    intermediate_messages
                );
            }
            AgentProvider::OpenAI(agent) => {
                let mut stream = agent
                    .stream_chat(&prompt, history)
                    .multi_turn(MAX_TOOL_TURNS)
                    .await;
                process_stream!(
                    stream,
                    callback,
                    full_response,
                    final_response,
                    intermediate_messages
                );
            }
            AgentProvider::Ollama(agent) => {
                let mut stream = agent
                    .stream_chat(&prompt, history)
                    .multi_turn(MAX_TOOL_TURNS)
                    .await;
                process_stream!(
                    stream,
                    callback,
                    full_response,
                    final_response,
                    intermediate_messages
                );
            }
        }

        // Record token usage from FinalResponse and persist to DB
        if let Some(ref res) = final_response {
            let usage = res.usage();
            current_span.set_attribute("gen_ai.usage.input_tokens", usage.input_tokens as i64);
            current_span.set_attribute("gen_ai.usage.output_tokens", usage.output_tokens as i64);

            // Persist token counts: input_tokens on user message, output_tokens on agent message
            if usage.input_tokens > 0 {
                Self::update_tokens_used(&self.db, &user_msg_id, usage.input_tokens as i64).await;
            }
        }

        // 6. Save intermediate tool messages (tool calls + results from multi-turn)
        for msg in intermediate_messages {
            self.save_message_to_db(&msg).await?;
            self.add_to_working_memory(msg).await;
        }

        // 7. Save agent response to DB first, then add to working memory.
        //    Skip if the final response is empty (can happen when the agent's
        //    last turn was purely tool calls with no accompanying text).
        let agent_msg_id = if !full_response.is_empty() {
            let agent_msg = Message {
                id: uuid::Uuid::new_v4().to_string(),
                role: "agent".to_string(),
                content: full_response.clone(),
                timestamp: Utc::now(),
                importance_score: None,
                metadata: None,
            };
            let id = agent_msg.id.clone();
            self.save_message_to_db(&agent_msg).await?;
            self.add_to_working_memory(agent_msg).await;
            Some(id)
        } else {
            tracing::debug!("Skipping empty final agent message (all text was in tool-call turns)");
            None
        };

        // Persist output_tokens on agent message
        if let (Some(ref res), Some(ref msg_id)) = (&final_response, &agent_msg_id) {
            let usage = res.usage();
            if usage.output_tokens > 0 {
                Self::update_tokens_used(&self.db, msg_id, usage.output_tokens as i64).await;
            }
        }

        // 8. Extract and store facts in long-term memory (background task)
        if let Some(ref agent_id) = agent_msg_id {
            self.spawn_fact_extraction(user_message, &full_response, &user_msg_id, agent_id);
        }

        // Set completion attributes on parent span for Langfuse Input/Output display
        current_span.set_attribute("gen_ai.completion.0.role", "assistant");
        current_span.set_attribute("gen_ai.completion.0.content", full_response.clone());

        Ok(full_response)
    }

    /// System prompt for ownAI -- includes identity, delegation instructions,
    /// and shared tool documentation from `base_tools_prompt()`.
    fn system_prompt(instance_name: &str) -> String {
        format!(
            r#"You are {name}, a personal AI agent that evolves with your user.

## Core Identity

You maintain a permanent, growing relationship with your user by:
- Remembering everything important across all conversations
- Learning and adapting to their preferences
- Proactively improving yourself by creating new capabilities
- Being helpful, concise, and honest

{tools}

## Task Delegation

You can delegate complex tasks to temporary sub-agents using the **delegate_task** tool.
Sub-agents work independently with their own context window and have access to all tools.

### When to Delegate
- A task requires many tool calls that would clutter the conversation
- A task is self-contained and can be described clearly
- You want to run a complex multi-step operation (e.g. research, code generation, file organization)

### How to Delegate
1. Call `delegate_task` with a short task name, a system prompt describing the sub-agent's role, and the task description
2. The sub-agent will execute the task and return a summary of what was done
3. You can then review the results and report back to the user

Tool documentation is automatically included for sub-agents -- you only need to provide a focused system prompt describing the sub-agent's role and approach.

## Memory System

You have access to:
- **Working Memory**: Recent messages in the current conversation
- **Long-term Memory**: Important facts retrieved via semantic search
- **Summaries**: Condensed older conversations

When you see "[Context from memory]" above a message, that information comes from previous conversations. Use it naturally.

## Response Guidelines

1. **Be conversational**: This is a continuous relationship, not isolated chats
2. **Use tools proactively**: Do not hesitate to use workspace, planning, or dynamic tools
3. **Be honest**: Admit when you do not know something
4. **Be adaptive**: Learn from user feedback and adjust your style
5. **Plan before acting**: For complex tasks, create a TODO list first
6. **Extend yourself**: When you lack a capability, consider creating a tool for it
7. **Delegate when appropriate**: For complex multi-step tasks, use delegate_task

Remember: You are building a long-term relationship with this user."#,
            name = instance_name,
            tools = base_tools_prompt(),
        )
    }

    /// Helper: Load recent messages from database for working memory initialization.
    /// Includes metadata column to restore tool call/result information.
    async fn load_recent_messages_from_db(db: &Pool<Sqlite>, limit: i32) -> Result<Vec<Message>> {
        let rows = sqlx::query(
            r#"
            SELECT * FROM (
                SELECT id, role, content, timestamp, importance_score, metadata
                FROM messages
                ORDER BY timestamp DESC
                LIMIT ?
            ) ORDER BY timestamp ASC
            "#,
        )
        .bind(limit)
        .fetch_all(db)
        .await
        .context("Failed to load recent messages")?;

        let messages: Vec<Message> = rows
            .into_iter()
            .map(|row| {
                let metadata_str: Option<String> = row.get("metadata");
                let metadata =
                    metadata_str.and_then(|s| serde_json::from_str::<MessageMetadata>(&s).ok());
                Message {
                    id: row.get("id"),
                    role: row.get("role"),
                    content: row.get("content"),
                    timestamp: row.get("timestamp"),
                    importance_score: row.get("importance_score"),
                    metadata,
                }
            })
            .collect();

        tracing::debug!(
            "Loaded {} messages from database for working memory",
            messages.len()
        );
        Ok(messages)
    }

    /// Helper: Save a Message to the database, including metadata as JSON.
    async fn save_message_to_db(&self, msg: &Message) -> Result<()> {
        let metadata_json = msg
            .metadata
            .as_ref()
            .and_then(|m| serde_json::to_string(m).ok());

        sqlx::query(
            "INSERT INTO messages (id, role, content, timestamp, metadata) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&msg.id)
        .bind(&msg.role)
        .bind(&msg.content)
        .bind(msg.timestamp)
        .bind(metadata_json)
        .execute(&self.db)
        .await
        .context("Failed to save message")?;

        Ok(())
    }

    /// Convert a rig Message (from history) into our DB Message(s).
    /// Assistant messages with tool calls -> one agent message with ToolCalls metadata.
    /// User messages with tool results -> one tool_result message per result.
    fn rig_message_to_db_messages(rig_msg: &RigMessage) -> Vec<Message> {
        match rig_msg {
            RigMessage::Assistant { content, .. } => {
                let mut text_parts = Vec::new();
                let mut tool_calls = Vec::new();

                for item in content.iter() {
                    match item {
                        AssistantContent::Text(t) => text_parts.push(t.text.clone()),
                        AssistantContent::ToolCall(tc) => {
                            tool_calls.push(ToolCallData {
                                id: tc.id.clone(),
                                call_id: tc.call_id.clone(),
                                name: tc.function.name.clone(),
                                arguments: tc.function.arguments.clone(),
                            });
                        }
                        _ => {}
                    }
                }

                let content_text = text_parts.join("");
                let metadata = if !tool_calls.is_empty() {
                    Some(MessageMetadata::ToolCalls { calls: tool_calls })
                } else {
                    None
                };

                vec![Message {
                    id: uuid::Uuid::new_v4().to_string(),
                    role: "agent".to_string(),
                    content: content_text,
                    timestamp: Utc::now(),
                    importance_score: None,
                    metadata,
                }]
            }
            RigMessage::User { content } => {
                let mut messages = Vec::new();

                for item in content.iter() {
                    if let UserContent::ToolResult(tr) = item {
                        let result_text = tr
                            .content
                            .iter()
                            .map(|c| match c {
                                RigToolResultContent::Text(t) => t.text.clone(),
                                _ => String::new(),
                            })
                            .collect::<Vec<_>>()
                            .join("");

                        messages.push(Message {
                            id: uuid::Uuid::new_v4().to_string(),
                            role: "tool_result".to_string(),
                            content: result_text,
                            timestamp: Utc::now(),
                            importance_score: None,
                            metadata: Some(MessageMetadata::ToolResult(ToolResultData {
                                tool_call_id: tr.id.clone(),
                                call_id: tr.call_id.clone(),
                            })),
                        });
                    }
                }

                messages
            }
        }
    }

    /// Convert a DB Message back to a rig Message for history reconstruction.
    /// Handles tool_calls metadata on agent messages and tool_result role messages
    /// to produce properly structured rig Messages that preserve the tool-use protocol.
    fn db_message_to_rig_message(msg: &Message) -> RigMessage {
        match msg.role.as_str() {
            "agent" => {
                if let Some(MessageMetadata::ToolCalls { ref calls }) = msg.metadata {
                    // Agent message with tool calls: reconstruct AssistantContent with ToolCall items
                    let mut content_items: Vec<AssistantContent> = Vec::new();
                    if !msg.content.is_empty() {
                        content_items.push(AssistantContent::Text(RigText {
                            text: msg.content.clone(),
                        }));
                    }
                    for call in calls {
                        content_items.push(AssistantContent::ToolCall(RigToolCall {
                            id: call.id.clone(),
                            call_id: call.call_id.clone(),
                            function: RigToolFunction {
                                name: call.name.clone(),
                                arguments: call.arguments.clone(),
                            },
                            signature: None,
                            additional_params: None,
                        }));
                    }
                    let mut content = OneOrMany::one(content_items.remove(0));
                    for item in content_items {
                        content.push(item);
                    }
                    RigMessage::Assistant { id: None, content }
                } else {
                    // Plain text agent message
                    RigMessage::assistant(&msg.content)
                }
            }
            "tool_result" => {
                if let Some(MessageMetadata::ToolResult(ref tr_data)) = msg.metadata {
                    RigMessage::User {
                        content: OneOrMany::one(UserContent::ToolResult(RigToolResult {
                            id: tr_data.tool_call_id.clone(),
                            call_id: tr_data.call_id.clone(),
                            content: OneOrMany::one(RigToolResultContent::Text(RigText {
                                text: msg.content.clone(),
                            })),
                        })),
                    }
                } else {
                    // Fallback: treat as user message
                    RigMessage::user(&msg.content)
                }
            }
            _ => {
                // "user" or any other role
                RigMessage::user(&msg.content)
            }
        }
    }

    /// Public mutable accessor for context builder (used by memory commands)
    pub fn context_builder_mut(&mut self) -> &mut ContextBuilder {
        &mut self.context_builder
    }

    /// Public accessor for the Rhai tool registry (used by tool commands)
    pub fn tool_registry(&self) -> &SharedRegistry {
        &self.tool_registry
    }

    /// Helper: Update tokens_used on a message in the database.
    /// Used after streaming to persist LLM token usage (input_tokens on user
    /// message, output_tokens on agent message). Logs errors but does not fail.
    async fn update_tokens_used(db: &Pool<Sqlite>, message_id: &str, tokens: i64) {
        if let Err(e) = sqlx::query("UPDATE messages SET tokens_used = ? WHERE id = ?")
            .bind(tokens)
            .bind(message_id)
            .execute(db)
            .await
        {
            tracing::warn!(
                "Failed to update tokens_used for message {}: {}",
                message_id,
                e
            );
        }
    }

    /// Helper: Update importance_score on a message in the database.
    /// Called from fact extraction background task with the max importance
    /// of all extracted facts. Logs errors but does not fail.
    async fn update_importance_score(db: &Pool<Sqlite>, message_id: &str, score: f32) {
        if let Err(e) = sqlx::query("UPDATE messages SET importance_score = ? WHERE id = ?")
            .bind(score)
            .bind(message_id)
            .execute(db)
            .await
        {
            tracing::warn!(
                "Failed to update importance_score for message {}: {}",
                message_id,
                e
            );
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
    fn spawn_fact_extraction(
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
