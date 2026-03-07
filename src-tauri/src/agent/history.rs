use chrono::Utc;
use rig::message::{
    AssistantContent, Message as RigMessage, Text as RigText, ToolCall as RigToolCall,
    ToolFunction as RigToolFunction, ToolResult as RigToolResult,
    ToolResultContent as RigToolResultContent, UserContent,
};
use rig::one_or_many::OneOrMany;

use crate::memory::working_memory::{Message, MessageMetadata, ToolCallData, ToolResultData};

use super::OwnAIAgent;

impl OwnAIAgent {
    /// Build chat history from working memory messages, inserting time gap markers
    /// between messages that have significant temporal gaps (>= 4 hours).
    /// The markers are prepended to the following message's content so the LLM
    /// can see where temporal breaks occurred without altering the user/assistant
    /// alternation pattern. The last message in working memory (the current user
    /// message) is excluded since it becomes the prompt.
    pub(super) fn build_history_with_time_markers(&self) -> Vec<RigMessage> {
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

    /// Convert a rig Message (from history) into our DB Message(s).
    /// Assistant messages with tool calls -> one agent message with ToolCalls metadata.
    /// User messages with tool results -> one tool_result message per result.
    pub(super) fn rig_message_to_db_messages(rig_msg: &RigMessage) -> Vec<Message> {
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
    pub(super) fn db_message_to_rig_message(msg: &Message) -> RigMessage {
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
}
