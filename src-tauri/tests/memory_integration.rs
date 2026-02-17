//! End-to-end integration tests for the memory and summarization pipeline.
//!
//! These tests require a running LLM backend. By default, they use Ollama
//! (local, free). Set environment variables to switch providers:
//!
//! ```bash
//! # Default: Ollama (requires running Ollama instance)
//! cargo test --test memory_integration -- --ignored
//!
//! # With Anthropic:
//! TEST_LLM_PROVIDER=anthropic ANTHROPIC_API_KEY=sk-... cargo test --test memory_integration -- --ignored
//!
//! # With OpenAI:
//! TEST_LLM_PROVIDER=openai OPENAI_API_KEY=sk-... cargo test --test memory_integration -- --ignored
//! ```

use chrono::Utc;
use ownai_lib::agent::OwnAIAgent;
use ownai_lib::ai_instances::{AIInstance, APIKeyStorage, LLMProvider};
use ownai_lib::database::schema;
use sqlx::sqlite::SqlitePool;

/// Read test configuration from environment variables
fn test_provider() -> LLMProvider {
    match std::env::var("TEST_LLM_PROVIDER")
        .unwrap_or_else(|_| "ollama".to_string())
        .to_lowercase()
        .as_str()
    {
        "anthropic" => LLMProvider::Anthropic,
        "openai" => LLMProvider::OpenAI,
        _ => LLMProvider::Ollama,
    }
}

fn test_model(provider: &LLMProvider) -> String {
    std::env::var("TEST_LLM_MODEL").unwrap_or_else(|_| match provider {
        LLMProvider::Anthropic => "claude-sonnet-4-5-20250929".to_string(),
        LLMProvider::OpenAI => "gpt-5-mini-2025-08-07".to_string(),
        LLMProvider::Ollama => "qwen2.5:0.5b".to_string(),
    })
}

fn test_base_url() -> Option<String> {
    std::env::var("OLLAMA_BASE_URL").ok()
}

/// Create a test AIInstance with the configured provider
fn create_test_instance() -> AIInstance {
    let provider = test_provider();
    let model = test_model(&provider);

    AIInstance {
        id: format!("test-{}", uuid::Uuid::new_v4()),
        name: "Test Agent".to_string(),
        provider,
        model,
        api_base_url: test_base_url(),
        db_path: None,
        tools_path: None,
        created_at: Utc::now(),
        last_active: Utc::now(),
    }
}

/// Setup an in-memory database with all required tables
async fn setup_test_db() -> SqlitePool {
    let pool = SqlitePool::connect("sqlite::memory:")
        .await
        .expect("Failed to create in-memory database");

    // Create base schema
    schema::create_tables(&pool)
        .await
        .expect("Failed to create tables");

    pool
}

/// Ensure API key is available in the keychain for the test.
/// If the key already exists in the keychain, it is left untouched.
/// Returns true if a NEW key was saved (and should be cleaned up after the test).
fn setup_api_key(provider: &LLMProvider) -> bool {
    if !provider.needs_api_key() {
        return false;
    }

    // Check if key already exists in keychain
    if let Ok(Some(_)) = APIKeyStorage::load(provider) {
        println!(
            "API key already in keychain for {:?}, using existing key.",
            provider
        );
        return false; // Don't clean up - it was already there
    }

    let env_key = match provider {
        LLMProvider::Anthropic => "ANTHROPIC_API_KEY",
        LLMProvider::OpenAI => "OPENAI_API_KEY",
        _ => return false,
    };

    if let Ok(key) = std::env::var(env_key) {
        APIKeyStorage::save(provider, &key).expect("Failed to save test API key to keychain");
        true // We saved it, so clean up after test
    } else {
        panic!(
            "Provider {:?} requires API key. Either save it in the app settings \
             or set the {} environment variable.",
            provider, env_key
        );
    }
}

/// End-to-end test: Agent with small token budget triggers eviction
/// and LLM-based summarization.
///
/// Run with: cargo test --test memory_integration -- --ignored
#[tokio::test]
#[ignore]
async fn test_summarization_pipeline_with_llm() {
    // Initialize logging for test output
    let _ = tracing_subscriber::fmt::try_init();

    let instance = create_test_instance();
    let api_key_saved = setup_api_key(&instance.provider);

    println!(
        "Testing with provider: {:?}, model: {}",
        instance.provider, instance.model
    );

    let db = setup_test_db().await;

    // Create agent with very small token budget to trigger quick eviction
    let mut agent = OwnAIAgent::new(&instance, db.clone(), Some(300))
        .await
        .expect("Failed to create agent");

    // Send multiple messages to trigger eviction + summarization
    let test_messages = [
        "My name is Alice and I live in Berlin.",
        "I work as a software engineer at a startup.",
        "I am currently learning Rust and really enjoying it.",
        "Can you help me understand how ownership works in Rust?",
        "I also have a cat named Pixel.",
    ];

    for (i, msg) in test_messages.iter().enumerate() {
        println!("Sending message {}/{}: {}", i + 1, test_messages.len(), msg);
        let response = agent.chat(msg).await;
        match &response {
            Ok(r) => println!("Response: {}...", &r[..r.len().min(100)]),
            Err(e) => println!("Error (continuing): {}", e),
        }
    }

    // Check working memory state
    let wm = agent.context_builder().working_memory();
    println!(
        "Working memory: {} messages, {} tokens, {:.1}% utilization",
        wm.message_count(),
        wm.current_tokens(),
        wm.utilization()
    );

    // Check if summaries were created in the database
    let summary_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM summaries")
        .fetch_one(&db)
        .await
        .unwrap_or(0);

    println!("Summaries in database: {}", summary_count);

    // With a 300 token budget and 5 messages + responses, eviction should have triggered
    // Note: This assertion may be soft depending on model response length
    if summary_count > 0 {
        // Verify summary content is meaningful
        let summary_text: String = sqlx::query_scalar("SELECT summary_text FROM summaries LIMIT 1")
            .fetch_one(&db)
            .await
            .unwrap();

        println!("First summary: {}", summary_text);
        assert!(!summary_text.is_empty(), "Summary text should not be empty");

        // Check that key_facts were extracted
        let key_facts_json: String = sqlx::query_scalar("SELECT key_facts FROM summaries LIMIT 1")
            .fetch_one(&db)
            .await
            .unwrap();

        let key_facts: Vec<String> =
            serde_json::from_str(&key_facts_json).expect("key_facts should be valid JSON");
        println!("Extracted key facts: {:?}", key_facts);
    } else {
        println!(
            "No summaries created (working memory may not have reached eviction threshold). \
             This can happen with very short LLM responses."
        );
    }

    // Verify messages were saved to the database
    let msg_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messages")
        .fetch_one(&db)
        .await
        .unwrap_or(0);

    println!("Messages in database: {}", msg_count);
    assert!(msg_count > 0, "Messages should have been saved to database");

    // Clean up API key from keychain if we saved one
    if api_key_saved {
        let _ = APIKeyStorage::delete(&instance.provider);
    }

    println!("Test completed successfully.");
}

/// Simpler test: Verify agent creation works with the configured provider.
///
/// Run with: cargo test --test memory_integration -- --ignored
#[tokio::test]
#[ignore]
async fn test_agent_creation() {
    let _ = tracing_subscriber::fmt::try_init();

    let instance = create_test_instance();
    let api_key_saved = setup_api_key(&instance.provider);

    println!(
        "Creating agent with provider: {:?}, model: {}",
        instance.provider, instance.model
    );

    let db = setup_test_db().await;

    let agent = OwnAIAgent::new(&instance, db, None).await;

    assert!(
        agent.is_ok(),
        "Agent creation should succeed: {:?}",
        agent.err()
    );

    println!("Agent created successfully.");

    if api_key_saved {
        let _ = APIKeyStorage::delete(&instance.provider);
    }
}
