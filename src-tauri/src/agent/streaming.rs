use anyhow::Result;
use chrono::Utc;
use futures::StreamExt;
use rig::agent::MultiTurnStreamItem;
use rig::message::ToolResultContent as RigToolResultContent;
use rig::streaming::{StreamedAssistantContent, StreamedUserContent, StreamingChat};
use tracing::Instrument;
use tracing_opentelemetry::OpenTelemetrySpanExt;

use crate::memory::working_memory::Message;

use super::providers::AgentProvider;
use super::OwnAIAgent;
use super::MAX_TOOL_TURNS;

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

impl OwnAIAgent {
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
}
