use anyhow::{Context, Result};
use chrono::Utc;
use futures::StreamExt;
use rig::agent::{Agent, MultiTurnStreamItem};
use rig::client::{CompletionClient, Nothing};
use rig::completion::Prompt;
use rig::message::Message as RigMessage;
use rig::providers::{anthropic, ollama, openai};
use rig::streaming::{StreamedAssistantContent, StreamingChat};
use rig::tool::ToolDyn;
use sqlx::{Pool, Sqlite};
use std::path::PathBuf;

use crate::ai_instances::{AIInstance, APIKeyStorage, LLMProvider};
use crate::memory::{
    working_memory::Message, ContextBuilder, LongTermMemory, SummarizationAgent, WorkingMemory,
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
    let workspace = paths::get_instance_workspace_path(instance_id)
        .unwrap_or_else(|_| PathBuf::from("."));

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

/// OwnAI Agent with Memory, Tools, and LLM integration
pub struct OwnAIAgent {
    agent: AgentProvider,
    context_builder: ContextBuilder,
    db: Pool<Sqlite>,
    #[allow(dead_code)]
    todo_list: SharedTodoList,
}

/// Maximum number of multi-turn iterations for tool calling
const MAX_TOOL_TURNS: usize = 50;

impl OwnAIAgent {
    /// Create a new OwnAI Agent with tools
    pub async fn new(instance: &AIInstance, db: Pool<Sqlite>) -> Result<Self> {
        // Initialize Memory System components
        let working_memory = WorkingMemory::new(50_000); // 50k tokens
        let long_term_memory = LongTermMemory::new(db.clone()).await?;
        let summarization_agent = SummarizationAgent::new(db.clone());

        // Initialize table if needed
        summarization_agent.init_table().await?;

        let context_builder = ContextBuilder::new(
            working_memory,
            long_term_memory,
            summarization_agent,
            db.clone(),
        );

        // Create shared TODO list state
        let todo_list = planning::create_shared_todo_list();

        // Load API key from keychain if needed
        let api_key = if instance.provider.needs_api_key() {
            APIKeyStorage::load(&instance.provider)?
                .ok_or_else(|| anyhow::anyhow!("API key not found for provider: {}", instance.provider))?
        } else {
            String::new()
        };

        let system_prompt = Self::system_prompt(&instance.name);

        // Create provider-specific agent with tools
        let agent = match instance.provider {
            LLMProvider::Anthropic => {
                let client = anthropic::Client::builder()
                    .api_key(&api_key)
                    .build()?;

                let tools = create_tools(&instance.id, todo_list.clone());

                let agent = client
                    .agent(&instance.model)
                    .preamble(&system_prompt)
                    .temperature(0.7)
                    .tools(tools)
                    .build();

                AgentProvider::Anthropic(agent)
            }

            LLMProvider::OpenAI => {
                let openai_client = openai::Client::builder()
                    .api_key(&api_key)
                    .build()?;

                let tools = create_tools(&instance.id, todo_list.clone());

                let agent = openai_client
                    .completions_api()
                    .agent(&instance.model)
                    .preamble(&system_prompt)
                    .temperature(0.7)
                    .tools(tools)
                    .build();

                AgentProvider::OpenAI(agent)
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

                AgentProvider::Ollama(agent)
            }
        };

        Ok(OwnAIAgent {
            agent,
            context_builder,
            db,
            todo_list,
        })
    }

    /// Main chat method (non-streaming) - combines Memory + Tools + LLM
    pub async fn chat(&mut self, user_message: &str) -> Result<String> {
        // 1. Add user message to working memory
        self.context_builder
            .working_memory_mut()
            .add_message(Message {
                id: uuid::Uuid::new_v4().to_string(),
                role: "user".to_string(),
                content: user_message.to_string(),
                timestamp: Utc::now(),
            });

        // 2. Build context from all memory layers
        let context = self.context_builder.build_context(user_message).await?;

        // 3. Build chat history from working memory
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

        // 4. Prepare prompt with memory context
        let prompt = if !context.is_empty() {
            format!("[Context from memory]\n{}\n\n{}", context, user_message)
        } else {
            user_message.to_string()
        };

        // 5. Call LLM with multi-turn tool support
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

        // 6. Add agent response to working memory
        self.context_builder
            .working_memory_mut()
            .add_message(Message {
                id: uuid::Uuid::new_v4().to_string(),
                role: "agent".to_string(),
                content: response.clone(),
                timestamp: Utc::now(),
            });

        // 7. Save to database
        self.save_message("user", user_message).await?;
        self.save_message("agent", &response).await?;

        Ok(response)
    }

    /// Stream chat response with tool support
    pub async fn stream_chat(
        &mut self,
        user_message: &str,
        mut callback: impl FnMut(String) + Send + 'static,
    ) -> Result<String> {
        // 1. Add user message to working memory
        self.context_builder
            .working_memory_mut()
            .add_message(Message {
                id: uuid::Uuid::new_v4().to_string(),
                role: "user".to_string(),
                content: user_message.to_string(),
                timestamp: Utc::now(),
            });

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

        // 6. Add response to memory
        self.context_builder
            .working_memory_mut()
            .add_message(Message {
                id: uuid::Uuid::new_v4().to_string(),
                role: "agent".to_string(),
                content: full_response.clone(),
                timestamp: Utc::now(),
            });

        // 7. Save to database
        self.save_message("user", user_message).await?;
        self.save_message("agent", &full_response).await?;

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

    /// Helper: Save message to database
    async fn save_message(&self, role: &str, content: &str) -> Result<()> {
        sqlx::query(
            "INSERT INTO messages (id, role, content, timestamp) 
             VALUES (?, ?, ?, ?)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(role)
        .bind(content)
        .bind(Utc::now())
        .execute(&self.db)
        .await
        .context("Failed to save message")?;

        Ok(())
    }
}
