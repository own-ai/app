use chrono::Utc;
use serde::{Deserialize, Serialize};
use tokio::time::Duration;

use crate::database::init_database;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub role: String,
    pub content: String,
    pub timestamp: String,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct SendMessageRequest {
    pub instance_id: String,
    pub content: String,
}

/// Mock chat command - simulates AI response
#[tauri::command]
pub async fn send_message_mock(
    request: SendMessageRequest,
) -> Result<Message, String> {
    // Simulate thinking time
    tokio::time::sleep(Duration::from_millis(800)).await;
    
    // Generate mock response
    let response_content = generate_mock_response(&request.content);
    
    let response = Message {
        id: uuid::Uuid::new_v4().to_string(),
        role: "agent".to_string(),
        content: response_content,
        timestamp: Utc::now().to_rfc3339(),
        metadata: None,
    };
    
    Ok(response)
}

/// Save a message to the database
#[tauri::command]
pub async fn save_message(
    instance_id: String,
    message: Message,
) -> Result<(), String> {
    let pool = init_database(&instance_id)
        .await
        .map_err(|e| e.to_string())?;
    
    let metadata_json = message.metadata
        .as_ref()
        .and_then(|m| serde_json::to_string(m).ok());
    
    sqlx::query(
        r#"
        INSERT INTO messages (id, role, content, timestamp, metadata)
        VALUES (?, ?, ?, ?, ?)
        "#,
    )
    .bind(&message.id)
    .bind(&message.role)
    .bind(&message.content)
    .bind(&message.timestamp)
    .bind(metadata_json)
    .execute(&pool)
    .await
    .map_err(|e| format!("Failed to save message: {}", e))?;
    
    tracing::debug!("Message saved: {} ({})", message.id, message.role);
    
    Ok(())
}

/// Load messages from the database
#[tauri::command]
pub async fn load_messages(
    instance_id: String,
    limit: i32,
    offset: i32,
) -> Result<Vec<Message>, String> {
    let pool = init_database(&instance_id)
        .await
        .map_err(|e| e.to_string())?;
    
    let messages = sqlx::query_as::<_, (String, String, String, String, Option<String>)>(
        r#"
        SELECT id, role, content, timestamp, metadata
        FROM messages
        ORDER BY timestamp ASC
        LIMIT ? OFFSET ?
        "#,
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(&pool)
    .await
    .map_err(|e| format!("Failed to load messages: {}", e))?;
    
    let messages: Vec<Message> = messages
        .into_iter()
        .map(|(id, role, content, timestamp, metadata)| Message {
            id,
            role,
            content,
            timestamp,
            metadata: metadata.and_then(|m| serde_json::from_str(&m).ok()),
        })
        .collect();
    
    tracing::debug!("Loaded {} messages", messages.len());
    
    Ok(messages)
}

/// Generate a mock AI response
fn generate_mock_response(user_message: &str) -> String {
    let lower = user_message.to_lowercase();
    
    // Simple pattern matching for demo purposes
    if lower.contains("hallo") || lower.contains("hi") || lower.contains("hello") {
        "Hallo! Wie kann ich dir heute helfen? Ich bin ein Mock-Agent und antworte mit vordefinierten Nachrichten, bis die echte LLM-Integration implementiert ist.".to_string()
    } else if lower.contains("wie geht") || lower.contains("how are") {
        "Mir geht es gut, danke! Ich bin bereit, dir bei deinen Aufgaben zu helfen. Was möchtest du heute erreichen?".to_string()
    } else if lower.contains("wetter") || lower.contains("weather") {
        "Das Wetter ist heute schön! ☀️\n\n(Dies ist eine Mock-Antwort. In Phase 3 werde ich echte Wetter-Tools erstellen können.)".to_string()
    } else if lower.contains("code") || lower.contains("programmier") {
        "Ich kann dir beim Programmieren helfen! Hier ist ein Beispiel:\n\n```rust\nfn main() {\n    println!(\"Hello, ownAI!\");\n}\n```\n\nWas möchtest du erstellen?".to_string()
    } else if lower.contains("danke") || lower.contains("thank") {
        "Gerne! Lass mich wissen, wenn du noch etwas brauchst. 😊".to_string()
    } else {
        format!(
            "Ich habe deine Nachricht verstanden: \"{}\"\n\nIch bin momentan ein Mock-Agent und gebe vordefinierte Antworten. In den nächsten Phasen werde ich:\n\n1. **Phase 2**: Ein echtes Memory-System erhalten\n2. **Phase 3**: Selbst Tools programmieren können\n3. **Phase 4**: Deep Agent Features wie TODO-Listen nutzen\n\nBis dahin helfe ich dir gerne mit diesen Demo-Antworten!",
            user_message
        )
    }
}
