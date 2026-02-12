use std::collections::VecDeque;
use serde::{Deserialize, Serialize};

/// Message structure for working memory
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub role: String,
    pub content: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Working Memory manages a rolling window of recent messages
/// with token budget management
#[derive(Debug)]
pub struct WorkingMemory {
    messages: VecDeque<Message>,
    max_tokens: usize,
    current_tokens: usize,
}

impl WorkingMemory {
    /// Create new working memory with specified token limit
    pub fn new(max_tokens: usize) -> Self {
        Self {
            messages: VecDeque::new(),
            max_tokens,
            current_tokens: 0,
        }
    }

    /// Add a message to working memory
    /// Returns messages that should be summarized if budget exceeded
    pub fn add_message(&mut self, msg: Message) -> Option<Vec<Message>> {
        let msg_tokens = Self::estimate_tokens(&msg);
        
        self.messages.push_back(msg);
        self.current_tokens += msg_tokens;

        // Check if we need to evict old messages
        if self.current_tokens > self.max_tokens {
            Some(self.evict_oldest())
        } else {
            None
        }
    }

    /// Evict oldest messages (about 30% of total) and return them for summarization
    fn evict_oldest(&mut self) -> Vec<Message> {
        let to_remove = std::cmp::max(1, (self.messages.len() * 30) / 100);
        let mut removed = Vec::new();

        for _ in 0..to_remove {
            if let Some(msg) = self.messages.pop_front() {
                self.current_tokens = self.current_tokens.saturating_sub(Self::estimate_tokens(&msg));
                removed.push(msg);
            }
        }

        tracing::info!(
            "Evicted {} messages from working memory (freed ~{} tokens)",
            removed.len(),
            removed.iter().map(|m| Self::estimate_tokens(m)).sum::<usize>()
        );

        removed
    }

    /// Get all messages currently in context
    pub fn get_context(&self) -> Vec<Message> {
        self.messages.iter().cloned().collect()
    }

    /// Get number of messages in working memory
    pub fn message_count(&self) -> usize {
        self.messages.len()
    }

    /// Get current token usage
    pub fn current_tokens(&self) -> usize {
        self.current_tokens
    }

    /// Get max token limit
    pub fn max_tokens(&self) -> usize {
        self.max_tokens
    }

    /// Get utilization percentage
    pub fn utilization(&self) -> f32 {
        if self.max_tokens == 0 {
            0.0
        } else {
            (self.current_tokens as f32 / self.max_tokens as f32) * 100.0
        }
    }

    /// Clear all messages from working memory
    pub fn clear(&mut self) {
        self.messages.clear();
        self.current_tokens = 0;
    }

    /// Estimate tokens for a message (rough approximation: ~4 chars = 1 token)
    fn estimate_tokens(msg: &Message) -> usize {
        // Content + role + some overhead for metadata
        let content_tokens = msg.content.len() / 4;
        let role_tokens = msg.role.len() / 4;
        content_tokens + role_tokens + 5 // +5 for metadata overhead
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn create_test_message(content: &str) -> Message {
        Message {
            id: uuid::Uuid::new_v4().to_string(),
            role: "user".to_string(),
            content: content.to_string(),
            timestamp: Utc::now(),
        }
    }

    fn create_test_message_with_role(role: &str, content: &str) -> Message {
        Message {
            id: uuid::Uuid::new_v4().to_string(),
            role: role.to_string(),
            content: content.to_string(),
            timestamp: Utc::now(),
        }
    }

    #[test]
    fn test_add_message() {
        let mut wm = WorkingMemory::new(1000);
        let msg = create_test_message("Hello");
        
        let evicted = wm.add_message(msg);
        
        assert!(evicted.is_none());
        assert_eq!(wm.message_count(), 1);
    }

    #[test]
    fn test_eviction() {
        let mut wm = WorkingMemory::new(100); // Very small budget
        
        // Add messages until eviction occurs
        for i in 0..20 {
            let msg = create_test_message(&format!("Message {}", i));
            if let Some(evicted) = wm.add_message(msg) {
                assert!(!evicted.is_empty());
                break;
            }
        }
    }

    #[test]
    fn test_eviction_returns_oldest_messages() {
        let mut wm = WorkingMemory::new(100); // Very small budget

        // Track message IDs in order
        let mut all_ids = Vec::new();
        let mut evicted_ids = Vec::new();

        for i in 0..20 {
            let msg = create_test_message(&format!("Message number {}", i));
            all_ids.push(msg.id.clone());

            if let Some(evicted) = wm.add_message(msg) {
                evicted_ids = evicted.iter().map(|m| m.id.clone()).collect();
                break;
            }
        }

        // Evicted messages should be the oldest ones (from the start)
        assert!(!evicted_ids.is_empty());
        for evicted_id in &evicted_ids {
            assert!(all_ids.iter().position(|id| id == evicted_id).unwrap() < evicted_ids.len());
        }
    }

    #[test]
    fn test_eviction_removes_about_30_percent() {
        let mut wm = WorkingMemory::new(200); // Small budget

        // Fill up working memory
        let mut total_added = 0;
        for i in 0..50 {
            let msg = create_test_message(&format!("Test message content {}", i));
            total_added += 1;
            if let Some(evicted) = wm.add_message(msg) {
                // Eviction removes ~30% of messages
                let expected_min = (total_added * 20) / 100; // At least ~20%
                let expected_max = (total_added * 40) / 100; // At most ~40%
                assert!(
                    evicted.len() >= expected_min && evicted.len() <= expected_max,
                    "Evicted {} messages out of {} (expected between {} and {})",
                    evicted.len(),
                    total_added,
                    expected_min,
                    expected_max,
                );
                break;
            }
        }
    }

    #[test]
    fn test_token_tracking_after_eviction() {
        let mut wm = WorkingMemory::new(100);

        // Fill and trigger eviction
        for i in 0..20 {
            let msg = create_test_message(&format!("Message {}", i));
            if wm.add_message(msg).is_some() {
                // After eviction, tokens should be below max
                assert!(
                    wm.current_tokens() <= wm.max_tokens(),
                    "Tokens {} should be <= max {} after eviction",
                    wm.current_tokens(),
                    wm.max_tokens()
                );
                break;
            }
        }
    }

    #[test]
    fn test_multiple_evictions() {
        let mut wm = WorkingMemory::new(100);
        let mut eviction_count = 0;

        // Keep adding messages to trigger multiple evictions
        for i in 0..100 {
            let msg = create_test_message(&format!("Message {}", i));
            if wm.add_message(msg).is_some() {
                eviction_count += 1;
            }
        }

        assert!(
            eviction_count >= 2,
            "Expected multiple evictions, got {}",
            eviction_count
        );
        // Working memory should still have messages
        assert!(wm.message_count() > 0);
    }

    #[test]
    fn test_get_context_preserves_order() {
        let mut wm = WorkingMemory::new(10000);

        for i in 0..5 {
            wm.add_message(create_test_message_with_role(
                if i % 2 == 0 { "user" } else { "agent" },
                &format!("Message {}", i),
            ));
        }

        let context = wm.get_context();
        assert_eq!(context.len(), 5);
        assert_eq!(context[0].content, "Message 0");
        assert_eq!(context[4].content, "Message 4");
        assert_eq!(context[0].role, "user");
        assert_eq!(context[1].role, "agent");
    }

    #[test]
    fn test_utilization() {
        let mut wm = WorkingMemory::new(1000);
        assert_eq!(wm.utilization(), 0.0);
        
        let msg = create_test_message("Test message");
        wm.add_message(msg);
        
        assert!(wm.utilization() > 0.0);
        assert!(wm.utilization() <= 100.0);
    }

    #[test]
    fn test_utilization_zero_budget() {
        let wm = WorkingMemory::new(0);
        assert_eq!(wm.utilization(), 0.0);
    }

    #[test]
    fn test_clear() {
        let mut wm = WorkingMemory::new(1000);
        wm.add_message(create_test_message("Test"));
        
        assert_eq!(wm.message_count(), 1);
        
        wm.clear();
        
        assert_eq!(wm.message_count(), 0);
        assert_eq!(wm.current_tokens(), 0);
    }
}
