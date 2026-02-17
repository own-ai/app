use std::collections::VecDeque;
use serde::{Deserialize, Serialize};

/// Message structure for working memory
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub role: String,
    pub content: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub importance_score: f32,  // 0.0-1.0, default 0.5
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
            removed.iter().map(Self::estimate_tokens).sum::<usize>()
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

    /// Load messages from database into working memory (e.g., on agent initialization)
    /// Respects token budget - only loads as many messages as fit within the budget
    pub fn load_from_messages(&mut self, messages: Vec<Message>) {
        self.clear();
        
        // Load messages in order, respecting token budget
        for msg in messages {
            let msg_tokens = Self::estimate_tokens(&msg);
            
            // Stop loading if adding this message would exceed budget
            if self.current_tokens + msg_tokens > self.max_tokens {
                tracing::warn!(
                    "Working memory budget reached during load. Loaded {}/{} messages",
                    self.messages.len(),
                    self.messages.len() + 1
                );
                break;
            }
            
            self.messages.push_back(msg);
            self.current_tokens += msg_tokens;
        }
        
        tracing::info!(
            "Loaded {} messages into working memory ({} tokens, {:.1}% utilization)",
            self.messages.len(),
            self.current_tokens,
            self.utilization()
        );
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
            importance_score: 0.5,
        }
    }

    fn create_test_message_with_role(role: &str, content: &str) -> Message {
        Message {
            id: uuid::Uuid::new_v4().to_string(),
            role: role.to_string(),
            content: content.to_string(),
            timestamp: Utc::now(),
            importance_score: 0.5,
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

    #[test]
    fn test_load_from_messages() {
        let mut wm = WorkingMemory::new(10000);
        
        let messages = vec![
            create_test_message("First message"),
            create_test_message("Second message"),
            create_test_message("Third message"),
        ];
        
        wm.load_from_messages(messages.clone());
        
        assert_eq!(wm.message_count(), 3);
        let context = wm.get_context();
        assert_eq!(context[0].content, "First message");
        assert_eq!(context[1].content, "Second message");
        assert_eq!(context[2].content, "Third message");
    }

    #[test]
    fn test_load_from_messages_empty() {
        let mut wm = WorkingMemory::new(1000);
        
        wm.load_from_messages(vec![]);
        
        assert_eq!(wm.message_count(), 0);
        assert_eq!(wm.current_tokens(), 0);
    }

    #[test]
    fn test_load_from_messages_respects_token_budget() {
        let mut wm = WorkingMemory::new(50); // Very small budget
        
        // Create messages with longer content to exceed budget
        let messages = vec![
            create_test_message("This is a very long message that will definitely consume many tokens and should help us test the budget limits properly"),
            create_test_message("Another extremely long message that will also consume a significant number of tokens to ensure we exceed the budget"),
            create_test_message("Yet another message with substantial content to add more token usage"),
            create_test_message("And one more message to make absolutely sure we exceed the token limit"),
            create_test_message("Final message that definitely should not fit in the budget anymore"),
        ];
        
        wm.load_from_messages(messages.clone());
        
        // Should have loaded some messages but not all due to budget
        assert!(wm.message_count() < messages.len(), 
            "Expected to load fewer than {} messages, but loaded {}", 
            messages.len(), 
            wm.message_count());
        assert!(wm.message_count() > 0);
        assert!(wm.current_tokens() <= wm.max_tokens());
    }

    #[test]
    fn test_load_from_messages_clears_existing() {
        let mut wm = WorkingMemory::new(10000);
        
        // Add some messages first
        wm.add_message(create_test_message("Old message 1"));
        wm.add_message(create_test_message("Old message 2"));
        assert_eq!(wm.message_count(), 2);
        
        // Load new messages - should clear old ones
        let new_messages = vec![
            create_test_message("New message 1"),
            create_test_message("New message 2"),
            create_test_message("New message 3"),
        ];
        
        wm.load_from_messages(new_messages);
        
        assert_eq!(wm.message_count(), 3);
        let context = wm.get_context();
        assert_eq!(context[0].content, "New message 1");
    }
}
