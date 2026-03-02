//! Text chunking for document ingestion.
//!
//! Splits text into overlapping chunks suitable for embedding and storage
//! in the vector store. Respects paragraph boundaries when possible.

/// Configuration for the chunking process.
#[derive(Debug, Clone)]
pub struct ChunkingConfig {
    /// Maximum number of estimated tokens per chunk (default: 400).
    pub max_chunk_tokens: usize,
    /// Number of overlapping tokens between consecutive chunks (default: 80).
    pub overlap_tokens: usize,
    /// Whether to prefer splitting at paragraph boundaries (default: true).
    pub respect_paragraphs: bool,
}

impl Default for ChunkingConfig {
    fn default() -> Self {
        Self {
            max_chunk_tokens: 400,
            overlap_tokens: 80,
            respect_paragraphs: true,
        }
    }
}

/// A chunk of text from a document.
#[derive(Debug, Clone)]
pub struct Chunk {
    /// The text content of this chunk.
    pub text: String,
    /// Zero-based index of this chunk in the document.
    pub index: usize,
}

/// Split text into overlapping chunks based on the given configuration.
///
/// The chunking strategy:
/// 1. Split text into paragraphs (if `respect_paragraphs` is true).
/// 2. Group paragraphs into chunks up to `max_chunk_tokens`.
/// 3. Add overlap from the end of the previous chunk.
pub fn chunk_text(text: &str, config: &ChunkingConfig) -> Vec<Chunk> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    // If the entire text fits in one chunk, return it as-is
    if estimate_tokens(trimmed) <= config.max_chunk_tokens {
        return vec![Chunk {
            text: trimmed.to_string(),
            index: 0,
        }];
    }

    if config.respect_paragraphs {
        chunk_by_paragraphs(trimmed, config)
    } else {
        chunk_by_words(trimmed, config)
    }
}

/// Chunk text by grouping paragraphs.
fn chunk_by_paragraphs(text: &str, config: &ChunkingConfig) -> Vec<Chunk> {
    // Split into paragraphs (separated by one or more blank lines)
    let paragraphs: Vec<&str> = text
        .split("\n\n")
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .collect();

    if paragraphs.is_empty() {
        return Vec::new();
    }

    let mut chunks: Vec<Chunk> = Vec::new();
    let mut current_parts: Vec<String> = Vec::new();
    let mut current_tokens = 0;

    for para in &paragraphs {
        let para_tokens = estimate_tokens(para);

        // If a single paragraph exceeds the chunk size, split it by words
        if para_tokens > config.max_chunk_tokens {
            // First, flush current accumulated paragraphs
            if !current_parts.is_empty() {
                let chunk_text = current_parts.join("\n\n");
                chunks.push(Chunk {
                    text: chunk_text,
                    index: chunks.len(),
                });
                current_parts.clear();
                current_tokens = 0;
            }

            // Split the large paragraph by words
            let sub_chunks = chunk_by_words(para, config);
            for sub in sub_chunks {
                chunks.push(Chunk {
                    text: sub.text,
                    index: chunks.len(),
                });
            }
            continue;
        }

        // Check if adding this paragraph would exceed the limit
        if current_tokens + para_tokens > config.max_chunk_tokens && !current_parts.is_empty() {
            // Save current chunk
            let chunk_text = current_parts.join("\n\n");
            chunks.push(Chunk {
                text: chunk_text,
                index: chunks.len(),
            });

            // Start new chunk with overlap from the end of the previous
            current_parts.clear();
            current_tokens = 0;

            // Add overlap: take last words from the previous chunk that fit within overlap_tokens
            if config.overlap_tokens > 0 && !chunks.is_empty() {
                let prev_text = &chunks.last().unwrap().text;
                let overlap = get_overlap_suffix(prev_text, config.overlap_tokens);
                if !overlap.is_empty() {
                    let overlap_tokens = estimate_tokens(&overlap);
                    current_parts.push(overlap);
                    current_tokens = overlap_tokens;
                }
            }
        }

        current_parts.push(para.to_string());
        current_tokens += para_tokens;
    }

    // Flush remaining
    if !current_parts.is_empty() {
        let chunk_text = current_parts.join("\n\n");
        chunks.push(Chunk {
            text: chunk_text,
            index: chunks.len(),
        });
    }

    // Re-index chunks
    for (i, chunk) in chunks.iter_mut().enumerate() {
        chunk.index = i;
    }

    chunks
}

/// Chunk text by splitting on word boundaries (no paragraph awareness).
fn chunk_by_words(text: &str, config: &ChunkingConfig) -> Vec<Chunk> {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() {
        return Vec::new();
    }

    let mut chunks: Vec<Chunk> = Vec::new();
    let mut start = 0;

    while start < words.len() {
        let prev_start = start;

        // Find the end of this chunk
        let mut end = start;
        let mut tokens = 0;

        while end < words.len() {
            let word_tokens = estimate_tokens(words[end]);
            if tokens + word_tokens > config.max_chunk_tokens && end > start {
                break;
            }
            tokens += word_tokens;
            end += 1;
        }

        let chunk_text = words[start..end].join(" ");
        chunks.push(Chunk {
            text: chunk_text,
            index: chunks.len(),
        });

        // If we consumed all words, we are done
        if end >= words.len() {
            break;
        }

        // Calculate overlap: move start back by overlap_words from end
        let overlap_words = count_words_for_tokens(&words[start..end], config.overlap_tokens);
        start = if end > overlap_words {
            end - overlap_words
        } else {
            end
        };

        // Prevent infinite loop: ensure start always advances
        if start <= prev_start {
            start = prev_start + 1;
        }
    }

    chunks
}

/// Get the last N tokens worth of text from a string (for overlap).
fn get_overlap_suffix(text: &str, max_tokens: usize) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    let word_count = count_words_for_tokens(&words, max_tokens);
    if word_count == 0 {
        return String::new();
    }
    let start = words.len().saturating_sub(word_count);
    words[start..].join(" ")
}

/// Count how many words from the end of a slice fit within a token budget.
fn count_words_for_tokens(words: &[&str], max_tokens: usize) -> usize {
    let mut tokens = 0;
    let mut count = 0;

    for word in words.iter().rev() {
        let word_tokens = estimate_tokens(word);
        if tokens + word_tokens > max_tokens {
            break;
        }
        tokens += word_tokens;
        count += 1;
    }

    count
}

/// Estimate the number of tokens in a text string.
///
/// Uses a simple heuristic: roughly 0.75 words per token for English text.
/// This avoids depending on a tokenizer crate.
pub fn estimate_tokens(text: &str) -> usize {
    let word_count = text.split_whitespace().count();
    // Roughly 1 token per 0.75 words (i.e., ~1.33 tokens per word)
    // This is a conservative estimate that works for most English/German text.
    ((word_count as f64) * 1.33).ceil() as usize
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_estimate_tokens() {
        assert_eq!(estimate_tokens(""), 0);
        assert_eq!(estimate_tokens("hello"), 2); // 1 * 1.33 = 1.33 -> 2
        assert!(estimate_tokens("hello world foo bar") > 0);
    }

    #[test]
    fn test_chunk_empty_text() {
        let config = ChunkingConfig::default();
        let chunks = chunk_text("", &config);
        assert!(chunks.is_empty());
    }

    #[test]
    fn test_chunk_short_text() {
        let config = ChunkingConfig::default();
        let chunks = chunk_text("This is a short text.", &config);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].text, "This is a short text.");
        assert_eq!(chunks[0].index, 0);
    }

    #[test]
    fn test_chunk_respects_max_tokens() {
        let config = ChunkingConfig {
            max_chunk_tokens: 10,
            overlap_tokens: 0,
            respect_paragraphs: false,
        };

        // Generate text that exceeds 10 tokens
        let words: Vec<&str> = (0..50).map(|_| "word").collect();
        let text = words.join(" ");

        let chunks = chunk_text(&text, &config);
        assert!(chunks.len() > 1);

        // Each chunk should not exceed the token limit (approximately)
        for chunk in &chunks {
            let tokens = estimate_tokens(&chunk.text);
            // Allow some tolerance since we estimate tokens
            assert!(
                tokens <= config.max_chunk_tokens + 5,
                "Chunk has {} tokens, max is {}",
                tokens,
                config.max_chunk_tokens
            );
        }
    }

    #[test]
    fn test_chunk_with_paragraphs() {
        let config = ChunkingConfig {
            max_chunk_tokens: 12,
            overlap_tokens: 0,
            respect_paragraphs: true,
        };

        // Each paragraph ~7 tokens, so two paragraphs (~14) exceed limit of 12
        let text = "First paragraph with some words.\n\nSecond paragraph with more text.\n\nThird paragraph here.";

        let chunks = chunk_text(text, &config);
        assert!(
            chunks.len() >= 2,
            "Expected at least 2 chunks, got {}: {:?}",
            chunks.len(),
            chunks.iter().map(|c| &c.text).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_chunk_with_overlap() {
        let config = ChunkingConfig {
            max_chunk_tokens: 15,
            overlap_tokens: 5,
            respect_paragraphs: false,
        };

        let words: Vec<&str> = (0..40).map(|_| "test").collect();
        let text = words.join(" ");

        let chunks = chunk_text(&text, &config);
        assert!(chunks.len() > 1);

        // Check that chunks have overlapping content
        if chunks.len() >= 2 {
            let first_words: Vec<&str> = chunks[0].text.split_whitespace().collect();
            let second_words: Vec<&str> = chunks[1].text.split_whitespace().collect();

            // The second chunk should start with some words from the end of the first
            let first_end = &first_words[first_words.len().saturating_sub(3)..];
            let second_start = &second_words[..3.min(second_words.len())];

            // At least some overlap should exist
            let has_overlap = first_end.iter().any(|w| second_start.contains(w));
            assert!(
                has_overlap,
                "Expected overlap between chunks, first ends with: {:?}, second starts with: {:?}",
                first_end, second_start
            );
        }
    }

    #[test]
    fn test_chunk_indices_are_sequential() {
        let config = ChunkingConfig {
            max_chunk_tokens: 10,
            overlap_tokens: 0,
            respect_paragraphs: false,
        };

        let words: Vec<&str> = (0..100).map(|_| "word").collect();
        let text = words.join(" ");

        let chunks = chunk_text(&text, &config);
        for (i, chunk) in chunks.iter().enumerate() {
            assert_eq!(chunk.index, i);
        }
    }

    #[test]
    fn test_chunk_whitespace_only() {
        let config = ChunkingConfig::default();
        let chunks = chunk_text("   \n\n  \t  ", &config);
        assert!(chunks.is_empty());
    }

    #[test]
    fn test_get_overlap_suffix() {
        let text = "one two three four five six seven eight nine ten";
        let overlap = get_overlap_suffix(text, 5);
        assert!(!overlap.is_empty());
        // Should contain some of the last words
        assert!(overlap.contains("ten"));
    }

    #[test]
    fn test_default_config() {
        let config = ChunkingConfig::default();
        assert_eq!(config.max_chunk_tokens, 400);
        assert_eq!(config.overlap_tokens, 80);
        assert!(config.respect_paragraphs);
    }
}
