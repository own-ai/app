use anyhow::{Context, Result};
use chrono::Utc;
use futures::StreamExt;
use rig::agent::{Agent, MultiTurnStreamItem};
use rig::client::{CompletionClient, Nothing};
use rig::completion::Prompt;
use rig::message::Message as RigMessage;
use rig::providers::{anthropic, ollama, openai};
use rig::streaming::{StreamedAssistantContent, StreamingChat};
use sqlx::{Pool, Sqlite};

use crate::ai_instances::{AIInstance, APIKeyStorage, LLMProvider};
use crate::memory::{
    working_memory::Message, ContextBuilder, LongTermMemory, SummarizationAgent, WorkingMemory,
};

/// Type aliases for each provider's agent type
type AnthropicAgent = Agent<anthropic::completion::CompletionModel>;
type OpenAIAgent = Agent<openai::CompletionModel>;
type OllamaAgent = Agent<ollama::CompletionModel>;

/// Macro to process streaming responses uniformly across providers
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
                        StreamedAssistantContent::Final(_) => break,
                        _ => {} // Ignore other content types
                    },
                    _ => {} // Ignore other item types
                },
                Err(e) => {
                    return Err(anyhow::anyhow!("Streaming error: {}", e));
                }
            }
        }
    };
}

/// Provider-specific agent wrapper
enum AgentProvider {
    Anthropic(AnthropicAgent),
    OpenAI(OpenAIAgent),
    Ollama(OllamaAgent),
}

/// OwnAI Agent with Memory and LLM integration
pub struct OwnAIAgent {
    agent: AgentProvider,
    context_builder: ContextBuilder,
    db: Pool<Sqlite>,
}

impl OwnAIAgent {
    /// Create a new OwnAI Agent
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

        // Load API key from keychain if needed
        let api_key = if instance.provider.needs_api_key() {
            APIKeyStorage::load(&instance.provider)?
                .ok_or_else(|| anyhow::anyhow!("API key not found for provider: {}", instance.provider))?
        } else {
            String::new()
        };

        // Create provider-specific agent
        let agent = match instance.provider {
            LLMProvider::Anthropic => {
                let client = anthropic::Client::builder()
                    .api_key(&api_key)
                    .build()?;

                let agent = client
                    .agent(&instance.model)
                    .preamble(&Self::system_prompt(&instance.name))
                    .temperature(0.7)
                    .build();

                AgentProvider::Anthropic(agent)
            }

            LLMProvider::OpenAI => {
                let openai_client = openai::Client::builder()
                    .api_key(&api_key)
                    .build()?;

                let agent = openai_client
                    .completions_api()
                    .agent(&instance.model)
                    .preamble(&Self::system_prompt(&instance.name))
                    .temperature(0.7)
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

                // Use the client directly as the model
                let agent = ollama_client
                    .agent(&instance.model)
                    .preamble(&Self::system_prompt(&instance.name))
                    .build();

                AgentProvider::Ollama(agent)
            }
        };

        Ok(OwnAIAgent {
            agent,
            context_builder,
            db,
        })
    }

    /// Main chat method - combines Memory + LLM
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

        let full_prompt = format!("{}\n\nUser: {}", context, user_message);

        // 3. Call LLM based on provider
        let response = match &self.agent {
            AgentProvider::Anthropic(agent) => agent.prompt(&full_prompt).await?,
            AgentProvider::OpenAI(agent) => agent.prompt(&full_prompt).await?,
            AgentProvider::Ollama(agent) => agent.prompt(&full_prompt).await?,
        };

        // 4. Add agent response to working memory
        self.context_builder
            .working_memory_mut()
            .add_message(Message {
                id: uuid::Uuid::new_v4().to_string(),
                role: "agent".to_string(),
                content: response.clone(),
                timestamp: Utc::now(),
            });

        // 5. Save to database
        self.save_message("user", user_message).await?;
        self.save_message("agent", &response).await?;

        Ok(response)
    }

    /// Stream chat response with history
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
        // Note: We skip the last message (current user message) as it will be sent separately
        let messages = self.context_builder.working_memory().get_context();
        let history: Vec<RigMessage> = messages
            .iter()
            .take(messages.len().saturating_sub(1)) // Exclude last (current) message
            .map(|msg| {
                if msg.role == "user" {
                    RigMessage::user(&msg.content)
                } else {
                    RigMessage::assistant(&msg.content)
                }
            })
            .collect();

        // 4. Prepare prompt: include context from long-term memory and summaries
        let prompt = if !context.is_empty() {
            format!("[Context from memory]\n{}\n\n{}", context, user_message)
        } else {
            user_message.to_string()
        };

        // 5. Stream with chat history for proper conversation context
        let mut full_response = String::new();

        match &self.agent {
            AgentProvider::Anthropic(agent) => {
                let mut stream = agent.stream_chat(&prompt, history.clone()).await;
                process_stream!(stream, callback, full_response);
            }
            AgentProvider::OpenAI(agent) => {
                let mut stream = agent.stream_chat(&prompt, history.clone()).await;
                process_stream!(stream, callback, full_response);
            }
            AgentProvider::Ollama(agent) => {
                let mut stream = agent.stream_chat(&prompt, history).await;
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

    /// System prompt for ownAI
    fn system_prompt(instance_name: &str) -> String {
        format!(r#"You are {}, a personal AI agent that evolves with your user.

## Core Identity

You maintain a permanent, growing relationship with your user by:
- Remembering everything important across all conversations
- Learning and adapting to their preferences
- Proactively improving yourself
- Being helpful, concise, and honest

## Your Capabilities

**Memory System:**
- You have access to long-term memories (semantic search)
- Recent conversation summaries
- Full working memory of recent messages

**When you see relevant context above:**
- It comes from previous conversations
- Use it naturally in your responses
- Reference it when relevant

## Response Guidelines

1. **Be conversational**: This is a continuous relationship, not isolated chats
2. **Be proactive**: Suggest improvements when you see patterns
3. **Be honest**: Admit when you don't know something
4. **Be adaptive**: Learn from user feedback and adjust your style

## Important Notes

- You're in Phase 3: LLM Integration is complete!
- Tool creation and advanced features coming in Phase 4
- Focus on being helpful with current capabilities

Remember: You're building a long-term relationship with this user."#,
            instance_name)
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
