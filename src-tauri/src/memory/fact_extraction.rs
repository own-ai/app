use chrono::Utc;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{MemoryEntry, MemoryType};

/// Structured response extracted from LLM when extracting facts from a conversation turn.
/// Used with rig Extractors for type-safe structured output.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct FactExtractionResponse {
    /// List of extracted facts
    pub facts: Vec<ExtractedFactItem>,
}

/// Individual fact extracted from conversation
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct ExtractedFactItem {
    /// The fact content as a concise statement
    pub content: String,
    /// Type of fact: "fact", "preference", "skill", "context", "tool_usage"
    pub fact_type: String,
    /// Importance score (0.0 - 1.0)
    pub importance: f32,
}

/// Parse fact_type string into MemoryType enum
pub fn parse_memory_type(fact_type: &str) -> MemoryType {
    match fact_type.to_lowercase().as_str() {
        "fact" => MemoryType::Fact,
        "preference" => MemoryType::Preference,
        "skill" => MemoryType::Skill,
        "context" => MemoryType::Context,
        "tool_usage" => MemoryType::ToolUsage,
        _ => MemoryType::Fact, // Default to Fact
    }
}

/// Convert ExtractedFactItem to MemoryEntry
pub fn to_memory_entry(item: ExtractedFactItem, source_message_id: &str) -> MemoryEntry {
    let now = Utc::now();
    MemoryEntry {
        id: uuid::Uuid::new_v4().to_string(),
        content: item.content,
        entry_type: parse_memory_type(&item.fact_type),
        importance: item.importance.clamp(0.0, 1.0),
        created_at: now,
        last_accessed: now,
        access_count: 0,
        tags: Vec::new(),
        source_message_ids: vec![source_message_id.to_string()],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_memory_type() {
        assert_eq!(parse_memory_type("fact"), MemoryType::Fact);
        assert_eq!(parse_memory_type("Fact"), MemoryType::Fact);
        assert_eq!(parse_memory_type("FACT"), MemoryType::Fact);
        assert_eq!(parse_memory_type("preference"), MemoryType::Preference);
        assert_eq!(parse_memory_type("skill"), MemoryType::Skill);
        assert_eq!(parse_memory_type("context"), MemoryType::Context);
        assert_eq!(parse_memory_type("tool_usage"), MemoryType::ToolUsage);
        assert_eq!(parse_memory_type("unknown"), MemoryType::Fact); // Default
    }

    #[test]
    fn test_to_memory_entry() {
        let item = ExtractedFactItem {
            content: "User prefers concise answers".to_string(),
            fact_type: "preference".to_string(),
            importance: 0.8,
        };

        let entry = to_memory_entry(item, "msg_123");

        assert_eq!(entry.content, "User prefers concise answers");
        assert_eq!(entry.entry_type, MemoryType::Preference);
        assert_eq!(entry.importance, 0.8);
        assert_eq!(entry.access_count, 0);
        assert_eq!(entry.source_message_ids, vec!["msg_123"]);
        assert!(entry.tags.is_empty());
        assert!(!entry.id.is_empty());
    }

    #[test]
    fn test_to_memory_entry_clamps_importance() {
        let item = ExtractedFactItem {
            content: "Test fact".to_string(),
            fact_type: "fact".to_string(),
            importance: 1.5, // Over 1.0
        };

        let entry = to_memory_entry(item, "msg_456");
        assert_eq!(entry.importance, 1.0); // Should be clamped

        let item2 = ExtractedFactItem {
            content: "Test fact 2".to_string(),
            fact_type: "fact".to_string(),
            importance: -0.2, // Under 0.0
        };

        let entry2 = to_memory_entry(item2, "msg_789");
        assert_eq!(entry2.importance, 0.0); // Should be clamped
    }

    #[test]
    fn test_extracted_fact_item_clone() {
        let item = ExtractedFactItem {
            content: "User knows Rust".to_string(),
            fact_type: "skill".to_string(),
            importance: 0.9,
        };

        let cloned = item.clone();
        assert_eq!(cloned.content, item.content);
        assert_eq!(cloned.fact_type, item.fact_type);
        assert_eq!(cloned.importance, item.importance);
    }
}
