use anyhow::{Context, Result};
use chrono::Utc;
use futures::StreamExt;
use rig::agent::{Agent, MultiTurnStreamItem};
use rig::client::{CompletionClient, Nothing};
use rig::completion::Prompt;
use rig::extractor::Extractor;
use rig::message::Message as RigMessage;
use rig::providers::{anthropic, ollama, openai};
use rig::streaming::{StreamedAssistantContent, StreamingChat};
use rig::tool::ToolDyn;
use sqlx::{Pool, Row, Sqlite};
use std::path::PathBuf;

use crate::ai_instances::{AIInstance, APIKeyStorage, LLMProvider};
use crate::memory::{
    working_memory::Message, ContextBuilder, LongTermMemory, SessionSummary, SummarizationAgent,
    SummaryResponse, WorkingMemory,
};
use crate::tools::filesystem::{EditFileTool, GrepTool, LsTool, ReadFileTool, WriteFileTool};
use crate::tools::planning::{self, SharedTodoList, WriteTodosTool};
use crate::utils::paths;

/// Macro to process streaming responses uniformly across providers.
/// Handles both text chunks and multi-turn tool call items.
macro_rules! process_stream {
    ($stream:expr, $callback:expr, $full_response:expr) => {
        while let Some(result) = $stream.next().await {
            match result {
                Ok(item) => match item {
                    MultiTurnStreamItem::StreamAssistantItem(content) => match content {
                        StreamedAssistantContent::Text(text) => {
                            $callback(text.text.clone());
                            $full_response.push_str(&text.text);
                        }
                        StreamedAssistantContent::Final(_) => {}
                        _ => {} // Ignore other content types (tool use markers, etc.)
                    },
                    MultiTurnStreamItem::StreamUserItem(_) => {
                        // Tool result sent back (multi-turn); stream continues
                    }
                    MultiTurnStreamItem::FinalResponse(_) => {
                        // Final aggregated response; stream is done
                    }
                    _ => {} // Future variants
                },
                Err(e) => {
                    return Err(anyhow::anyhow!("Streaming error: {}", e));
                }
            }
        }
    };
}

/// Helper: Create the set of tools for an instance
fn create_tools(instance_id: &str, todo_list: SharedTodoList) -> Vec<Box<dyn ToolDyn>> {
    let workspace =
        paths::get_instance_workspace_path(instance_id).unwrap_or_else(|_| PathBuf::from("."));

    let tools: Vec<Box<dyn ToolDyn>> = vec![
        // Filesystem tools
        Box::new(LsTool::new(workspace.clone())),
        Box::new(ReadFileTool::new(workspace.clone())),
        Box::new(WriteFileTool::new(workspace.clone())),
        Box::new(EditFileTool::new(workspace.clone())),
        Box::new(GrepTool::new(workspace)),
        // Planning tool
        Box::new(WriteTodosTool::new(todo_list)),
    ];

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

impl SummaryExtractorProvider {
    /// Extract a structured summary from the given text
    async fn extract(&self, text: &str) -> Result<SummaryResponse> {
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
    summary_extractor: SummaryExtractorProvider,
    context_builder: ContextBuilder,
    db: Pool<Sqlite>,
    #[allow(dead_code)]
    todo_list: SharedTodoList,
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
    ) -> Result<Self> {
        // Initialize Memory System components
        let mut working_memory = WorkingMemory::new(max_tokens.unwrap_or(50_000));
        let long_term_memory = LongTermMemory::new(db.clone()).await?;
        let summarization_agent = SummarizationAgent::new(db.clone());

        // Initialize table if needed
        summarization_agent.init_table().await?;

        // Load recent messages from database into working memory
        let recent_messages = Self::load_recent_messages_from_db(&db, 100).await?;
        if !recent_messages.is_empty() {
            working_memory.load_from_messages(recent_messages);
        }

        let context_builder =
            ContextBuilder::new(working_memory, long_term_memory, summarization_agent);

        // Create shared TODO list state
        let todo_list = planning::create_shared_todo_list();

        // Load API key from keychain if needed
        let api_key = if instance.provider.needs_api_key() {
            APIKeyStorage::load(&instance.provider)?.ok_or_else(|| {
                anyhow::anyhow!("API key not found for provider: {}", instance.provider)
            })?
        } else {
            String::new()
        };

        let system_prompt = Self::system_prompt(&instance.name);
        let extractor_preamble = "Extract a structured summary from the conversation below. \
            Identify the key facts discussed, any tools that were used or mentioned, \
            and the main topics covered. Be concise but thorough.";

        // Create provider-specific agent with tools and summary extractor
        let (agent, summary_extractor) = match instance.provider {
            LLMProvider::Anthropic => {
                let client = anthropic::Client::builder().api_key(&api_key).build()?;

                let tools = create_tools(&instance.id, todo_list.clone());

                let agent = client
                    .agent(&instance.model)
                    .preamble(&system_prompt)
                    .max_tokens(32768)
                    .temperature(0.7)
                    .tools(tools)
                    .build();

                let extractor = client
                    .extractor::<SummaryResponse>(&instance.model)
                    .preamble(extractor_preamble)
                    .max_tokens(8192)
                    .build();

                (
                    AgentProvider::Anthropic(agent),
                    SummaryExtractorProvider::Anthropic(extractor),
                )
            }

            LLMProvider::OpenAI => {
                let openai_client = openai::Client::builder().api_key(&api_key).build()?;

                let tools = create_tools(&instance.id, todo_list.clone());

                let agent = openai_client
                    .clone()
                    .completions_api()
                    .agent(&instance.model)
                    .preamble(&system_prompt)
                    .temperature(0.7)
                    .tools(tools)
                    .build();

                let extractor = openai_client
                    .completions_api()
                    .extractor::<SummaryResponse>(&instance.model)
                    .preamble(extractor_preamble)
                    .build();

                (
                    AgentProvider::OpenAI(agent),
                    SummaryExtractorProvider::OpenAI(extractor),
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

                let tools = create_tools(&instance.id, todo_list.clone());

                let agent = ollama_client
                    .agent(&instance.model)
                    .preamble(&system_prompt)
                    .tools(tools)
                    .build();

                let extractor = ollama_client
                    .extractor::<SummaryResponse>(&instance.model)
                    .preamble(extractor_preamble)
                    .build();

                (
                    AgentProvider::Ollama(agent),
                    SummaryExtractorProvider::Ollama(extractor),
                )
            }
        };

        Ok(OwnAIAgent {
            agent,
            summary_extractor,
            context_builder,
            db,
            todo_list,
        })
    }

    /// Public accessor for context builder (used by memory stats command)
    pub fn context_builder(&self) -> &ContextBuilder {
        &self.context_builder
    }

    /// Summarize evicted messages using the LLM extractor and save to database.
    /// Called automatically when working memory exceeds its token budget.
    async fn summarize_evicted(&self, evicted: Vec<Message>) -> Result<()> {
        if evicted.is_empty() {
            return Ok(());
        }

        tracing::info!(
            "Summarizing {} evicted messages via LLM extractor",
            evicted.len()
        );

        // Format messages for the extractor
        let conversation_text = evicted
            .iter()
            .map(|m| format!("{}: {}", m.role, m.content))
            .collect::<Vec<_>>()
            .join("\n");

        // Use rig Extractor for type-safe structured output
        let extracted = self.summary_extractor.extract(&conversation_text).await?;

        // Build SessionSummary from extracted data
        let summary = SessionSummary {
            id: uuid::Uuid::new_v4().to_string(),
            start_message_id: evicted.first().unwrap().id.clone(),
            end_message_id: evicted.last().unwrap().id.clone(),
            summary_text: extracted.summary,
            key_facts: extracted.key_facts,
            tools_mentioned: extracted.tools_used,
            topics: extracted.topics,
            timestamp: Utc::now(),
            token_savings: evicted
                .iter()
                .map(|m| (m.content.len() + m.role.len()) / 4)
                .sum(),
        };

        // Save summary and link messages
        let summarization_agent = self.context_builder.summarization_agent();
        summarization_agent.save_summary(&summary).await?;

        let message_ids: Vec<String> = evicted.iter().map(|m| m.id.clone()).collect();
        summarization_agent
            .link_messages_to_summary(&message_ids, &summary.id)
            .await?;

        tracing::info!(
            "Saved LLM summary {} for {} messages (saved ~{} tokens)",
            summary.id,
            evicted.len(),
            summary.token_savings
        );

        Ok(())
    }

    /// Add a message to working memory; if eviction occurs, summarize in background
    async fn add_to_working_memory(&mut self, msg: Message) {
        if let Some(evicted) = self.context_builder.working_memory_mut().add_message(msg) {
            if let Err(e) = self.summarize_evicted(evicted).await {
                tracing::warn!("Failed to summarize evicted messages: {}", e);
            }
        }
    }

    /// Main chat method (non-streaming) - combines Memory + Tools + LLM
    pub async fn chat(&mut self, user_message: &str) -> Result<String> {
        // 1. Create user message, save to DB first (so FK constraints work for summaries)
        let user_msg = Message {
            id: uuid::Uuid::new_v4().to_string(),
            role: "user".to_string(),
            content: user_message.to_string(),
            timestamp: Utc::now(),
            importance_score: 0.5,
        };
        self.save_message_with_id(&user_msg.id, &user_msg.role, &user_msg.content)
            .await?;

        // 2. Add to working memory (may trigger eviction -> summarization)
        self.add_to_working_memory(user_msg).await;

        // 3. Build context from all memory layers
        let context = self.context_builder.build_context(user_message).await?;

        // 4. Build chat history from working memory
        let messages = self.context_builder.working_memory().get_context();
        let mut history: Vec<RigMessage> = messages
            .iter()
            .take(messages.len().saturating_sub(1))
            .map(|msg| {
                if msg.role == "user" {
                    RigMessage::user(&msg.content)
                } else {
                    RigMessage::assistant(&msg.content)
                }
            })
            .collect();

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

        // 7. Save agent response to DB first, then add to working memory
        let agent_msg = Message {
            id: uuid::Uuid::new_v4().to_string(),
            role: "agent".to_string(),
            content: response.clone(),
            timestamp: Utc::now(),
            importance_score: 0.5,
        };
        self.save_message_with_id(&agent_msg.id, &agent_msg.role, &agent_msg.content)
            .await?;
        self.add_to_working_memory(agent_msg).await;

        Ok(response)
    }

    /// Stream chat response with tool support
    pub async fn stream_chat(
        &mut self,
        user_message: &str,
        mut callback: impl FnMut(String) + Send + 'static,
    ) -> Result<String> {
        // 1. Save user message to DB first, then add to working memory
        let user_msg = Message {
            id: uuid::Uuid::new_v4().to_string(),
            role: "user".to_string(),
            content: user_message.to_string(),
            timestamp: Utc::now(),
            importance_score: 0.5,
        };
        self.save_message_with_id(&user_msg.id, &user_msg.role, &user_msg.content)
            .await?;
        self.add_to_working_memory(user_msg).await;

        // 2. Build context
        let context = self.context_builder.build_context(user_message).await?;

        // 3. Convert working memory to rig messages for chat history
        let messages = self.context_builder.working_memory().get_context();
        let history: Vec<RigMessage> = messages
            .iter()
            .take(messages.len().saturating_sub(1))
            .map(|msg| {
                if msg.role == "user" {
                    RigMessage::user(&msg.content)
                } else {
                    RigMessage::assistant(&msg.content)
                }
            })
            .collect();

        // 4. Prepare prompt with memory context
        let prompt = if !context.is_empty() {
            format!("[Context from memory]\n{}\n\n{}", context, user_message)
        } else {
            user_message.to_string()
        };

        // 5. Stream with multi-turn tool calling support
        let mut full_response = String::new();

        match &self.agent {
            AgentProvider::Anthropic(agent) => {
                let mut stream = agent
                    .stream_chat(&prompt, history)
                    .multi_turn(MAX_TOOL_TURNS)
                    .await;
                process_stream!(stream, callback, full_response);
            }
            AgentProvider::OpenAI(agent) => {
                let mut stream = agent
                    .stream_chat(&prompt, history)
                    .multi_turn(MAX_TOOL_TURNS)
                    .await;
                process_stream!(stream, callback, full_response);
            }
            AgentProvider::Ollama(agent) => {
                let mut stream = agent
                    .stream_chat(&prompt, history)
                    .multi_turn(MAX_TOOL_TURNS)
                    .await;
                process_stream!(stream, callback, full_response);
            }
        }

        // 6. Save agent response to DB first, then add to working memory
        let agent_msg = Message {
            id: uuid::Uuid::new_v4().to_string(),
            role: "agent".to_string(),
            content: full_response.clone(),
            timestamp: Utc::now(),
            importance_score: 0.5,
        };
        self.save_message_with_id(&agent_msg.id, &agent_msg.role, &agent_msg.content)
            .await?;
        self.add_to_working_memory(agent_msg).await;

        Ok(full_response)
    }

    /// System prompt for ownAI -- includes tool usage instructions
    fn system_prompt(instance_name: &str) -> String {
        format!(
            r#"You are {name}, a personal AI agent that evolves with your user.

## Core Identity

You maintain a permanent, growing relationship with your user by:
- Remembering everything important across all conversations
- Learning and adapting to their preferences
- Proactively improving yourself by creating new capabilities
- Being helpful, concise, and honest

## Available Tools

You have access to the following tools:

### Filesystem (Workspace)
- **ls**: List files and directories in your workspace
- **read_file**: Read file contents (supports line ranges)
- **write_file**: Write content to a file (creates dirs if needed)
- **edit_file**: Replace text in a file (old_text -> new_text)
- **grep**: Search for text patterns in files

Use the workspace to:
- Save research results, notes, or data
- Create and manage files for the user
- Offload large information from context

### Planning
- **write_todos**: Create/update a TODO list for multi-step tasks

Use write_todos when:
- A task requires more than 2-3 steps
- You need to track progress on complex work
- You discover new requirements mid-task

## Memory System

You have access to:
- **Working Memory**: Recent messages in the current conversation
- **Long-term Memory**: Important facts retrieved via semantic search
- **Summaries**: Condensed older conversations

When you see "[Context from memory]" above a message, that information
comes from previous conversations. Use it naturally.

## Response Guidelines

1. **Be conversational**: This is a continuous relationship, not isolated chats
2. **Use tools proactively**: Don't hesitate to use workspace or planning tools
3. **Be honest**: Admit when you don't know something
4. **Be adaptive**: Learn from user feedback and adjust your style
5. **Plan before acting**: For complex tasks, create a TODO list first

Remember: You are building a long-term relationship with this user."#,
            name = instance_name
        )
    }

    /// Helper: Load recent messages from database for working memory initialization
    async fn load_recent_messages_from_db(db: &Pool<Sqlite>, limit: i32) -> Result<Vec<Message>> {
        let rows = sqlx::query(
            r#"
            SELECT id, role, content, timestamp, COALESCE(importance_score, 0.5) as importance_score
            FROM messages
            ORDER BY timestamp ASC
            LIMIT ?
            "#,
        )
        .bind(limit)
        .fetch_all(db)
        .await
        .context("Failed to load recent messages")?;

        let messages: Vec<Message> = rows
            .into_iter()
            .map(|row| Message {
                id: row.get("id"),
                role: row.get("role"),
                content: row.get("content"),
                timestamp: row.get("timestamp"),
                importance_score: row.get("importance_score"),
            })
            .collect();

        tracing::debug!(
            "Loaded {} messages from database for working memory",
            messages.len()
        );
        Ok(messages)
    }

    /// Helper: Save message to database with a specific ID.
    /// This ensures the same ID is used in both the DB and working memory,
    /// so that FOREIGN KEY constraints in summaries work correctly.
    async fn save_message_with_id(&self, id: &str, role: &str, content: &str) -> Result<()> {
        sqlx::query(
            "INSERT INTO messages (id, role, content, timestamp) 
             VALUES (?, ?, ?, ?)",
        )
        .bind(id)
        .bind(role)
        .bind(content)
        .bind(Utc::now())
        .execute(&self.db)
        .await
        .context("Failed to save message")?;

        Ok(())
    }
}
