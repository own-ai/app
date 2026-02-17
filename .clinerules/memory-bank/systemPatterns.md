# System Patterns - ownAI

## Architecture Overview

ownAI is a Tauri 2.0 desktop application with a React frontend and a Rust backend. The architecture is designed to be cross-platform (Desktop now, iOS/Android later via Tauri Mobile).

```
┌─────────────────────────────────────────┐
│         React Frontend (WebView)         │
│  ┌─────────┐ ┌──────────┐ ┌──────────┐ │
│  │Chat UI  │ │Instances │ │Settings  │ │
│  │Messages │ │Selector  │ │Panel     │ │
│  │Input    │ │Dialog    │ │Provider  │ │
│  └────┬────┘ └────┬─────┘ └────┬─────┘ │
│       │Tauri invoke│            │        │
├───────┴────────────┴────────────┴────────┤
│              Tauri Commands               │
│  (15 commands: chat, instances, memory,   │
│   providers, api keys)                    │
├──────────────────────────────────────────┤
│            Rust Backend                   │
│  ┌──────────┐ ┌────────┐ ┌───────────┐  │
│  │OwnAI     │ │Instance│ │Memory     │  │
│  │Agent     │ │Manager │ │System     │  │
│  │(rig-core)│ │        │ │           │  │
│  └────┬─────┘ └───┬────┘ └─────┬─────┘  │
│       │           │             │         │
│  ┌────┴───────────┴─────────────┴──────┐ │
│  │  SQLite Database (per instance)      │ │
│  │  messages | summaries | memory_entries│ │
│  └─────────────────────────────────────┘ │
└──────────────────────────────────────────┘
```

## Key Technical Decisions

### 1. Rust-First Backend (No Node.js Sidecar)
- Apple forbids JIT compilers on iOS - Node.js cannot run there
- One codebase (Rust) runs everywhere: Desktop + Mobile
- Smaller binary, better performance, no runtime dependencies

### 2. Tauri 2.0 Framework
- Lightweight (smaller than Electron)
- Uses system WebView (no bundled Chromium)
- Direct migration path to mobile (Tauri Mobile)
- Security advantages through Rust

### 3. rig-core for LLM Integration
- Production-ready Rust LLM framework
- Unified interface for Anthropic, OpenAI (and more)
- Built-in agent, tool, and streaming support
- Currently using rig-core 0.30

### 4. fastembed for Local Embeddings
- Local embedding generation (no external API calls for embeddings)
- Using `Qwen3-Embedding-0.6B` model
- Privacy-preserving: embeddings computed on-device
- Used for semantic search in long-term memory

### 5. One SQLite Database Per AI Instance
- Complete isolation between instances
- Simple backup (copy one file)
- No cross-instance data leaks
- Path: `~/.ownai/instances/{id}/ownai.db`

### 6. OS Keychain for API Keys
- API keys stored securely via `keyring` crate
- Not in plaintext config files
- Platform-native secure storage

## Design Patterns

### Frontend Patterns

#### State Management (Zustand)
Two stores:
- **chatStore**: messages array, isTyping flag, streaming message ID, addMessage/updateMessage/setTyping actions
- **instanceStore**: instances list, activeInstanceId, settings state, CRUD actions

#### Component Architecture
- **Layout**: Header + scrollable content area + fixed input area
- **Chat Components**: Message > MessageContent (with Markdown), MessageInput, MessageList
- **Instance Components**: AIInstanceSelector (dropdown), CreateInstanceDialog (modal)
- **UI Primitives**: Button, IconButton, Input (reusable base components)

#### Typography-as-Voice Pattern
- Agent messages: `Noto Serif Variable` (serif) - warm, authoritative
- User messages: `Noto Sans Variable` (sans-serif) - clear, functional
- System/Code: `Noto Sans Mono` (monospace) - technical, precise
- All from the same Noto family for visual harmony

#### Styling Approach
- Tailwind CSS v4 with `@theme` for design tokens
- CSS custom properties for colors, spacing, typography
- `cn()` utility (clsx + tailwind-merge) for conditional classes
- Light + dark mode via `[data-theme="dark"]`

### Backend Patterns

#### Agent Architecture
```
OwnAIAgent
├── model (rig-core Agent - Anthropic, OpenAI, or Ollama)
├── tools (Filesystem: ls, read_file, write_file, edit_file, grep; Planning: write_todos)
├── working_memory (VecDeque<Message>)
├── summarization (SummarizationAgent)
├── long_term_memory (LongTermMemory with fastembed)
├── context_builder (ContextBuilder)
└── db (SqlitePool)
```

#### Tool Registration Pattern
- Tools created in `create_tools()` helper function
- Passed to agent builder via `.tools()` method
- Multi-turn tool calling enabled (MAX_TOOL_TURNS = 50)
- Agent uses `process_stream!` macro for uniform streaming across all providers

#### AgentCache Pattern
- `HashMap<String, OwnAIAgent>` wrapped in `Arc<Mutex<>>`
- Lazy initialization: agent created on first use per instance
- Avoids re-creating agents on every request

#### Command Pattern (Tauri)
- 15 registered commands grouped by domain:
  - **Instances**: create, list, update, delete, get_active, set_active
  - **Chat**: send_message, load_messages
  - **Providers**: get_available_providers, get_available_models
  - **API Keys**: save_api_key, get_api_key, delete_api_key
  - **Memory**: get_memory_stats
- All commands are async, return `Result<T, String>`
- State accessed via `tauri::State<Arc<Mutex<T>>>`

#### Memory Hierarchy
```
Working Memory (hot)
  ├── Last ~30-50 messages
  ├── Token budget: 50000 tokens (configurable)
  └── When 70% full → trigger summarization
          │
          ▼
Summaries (warm)
  ├── Condensed conversation segments
  ├── Key facts, decisions, tools used
  └── Stored in `summaries` table
          │
          ▼
Long-term Memory (cold)
  ├── Semantic vector search via fastembed
  ├── Entry types: fact, preference, skill, context
  ├── Importance scoring (0.0 - 1.0)
  └── Stored in `memory_entries` table with embeddings
```

#### Context Builder Assembly
1. Load working memory messages
2. Retrieve relevant summaries
3. Semantic search in long-term memory for current query
4. Assemble into token-budget-compliant context string

#### Streaming Pattern
- Backend sends tokens via Tauri events (`agent:token`)
- Frontend listens via `@tauri-apps/api/event`
- Chat store creates empty agent message, appends tokens incrementally

## Database Schema

```sql
-- Per-instance SQLite database
CREATE TABLE messages (
    id TEXT PRIMARY KEY,
    instance_id TEXT NOT NULL,
    role TEXT NOT NULL,              -- 'user' | 'agent' | 'system'
    content TEXT NOT NULL,
    timestamp DATETIME NOT NULL,
    tokens_used INTEGER,
    importance_score REAL DEFAULT 0.5,
    summary_id TEXT,
    metadata TEXT                    -- JSON
);

CREATE TABLE summaries (
    id TEXT PRIMARY KEY,
    instance_id TEXT NOT NULL,
    start_message_id TEXT NOT NULL,
    end_message_id TEXT NOT NULL,
    summary_text TEXT NOT NULL,
    key_points TEXT NOT NULL,        -- JSON array
    timestamp DATETIME NOT NULL,
    token_savings INTEGER
);

CREATE TABLE memory_entries (
    id TEXT PRIMARY KEY,
    instance_id TEXT NOT NULL,
    content TEXT NOT NULL,
    embedding BLOB,                  -- Vector embedding
    entry_type TEXT NOT NULL,        -- 'fact' | 'preference' | 'skill' | 'context'
    importance REAL DEFAULT 0.5,
    created_at DATETIME NOT NULL,
    last_accessed DATETIME NOT NULL,
    access_count INTEGER DEFAULT 0
);

CREATE TABLE user_profile (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at DATETIME NOT NULL
);
```

## Planned Architecture (Not Yet Implemented)

### Two-Level Self-Programming

#### Level 1: Tools (Rhai Scripts)
- Sandboxed Rhai scripting engine
- LLM generates Rhai scripts for new capabilities
- Tool Registry in database with versioning
- Dynamic loading and execution
- Security: max operations, max string size, safe function wrappers

#### Level 2: Programs (Canvas/HTML Apps)
- LLM generates HTML/CSS/JS single-page apps
- Rendered in sandboxed iframe ("Canvas")
- Bridge API via postMessage for backend communication
- Split-view layout: Chat + Canvas side by side
- Programs stored at `~/.ownai/instances/{id}/programs/`

### Deep Agent System (Remaining)
- **Sub-Agents**: Specialized agents (code-writer, researcher, memory-manager)
- **Scheduled Tasks**: tokio-cron-scheduler for recurring actions

> **Already Implemented**: Planning Tool (write_todos with SharedTodoList) and Filesystem Tools (ls, read_file, write_file, edit_file, grep) are fully implemented and registered with the agent via `create_tools()`.

### Bridge API (for Canvas Programs)
```typescript
window.ownai = {
  chat(prompt): Promise<string>,
  storeData(key, value): Promise<void>,
  loadData(key): Promise<any>,
  notify(message, delay_ms?): Promise<void>,
  readFile(path): Promise<string>,
  writeFile(path, content): Promise<void>,
};
```

## File System Layout

```
~/.ownai/
  instances/
    {instance-id}/
      ownai.db              # SQLite: messages, summaries, memory, user_profile
      workspace/             # Agent workspace directory (for filesystem tools)
      tools/scripts/*.rhai   # Generated Rhai tools (planned)
      programs/              # Generated HTML apps (planned)
        {app-name}/
          index.html
          metadata.json
```
