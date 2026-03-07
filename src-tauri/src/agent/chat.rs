use anyhow::Result;
use chrono::Utc;
use rig::completion::Prompt;
use tracing::Instrument;
use tracing_opentelemetry::OpenTelemetrySpanExt;

use crate::memory::working_memory::Message;

use super::providers::AgentProvider;
use super::OwnAIAgent;
use super::MAX_TOOL_TURNS;

impl OwnAIAgent {
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
}
