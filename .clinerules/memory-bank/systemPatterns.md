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
│  (24 commands: chat, instances, memory,   │
│   providers, api keys, dynamic tools)     │
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
├── tools (10 total):
│   ├── Filesystem: ls, read_file, write_file, edit_file, grep
│   ├── Planning: write_todos
│   ├── Dynamic: execute_dynamic_tool (Rhai bridge)
│   └── Self-Programming: create_tool, read_tool, update_tool
├── working_memory (VecDeque<Message>)
├── summarization (SummarizationAgent)
├── long_term_memory (LongTermMemory with fastembed)
├── context_builder (ContextBuilder)
├── tool_registry (SharedRegistry = Arc<RwLock<RhaiToolRegistry>>)
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
- 24 registered commands grouped by domain:
  - **Instances**: create, list, delete, set_active, get_active
  - **Providers**: get_providers
  - **API Keys**: save_api_key, has_api_key, delete_api_key
  - **Chat**: send_message, stream_message, load_messages, clear_agent_cache
  - **Memory**: get_memory_stats, search_memory, add_memory_entry, delete_memory_entry
  - **Dynamic Tools**: list_dynamic_tools, create_dynamic_tool, update_dynamic_tool, delete_dynamic_tool, execute_dynamic_tool
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

CREATE TABLE tools (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    description TEXT NOT NULL,
    version TEXT NOT NULL DEFAULT '1.0.0',
    script_content TEXT NOT NULL,
    parameters TEXT NOT NULL DEFAULT '[]',  -- JSON array of ParameterDef
    status TEXT NOT NULL DEFAULT 'active',  -- 'active' | 'deprecated' | 'testing'
    created_at DATETIME NOT NULL,
    last_used DATETIME,
    usage_count INTEGER NOT NULL DEFAULT 0,
    success_count INTEGER NOT NULL DEFAULT 0,
    failure_count INTEGER NOT NULL DEFAULT 0,
    parent_tool_id TEXT
);

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

## Implemented: Self-Programming Level 1 (Rhai Tools)

The agent can create, read, update, and execute dynamic Rhai tools:
- **Sandboxed Rhai Engine**: 14 safe functions (HTTP, filesystem, JSON, regex, base64, URL encoding, datetime, notifications), security limits (max operations, max string size)
- **Tool Registry**: SQLite-backed with AST caching, execution logging, usage statistics, version management
- **RhaiExecuteTool**: rig Tool bridge that lets the agent invoke dynamic tools by name
- **Self-Programming Tools**: CreateToolTool, ReadToolTool, UpdateToolTool - agent writes Rhai code and iterates
- **Comprehensive System Prompt**: Includes self-programming instructions, Rhai language reference, and tool iteration workflow

## Planned Architecture (Not Yet Implemented)

### Self-Programming Level 2: Programs (Canvas/HTML Apps)
- LLM generates HTML/CSS/JS single-page apps
- Rendered in sandboxed iframe ("Canvas")
- Bridge API via postMessage for backend communication
- Split-view layout: Chat + Canvas side by side
- Programs stored at `~/.ownai/instances/{id}/programs/`

### Deep Agent System (Remaining)
- **Sub-Agents**: Specialized agents (code-writer, researcher, memory-manager)
- **Scheduled Tasks**: tokio-cron-scheduler for recurring actions

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
