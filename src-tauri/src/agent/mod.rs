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
use tauri::AppHandle;

use crate::ai_instances::{AIInstance, APIKeyStorage, LLMProvider};
use crate::canvas::tools::{
    CreateProgramTool, ListProgramsTool, OpenProgramTool, ProgramEditFileTool, ProgramLsTool,
    ProgramReadFileTool, ProgramWriteFileTool,
};
use crate::memory::{
    fact_extraction, working_memory::Message, ContextBuilder, FactExtractionResponse,
    LongTermMemory, SessionSummary, SummarizationAgent, SummaryResponse, WorkingMemory,
};
use crate::tools::code_generation::{CreateToolTool, ReadToolTool, UpdateToolTool};
use crate::tools::filesystem::{EditFileTool, GrepTool, LsTool, ReadFileTool, WriteFileTool};
use crate::tools::planning::{self, SharedTodoList, WriteTodosTool};
use crate::tools::registry::RhaiToolRegistry;
use crate::tools::rhai_bridge_tool::{RhaiExecuteTool, SharedRegistry};
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
fn create_tools(
    instance_id: &str,
    todo_list: SharedTodoList,
    registry: SharedRegistry,
    available_dynamic_tools: Vec<(String, String)>,
    db: Pool<Sqlite>,
    programs_root: PathBuf,
    app_handle: Option<AppHandle>,
) -> Vec<Box<dyn ToolDyn>> {
    let workspace =
        paths::get_instance_workspace_path(instance_id).unwrap_or_else(|_| PathBuf::from("."));

    let tools: Vec<Box<dyn ToolDyn>> = vec![
        // Filesystem tools
        Box::new(LsTool::new(workspace.clone())),
        Box::new(ReadFileTool::new(workspace.clone())),
        Box::new(WriteFileTool::new(workspace.clone())),
        Box::new(EditFileTool::new(workspace.clone())),
        Box::new(GrepTool::new(workspace.clone())),
        // Planning tool
        Box::new(WriteTodosTool::new(todo_list)),
        // Dynamic Rhai tool executor
        Box::new(RhaiExecuteTool::new(
            registry.clone(),
            available_dynamic_tools,
        )),
        // Self-programming: create, read, and update dynamic tools
        Box::new(CreateToolTool::new(registry.clone(), workspace.clone())),
        Box::new(ReadToolTool::new(registry.clone())),
        Box::new(UpdateToolTool::new(registry, workspace)),
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
            db,
            instance_id.to_string(),
            programs_root,
            app_handle,
        )),
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
    summary_extractor: SummaryExtractorProvider,
    fact_extractor: FactExtractorProvider,
    context_builder: ContextBuilder,
    db: Pool<Sqlite>,
    #[allow(dead_code)]
    todo_list: SharedTodoList,
    tool_registry: SharedRegistry,
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
        let rhai_registry = RhaiToolRegistry::new(db.clone(), workspace);
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

                let tools = create_tools(
                    &instance.id,
                    todo_list.clone(),
                    tool_registry.clone(),
                    available_dynamic_tools.clone(),
                    db.clone(),
                    programs_root.clone(),
                    app_handle.clone(),
                );

                let agent = client
                    .agent(&instance.model)
                    .preamble(&system_prompt)
                    .max_tokens(32768)
                    .temperature(0.7)
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

                let tools = create_tools(
                    &instance.id,
                    todo_list.clone(),
                    tool_registry.clone(),
                    available_dynamic_tools.clone(),
                    db.clone(),
                    programs_root.clone(),
                    app_handle.clone(),
                );

                let agent = openai_client
                    .clone()
                    .completions_api()
                    .agent(&instance.model)
                    .preamble(&system_prompt)
                    .temperature(0.7)
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

                let tools = create_tools(
                    &instance.id,
                    todo_list.clone(),
                    tool_registry.clone(),
                    available_dynamic_tools.clone(),
                    db.clone(),
                    programs_root.clone(),
                    app_handle,
                );

                let agent = ollama_client
                    .clone()
                    .agent(&instance.model)
                    .preamble(&system_prompt)
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

        Ok(OwnAIAgent {
            agent,
            summary_extractor,
            fact_extractor,
            context_builder,
            db,
            todo_list,
            tool_registry,
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
        let agent_msg_id = agent_msg.id.clone();
        self.save_message_with_id(&agent_msg.id, &agent_msg.role, &agent_msg.content)
            .await?;
        self.add_to_working_memory(agent_msg).await;

        // 8. Extract and store facts in long-term memory (non-blocking)
        self.extract_and_store_facts(user_message, &response, &agent_msg_id)
            .await;

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
        let agent_msg_id = agent_msg.id.clone();
        self.save_message_with_id(&agent_msg.id, &agent_msg.role, &agent_msg.content)
            .await?;
        self.add_to_working_memory(agent_msg).await;

        // 7. Extract and store facts in long-term memory (non-blocking)
        self.extract_and_store_facts(user_message, &full_response, &agent_msg_id)
            .await;

        Ok(full_response)
    }

    /// System prompt for ownAI -- includes tool usage and self-programming instructions
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

### Dynamic Tool Execution
- **execute_dynamic_tool**: Run a previously created dynamic tool by name

### Self-Programming (Tool Management)
- **create_tool**: Create a new dynamic tool from Rhai script code
- **read_tool**: Read the source code and metadata of an existing tool
- **update_tool**: Update/fix an existing tool's Rhai script code

## Self-Programming

You can extend your own capabilities by creating dynamic tools written in Rhai (a lightweight scripting language). This is one of your most powerful features.

### When to Create a Tool
- A task requires calling external APIs (HTTP requests)
- Data processing that your built-in tools do not cover
- Recurring tasks that benefit from automation
- The user explicitly asks you to create a new capability
- You need to combine multiple operations into a single reusable step

### How to Create a Tool
1. Analyze what the tool needs to do
2. Write a Rhai script (see language reference below)
3. Call `create_tool` with a name, description, the script, and parameter definitions
4. Test the tool with `execute_dynamic_tool`
5. If it does not work correctly, use `read_tool` to inspect the code, then `update_tool` to fix it

### Iterating on Tools
If a dynamic tool produces unexpected results or errors:
1. Call `read_tool` to see the current source code and usage stats
2. Identify the issue in the Rhai script
3. Call `update_tool` with the corrected code
4. Re-test with `execute_dynamic_tool`

You can also improve existing tools by adding error handling, extending functionality, or optimizing performance.

### Rhai Language Reference

Scripts receive parameters through the `params_json` scope variable:
```
let params = json_parse(params_json);
let value = params["key"];
```

Available built-in functions:
- **http_get(url)**: HTTPS GET request, returns response body
- **http_post(url, body)**: HTTPS POST with JSON body
- **http_request(method, url, headers, body)**: Flexible HTTP with custom method/headers
- **read_file(path)**: Read file from workspace
- **write_file(path, content)**: Write file to workspace
- **json_parse(text)**: Parse JSON string to object/array
- **json_stringify(value)**: Convert value to JSON string
- **regex_match(text, pattern)**: Find all regex matches
- **regex_replace(text, pattern, replacement)**: Replace regex matches
- **base64_encode(text)**: Encode string to Base64
- **base64_decode(text)**: Decode Base64 to string
- **url_encode(text)**: URL-encode a string
- **get_current_datetime()**: Get current UTC datetime (ISO 8601)
- **send_notification(title, body)**: Queue a system notification

Security constraints:
- All HTTP requests must use HTTPS
- File operations are restricted to the workspace directory
- Scripts are terminated after 100,000 operations (prevents infinite loops)

### Example: Creating a Simple Tool
To create a tool that fetches a URL and extracts the title:
1. Call create_tool with name="fetch_title", description="Fetches a URL and returns the page title"
2. Script: `let params = json_parse(params_json); let body = http_get(params["url"]); let matches = regex_match(body, "<title>(.*?)</title>"); if matches.len() > 0 {{ matches[0] }} else {{ "No title found" }}`
3. Parameters: [{{"name": "url", "type_hint": "string", "description": "URL to fetch", "required": true}}]

## Canvas Programs (Visual Apps)

You can create interactive HTML/CSS/JS applications that the user can see and interact with in an embedded view. These are called "Programs."

### Canvas Tools
- **create_program**: Create a new program with an initial index.html
- **list_programs**: List all programs you have created
- **open_program**: Open an existing program in the Canvas panel for the user to see
- **program_ls**: List files within a program directory
- **program_read_file**: Read the contents of a file in a program
- **program_write_file**: Write/create a file in a program (bumps version, auto-reloads in frontend)
- **program_edit_file**: Edit a file with search/replace (bumps version, auto-reloads in frontend)

### When to Use an Existing Program
IMPORTANT: Before creating a new program, always call `list_programs` first to check if a suitable
program already exists. If the user asks for something that matches an existing program (e.g. "Let's
play chess" and a "chess-board" program exists), use `open_program` to display it instead of creating
a new one. You can then use `program_edit_file` or `program_write_file` to modify it if needed.

### When to Create a Program
- The user needs a visual interface (dashboard, form, game, chart)
- A task benefits from interactive HTML rather than plain text
- The user asks you to "show", "display", or "build" something visual
- No suitable existing program was found via `list_programs`
- Examples: chess board, expense tracker, data dashboard, quiz app

### How to Create a Program
1. Call `create_program` with a descriptive name, description, and initial HTML
2. The HTML should be a complete, self-contained page (inline CSS/JS or use separate files)
3. Use `program_write_file` to add CSS, JavaScript, or other files
4. Use `program_edit_file` for targeted modifications to existing files
5. Use `program_read_file` to inspect current file contents before editing

### Best Practices
- Use semantic, lowercase names with hyphens (e.g. "expense-tracker", "chess-board")
- Start with a working index.html, then iterate
- For complex apps, separate HTML, CSS, and JS into different files
- Each write or edit automatically increments the program version
- The user can view the program in a Canvas iframe beside the chat

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

    /// Public mutable accessor for context builder (used by memory commands)
    pub fn context_builder_mut(&mut self) -> &mut ContextBuilder {
        &mut self.context_builder
    }

    /// Public accessor for the Rhai tool registry (used by tool commands)
    pub fn tool_registry(&self) -> &SharedRegistry {
        &self.tool_registry
    }

    /// Extract facts from a conversation turn and store them in long-term memory.
    /// Called after each agent response to build up semantic knowledge over time.
    async fn extract_and_store_facts(
        &mut self,
        user_message: &str,
        agent_response: &str,
        agent_msg_id: &str,
    ) {
        // Format conversation turn for extraction
        let conversation_turn = format!("User: {}\nAgent: {}", user_message, agent_response);

        // Extract facts using LLM
        match self.fact_extractor.extract(&conversation_turn).await {
            Ok(extraction) => {
                if extraction.facts.is_empty() {
                    tracing::debug!("No facts extracted from conversation turn");
                    return;
                }

                tracing::info!(
                    "Extracted {} facts from conversation",
                    extraction.facts.len()
                );

                // Convert extracted facts to memory entries and store them
                let long_term_memory = self.context_builder.long_term_memory_mut();

                for fact_item in extraction.facts {
                    let entry = fact_extraction::to_memory_entry(fact_item, agent_msg_id);

                    if let Err(e) = long_term_memory.store(entry).await {
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
}
