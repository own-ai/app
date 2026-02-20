# AGENTS.md

Technical documentation for AI code agents working on ownAI.

## Project Overview

ownAI is a personal AI agent application built with Tauri 2.0 and Rust. Unlike typical chat applications, it maintains a continuous, evolving relationship with the user through a hierarchical memory system and self-programming capabilities.

### Architecture Philosophy

- **Rust-First**: All backend logic in Rust for cross-platform compatibility (including iOS where Node.js cannot run)
- **Privacy-First**: Local-only data storage, no cloud dependencies
- **Single Database**: One SQLite file for messages, summaries, embeddings, and metadata
- **Provider-Agnostic**: LLM integration through rig-core supports multiple providers
- **Minimalist UI**: Typography-driven design with no decorative elements

### Key Design Decisions

1. **No Node.js Sidecar**: Rust backend ensures compatibility with iOS/Android mobile targets
2. **SQLite + fastembed**: Unified storage for structured data and vector embeddings
3. **rig-core**: Production-ready LLM framework with unified interface
4. **Tauri Commands**: Type-safe bridge between React frontend and Rust backend
5. **Two-Level Self-Programming**:
   - **Tools (Rhai)**: Backend functions the LLM calls directly. Sandboxed Rhai scripts for data processing, API calls, etc. No UI.
   - **Programs (Canvas)**: LLM-generated HTML/CSS/JS single-page apps rendered in an iframe. For visual use cases (dashboards, games, forms). Communicate with backend via Bridge API (postMessage).
6. **Scheduled Tasks**: Cron-like system (tokio-cron-scheduler) for recurring agent actions

## Development Setup

### System Requirements

- Rust 1.70 or later
- Node.js 18 or later
- pnpm package manager
- Platform-specific Tauri dependencies:
  - macOS: Xcode Command Line Tools
  - Linux: webkit2gtk, libgtk-3-dev, libayatana-appindicator3-dev
  - Windows: WebView2 runtime

### Installation

```bash
# Clone repository
git clone https://github.com/own-ai/app.git
cd app

# Install frontend dependencies
pnpm install

# Verify Rust setup
cargo --version
rustc --version

# Run in development mode
pnpm tauri dev
```

### Database Setup

The application automatically initializes the SQLite database on first run at:
- macOS/Linux: `~/.ownai/instances/{id}/ownai.db`
- Windows: `%APPDATA%/ownai/instances/{id}/ownai.db`

Database migrations are managed through sqlx. To create a new migration:

```bash
cd src-tauri
cargo install sqlx-cli --no-default-features --features sqlite
sqlx migrate add <migration_name>
```

## Build & Run Commands

```bash
# Frontend development server (without Tauri)
pnpm dev

# Tauri development with hot reload
pnpm tauri dev

# Build optimized production binary
pnpm tauri build

# Run fast Rust tests (no external dependencies)
cargo test

# Run slow/external tests only (marked with #[ignore])
cargo test -- --ignored

# Run ALL tests (fast + slow)
cargo test -- --include-ignored

# Run tests for specific module
cargo test memory::

# Lint Rust code
cargo clippy

# Format Rust code
cargo fmt

# Type-check TypeScript without building
pnpm tsc --noEmit

# Frontend build only
pnpm build

# Local CI (all checks, fast tests only)
./scripts/ci.sh

# Local CI including slow/external tests
RUN_ALL_TESTS=1 ./scripts/ci.sh
```

### Ignored Test Categories

Some tests are marked with `#[ignore]` because they require external resources or are slow. They are skipped by default during `cargo test` and can be run selectively:

```bash
# fastembed tests (require ~1GB model download, slow initialization)
cargo test long_term -- --ignored

# OS keychain tests (require keychain access, not available in headless CI)
cargo test keychain -- --ignored

# LLM integration tests (require running LLM backend like Ollama)
cargo test --test memory_integration -- --ignored

# With specific LLM provider:
TEST_LLM_PROVIDER=anthropic ANTHROPIC_API_KEY=sk-... cargo test --test memory_integration -- --ignored
```

## Project Structure

```
app/
├── src/                          # React frontend
│   ├── components/
│   │   ├── canvas/               # Canvas (program rendering)
│   │   │   ├── CanvasPanel.tsx   # Iframe + toolbar + program list
│   │   │   └── ProgramList.tsx   # Program selection with delete
│   │   ├── chat/                 # Chat UI components
│   │   │   ├── Message.tsx       # Message display (role-based styling)
│   │   │   ├── MessageContent.tsx # Markdown rendering, code blocks
│   │   │   ├── MessageInput.tsx  # Auto-growing textarea
│   │   │   └── MessageList.tsx   # Virtualized scrolling list
│   │   ├── instances/            # AI instance management
│   │   ├── layout/               # Header, Container
│   │   └── ui/                   # Reusable UI primitives
│   ├── i18n/                     # Internationalization (i18next)
│   ├── locales/                  # Translation files (en, de)
│   ├── stores/                   # Zustand state management
│   ├── styles/                   # Global CSS, design tokens
│   ├── types/                    # TypeScript type definitions
│   ├── utils/                    # Helper functions (cn, formatters)
│   └── App.tsx                   # Root component
│
├── src-tauri/                    # Rust backend
│   ├── src/
│   │   ├── ai_instances/         # AI instance management
│   │   │   ├── manager.rs        # Instance CRUD operations
│   │   │   └── models.rs         # Instance data structures
│   │   ├── commands/             # Tauri commands (Frontend API)
│   │   │   ├── chat.rs           # Chat message commands
│   │   │   ├── instances.rs      # Instance management commands
│   │   │   ├── memory.rs         # Memory system commands
│   │   │   └── tools.rs          # Dynamic tool management commands
│   │   ├── database/             # Database layer
│   │   │   ├── schema.rs         # Table definitions, migrations
│   │   │   └── mod.rs            # Connection management
│   │   ├── memory/               # Memory system (core feature)
│   │   │   ├── working_memory.rs # Recent messages (rolling window)
│   │   │   ├── summarization.rs  # Conversation summarization
│   │   │   ├── long_term.rs      # Vector store, semantic search
│   │   │   ├── fact_extraction.rs # Automatic fact extraction from conversations
│   │   │   ├── context_builder.rs # Assembles context for LLM
│   │   │   └── mod.rs
│   │   ├── tools/                # Tool system
│   │   │   ├── filesystem.rs     # ls, read, write, edit, grep tools
│   │   │   ├── planning.rs       # TODO list management tool
│   │   │   ├── registry.rs       # RhaiToolRegistry (SQLite, AST caching)
│   │   │   ├── rhai_engine.rs    # Sandboxed Rhai engine (14 safe functions)
│   │   │   ├── rhai_bridge_tool.rs # RhaiExecuteTool (rig Tool bridge)
│   │   │   ├── code_generation.rs # CreateTool, ReadTool, UpdateTool
│   │   │   └── mod.rs
│   │   ├── utils/                # Utility functions
│   │   │   └── paths.rs          # Cross-platform path handling
│   │   ├── lib.rs                # Library entry point
│   │   └── main.rs               # Binary entry point
│   ├── Cargo.toml                # Rust dependencies
│   └── tauri.conf.json           # Tauri configuration
│
├── public/                       # Static assets
├── README.md                     # Human-readable documentation
├── AGENTS.md                     # This file
└── package.json                  # Frontend dependencies
```

## Architecture Deep Dive

### Memory System

The memory system is the core differentiator of ownAI, enabling true continuity across sessions.

#### 1. Working Memory (`memory/working_memory.rs`)

- **Purpose**: Holds recent messages for immediate context
- **Capacity**: Dynamic based on token budget (typically 30-50 messages)
- **Behavior**: Rolling window, oldest messages summarized when capacity reached
- **Structure**:
  ```rust
  pub struct WorkingMemory {
      messages: VecDeque<Message>,
      max_tokens: usize,
  }
  ```

#### 2. Summarization (`memory/summarization.rs`)

- **Purpose**: Condenses message sequences into summaries
- **Trigger**: When working memory exceeds 70% capacity
- **Output**: Structured summary with key facts, decisions, tools used
- **Storage**: Saved to `summaries` table with references to original messages

#### 3. Long-Term Memory (`memory/long_term.rs`)

- **Purpose**: Semantic search over important information
- **Technology**: fastembed for local embeddings, SQLite vector search
- **Content Types**:
  - User facts ("User lives in Berlin")
  - Preferences ("Prefers concise answers")
  - Skills ("Knows Python")
  - Context ("Working on Project X")
- **Retrieval**: Automatic semantic search based on current query

#### 4. Context Builder (`memory/context_builder.rs`)

- **Purpose**: Assembles final context for LLM from all memory tiers
- **Process**:
  1. Load working memory messages
  2. Retrieve relevant summaries
  3. Semantic search in long-term memory
  4. Assemble into token-budget-compliant context

### Database Schema

Located in `src-tauri/src/database/schema.rs`:

```sql
-- Core conversation data
CREATE TABLE messages (
    id TEXT PRIMARY KEY,
    instance_id TEXT NOT NULL,
    role TEXT NOT NULL,           -- 'user' | 'agent' | 'system'
    content TEXT NOT NULL,
    timestamp DATETIME NOT NULL,
    tokens_used INTEGER,
    importance_score REAL DEFAULT 0.5,
    summary_id TEXT,              -- References summaries.id
    metadata TEXT,                -- JSON for flexibility
    FOREIGN KEY (summary_id) REFERENCES summaries(id)
);

-- Conversation summaries
CREATE TABLE summaries (
    id TEXT PRIMARY KEY,
    instance_id TEXT NOT NULL,
    start_message_id TEXT NOT NULL,
    end_message_id TEXT NOT NULL,
    summary_text TEXT NOT NULL,
    key_points TEXT NOT NULL,     -- JSON array
    timestamp DATETIME NOT NULL,
    token_savings INTEGER
);

-- Long-term memory with embeddings
CREATE TABLE memory_entries (
    id TEXT PRIMARY KEY,
    instance_id TEXT NOT NULL,
    content TEXT NOT NULL,
    embedding BLOB,               -- Vector embedding
    entry_type TEXT NOT NULL,     -- 'fact' | 'preference' | 'skill' | 'context'
    importance REAL DEFAULT 0.5,
    created_at DATETIME NOT NULL,
    last_accessed DATETIME NOT NULL,
    access_count INTEGER DEFAULT 0
);

-- AI instances (multiple agents)
CREATE TABLE ai_instances (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    provider TEXT NOT NULL,       -- 'anthropic' | 'openai' | etc.
    model TEXT NOT NULL,
    system_prompt TEXT,
    created_at DATETIME NOT NULL,
    last_active DATETIME
);

-- User profile/settings
CREATE TABLE user_profile (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at DATETIME NOT NULL
);

-- Dynamic Rhai tools (self-programming)
CREATE TABLE tools (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    description TEXT NOT NULL,
    version TEXT NOT NULL DEFAULT '1.0.0',
    script_content TEXT NOT NULL,
    parameters TEXT NOT NULL DEFAULT '[]',
    status TEXT NOT NULL DEFAULT 'active',
    created_at DATETIME NOT NULL,
    last_used DATETIME,
    usage_count INTEGER NOT NULL DEFAULT 0,
    success_count INTEGER NOT NULL DEFAULT 0,
    failure_count INTEGER NOT NULL DEFAULT 0,
    parent_tool_id TEXT
);

-- Tool execution log
CREATE TABLE tool_executions (
    id TEXT PRIMARY KEY,
    tool_id TEXT NOT NULL,
    timestamp DATETIME NOT NULL,
    success INTEGER NOT NULL,
    execution_time_ms INTEGER NOT NULL,
    error_message TEXT,
    input_params TEXT,
    output TEXT,
    FOREIGN KEY (tool_id) REFERENCES tools(id)
);
```

### Tauri Commands

Commands are the bridge between frontend and backend. Located in `src-tauri/src/commands/`.

#### Adding a New Command

1. Define the command function in appropriate module:

```rust
// src-tauri/src/commands/chat.rs
#[tauri::command]
pub async fn send_message(
    instance_id: String,
    content: String,
    state: tauri::State<'_, AppState>,
) -> Result<Message, String> {
    // Implementation
    Ok(message)
}
```

2. Register in `src-tauri/src/lib.rs`:

```rust
fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            commands::chat::send_message,
            // Add your new command here
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

3. Call from frontend:

```typescript
import { invoke } from '@tauri-apps/api/tauri';

const message = await invoke<Message>('send_message', {
  instanceId: 'xxx',
  content: 'Hello'
});
```

### State Management

#### Frontend (Zustand)

Located in `src/stores/`:

```typescript
// chatStore.ts
interface ChatStore {
  messages: Message[];
  isTyping: boolean;
  addMessage: (message: Message) => void;
  setTyping: (typing: boolean) => void;
}

export const useChatStore = create<ChatStore>((set) => ({
  messages: [],
  isTyping: false,
  addMessage: (message) =>
    set((state) => ({ messages: [...state.messages, message] })),
  setTyping: (typing) => set({ isTyping: typing }),
}));
```

#### Backend (AppState)

```rust
// Shared application state
pub struct AppState {
    pub db: Arc<Mutex<SqliteConnection>>,
    pub instance_manager: Arc<Mutex<InstanceManager>>,
}
```

## Code Style & Conventions

### Rust

#### Error Handling

Use `anyhow::Result` for application errors, `thiserror` for library errors:

```rust
use anyhow::{Result, Context};

pub async fn save_message(msg: &Message) -> Result<()> {
    sqlx::query("INSERT INTO messages ...")
        .execute(&pool)
        .await
        .context("Failed to save message")?;
    Ok(())
}
```

#### Async Patterns

Use `tokio` for async runtime. Always `.await?` database operations:

```rust
pub async fn load_messages(instance_id: &str) -> Result<Vec<Message>> {
    let messages = sqlx::query_as::<_, Message>(
        "SELECT * FROM messages WHERE instance_id = ? ORDER BY timestamp"
    )
    .bind(instance_id)
    .fetch_all(&pool)
    .await?;
    
    Ok(messages)
}
```

#### Naming Conventions

- Types: `PascalCase` (e.g., `WorkingMemory`, `MessageRole`)
- Functions: `snake_case` (e.g., `add_message`, `build_context`)
- Constants: `SCREAMING_SNAKE_CASE` (e.g., `MAX_TOKENS`)
- Modules: `snake_case` (e.g., `working_memory`, `long_term`)

### TypeScript/React

#### Component Structure

```typescript
// Message.tsx
interface MessageProps {
  role: 'user' | 'agent' | 'system';
  content: string;
  timestamp: Date;
  metadata?: MessageMetadata;
}

export const Message = ({ role, content, timestamp, metadata }: MessageProps) => {
  const isUser = role === 'user';
  
  return (
    <div className={cn(
      "py-6 px-8 group",
      isUser && "border-l-2 border-neutral-200 pl-10"
    )}>
      {/* Component content */}
    </div>
  );
};
```

#### Styling Approach

Use Tailwind CSS with the `cn()` utility for conditional classes:

```typescript
import { cn } from '@/utils/cn';

<div className={cn(
  "base-class",
  isActive && "active-class",
  variant === 'primary' && "primary-variant"
)} />
```

#### State Management Patterns

Prefer Zustand stores for global state, local useState for component state:

```typescript
// Global state
const { messages, addMessage } = useChatStore();

// Local state
const [inputValue, setInputValue] = useState('');
```

### Design System Rules

#### Typography

- **Agent messages**: Noto Serif Variable, 16px, line-height 1.7
- **User messages**: Noto Sans Variable, 16px, line-height 1.5, medium weight
- **System messages**: Noto Sans Mono, 14px
- **Code**: Noto Sans Mono, 14px

#### Colors

Use CSS variables defined in `src/styles/app.css`:

```css
/* Neutrals */
--color-background: #fafafa;
--color-foreground: #1a1a1a;
--color-muted: #737373;

/* Semantic (use sparingly) */
--color-accent: #6b4c9a;
--color-success: #059669;
--color-error: #dc2626;
```

**Rule**: Never use color decoratively. Color must have semantic meaning.

#### Spacing

Always use multiples of 4px. Use Tailwind spacing scale:

```typescript
// Good
<div className="py-6 px-8" />  // 24px, 32px

// Bad - arbitrary values
<div className="py-5.75" />
```

#### No Emojis

Never use emojis in UI text, code comments, or commit messages. They make the interface look AI-generated and violate the timeless aesthetic principle.

```typescript
// Bad
<button>Save 💾</button>

// Good
<button>Save</button>
```

## Testing Guidelines

### Rust Tests

Place unit tests in the same file as the code:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_add_message() {
        let mut memory = WorkingMemory::new(1000);
        let msg = Message::new("user", "Hello");
        
        memory.add_message(msg).await.unwrap();
        assert_eq!(memory.messages.len(), 1);
    }
}
```

Integration tests go in `src-tauri/tests/`:

```rust
// tests/memory_integration.rs
#[tokio::test]
async fn test_full_memory_cycle() {
    // Test working memory -> summarization -> long-term
}
```

### Frontend Tests

(To be implemented - consider Vitest + React Testing Library)

## Common Development Tasks

### Adding a New UI Component

1. Create component file in appropriate directory:

```typescript
// src/components/ui/Badge.tsx
interface BadgeProps {
  children: React.ReactNode;
  variant?: 'default' | 'success' | 'error';
}

export const Badge = ({ children, variant = 'default' }: BadgeProps) => {
  return (
    <span className={cn(
      "px-2 py-1 text-xs rounded",
      variant === 'success' && "bg-success-bg text-success",
      variant === 'error' && "bg-error-bg text-error"
    )}>
      {children}
    </span>
  );
};
```

2. Export from index if creating a component library:

```typescript
// src/components/ui/index.ts
export { Badge } from './Badge';
export { Button } from './Button';
```

### Working with the Memory System

Example: Storing a new message and updating memory:

```rust
use crate::memory::{WorkingMemory, LongTermMemory};

pub async fn handle_new_message(
    content: String,
    role: MessageRole,
    state: &AppState,
) -> Result<()> {
    // 1. Create message
    let message = Message::new(role, content);
    
    // 2. Save to database
    save_message(&message, &state.db).await?;
    
    // 3. Add to working memory
    let mut working_memory = state.working_memory.lock().await;
    working_memory.add_message(message.clone()).await?;
    
    // 4. Check if summarization needed
    if working_memory.should_summarize() {
        let summary = summarize_oldest(&working_memory).await?;
        save_summary(&summary, &state.db).await?;
    }
    
    // 5. Extract important facts for long-term memory
    if let Some(fact) = extract_fact(&message) {
        let mut long_term = state.long_term_memory.lock().await;
        long_term.store(fact).await?;
    }
    
    Ok(())
}
```

### Adding a Translation

1. Add key to English locale:

```json
// src/locales/en/translation.json
{
  "chat": {
    "placeholder": "What are you thinking?"
  }
}
```

2. Add to German locale:

```json
// src/locales/de/translation.json
{
  "chat": {
    "placeholder": "Was denkst du?"
  }
}
```

3. Use in component:

```typescript
import { useTranslation } from 'react-i18next';

const { t } = useTranslation();
return <input placeholder={t('chat.placeholder')} />;
```

## Debugging

### Logging

Use the `tracing` crate for structured logging:

```rust
use tracing::{info, warn, error, debug};

#[tracing::instrument]
pub async fn process_message(msg: &Message) -> Result<()> {
    debug!("Processing message: {}", msg.id);
    
    match handle_message(msg).await {
        Ok(_) => {
            info!("Message processed successfully");
            Ok(())
        }
        Err(e) => {
            error!("Failed to process message: {}", e);
            Err(e)
        }
    }
}
```

View logs in terminal when running `pnpm tauri dev`.

### Browser DevTools

Frontend logs appear in browser console. Use:

```typescript
console.log('[ChatStore]', 'Message added:', message);
```

### Rust Debugging

Use `rust-analyzer` extension in VS Code with breakpoints, or add debug prints:

```rust
dbg!(&message);  // Temporary debugging
```

### Common Issues

#### Database Locked

**Symptom**: "database is locked" error  
**Cause**: Multiple concurrent writes  
**Solution**: Ensure all database operations use proper async/await and connection pooling

#### WebView Not Loading

**Symptom**: Blank window in development  
**Cause**: Frontend dev server not running  
**Solution**: Ensure `pnpm dev` is running or use `pnpm tauri dev` which handles both

#### Rust Compilation Errors After Dependencies Change

**Solution**: Clean and rebuild
```bash
cd src-tauri
cargo clean
cargo build
```

## Security & Privacy

### Data Storage

All data is stored locally on the user's machine:
- macOS/Linux: `~/.ownai/`
- Windows: `%APPDATA%/ownai/`

No data is sent to external servers except LLM API calls (which the user configures).

### API Keys

Never commit API keys to the repository. Store them securely:
- Use environment variables or
- Store encrypted in user profile database

### Tool Execution Sandbox

Generated tools run in a Rhai sandbox with:
- No direct filesystem access (only through safe Rust functions)
- Max operations limit (prevents infinite loops)
- Max memory limits
- Network access only to approved domains

## Contributing Notes

When contributing, please:

1. Follow the design philosophy for UI/UX guidelines
2. Follow the code style conventions outlined here
3. Add tests for new functionality
4. Update this file if adding new patterns or architectural changes
5. Keep commits focused and write clear commit messages
6. No emojis anywhere in the codebase

For questions or discussions, please open an issue on GitHub.

---

This document is maintained for AI agents and human developers. Keep it updated as the project evolves.
