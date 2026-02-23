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
Three stores:
- **chatStore**: messages array, isTyping flag, streaming message ID, addMessage/updateMessage/setTyping actions
- **instanceStore**: instances list, activeInstanceId, settings state, CRUD actions
- **canvasStore**: programs list, activeProgram, programUrl, viewMode (chat/split/canvas), loadPrograms/selectProgram/deleteProgram/clearCanvas actions

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
├── tools (21 total for main agent, 20 for sub-agents):
│   ├── Filesystem: ls, read_file, write_file, edit_file, grep
│   ├── Planning: write_todos
│   ├── Dynamic: execute_dynamic_tool (Rhai bridge)
│   ├── Self-Programming: create_tool, read_tool, update_tool
│   ├── Canvas: create_program, list_programs, open_program, program_ls, program_read_file, program_write_file, program_edit_file
│   ├── Memory: search_memory, add_memory, delete_memory
│   └── Delegation: delegate_task (main agent only, excluded from sub-agents)
├── working_memory (VecDeque<Message>)
├── summarization (SummarizationAgent)
├── long_term_memory (SharedLongTermMemory = Arc<Mutex<LongTermMemory>>)
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

#### Sub-Agent Pattern
- Main agent creates sub-agents dynamically via `DelegateTaskTool`
- Sub-agents get ALL tools except `delegate_task` (prevents recursion)
- `base_tools_prompt()` in `subagents.rs` for tool documentation
- Main agent's `system_prompt()` calls `base_tools_prompt()` -- no duplication
- `ClientProvider` enum wraps Anthropic/OpenAI/Ollama clients for sub-agent creation
- Sub-agents have max 25 tool turns, temporary context (no memory persistence)

#### SharedLongTermMemory Pattern
- `SharedLongTermMemory = Arc<Mutex<LongTermMemory>>` type alias
- Created once in `OwnAIAgent::new()`, shared with tools and context builder
- Memory tools (search/add/delete) lock the mutex for each operation
- `commands/memory.rs` clones the Arc, drops AgentCache lock, then locks memory

#### Command Pattern (Tauri)
- 28 registered commands grouped by domain:
  - **Instances**: create, list, delete, set_active, get_active
  - **Providers**: get_providers
  - **API Keys**: save_api_key, has_api_key, delete_api_key
  - **Chat**: send_message, stream_message, load_messages, clear_agent_cache
  - **Memory**: get_memory_stats, search_memory, add_memory_entry, delete_memory_entry
  - **Dynamic Tools**: list_dynamic_tools, create_dynamic_tool, update_dynamic_tool, delete_dynamic_tool, execute_dynamic_tool
  - **Canvas Programs**: list_programs, delete_program, get_program_url
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

CREATE TABLE programs (
    id TEXT PRIMARY KEY,
    instance_id TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    version TEXT NOT NULL DEFAULT '1.0.0',
    created_at DATETIME NOT NULL,
    updated_at DATETIME NOT NULL,
    UNIQUE(instance_id, name)
);
CREATE INDEX idx_programs_instance ON programs(instance_id);
```

## Implemented: Self-Programming Level 1 (Rhai Tools)

The agent can create, read, update, and execute dynamic Rhai tools:
- **Sandboxed Rhai Engine**: 14 safe functions (HTTP, filesystem, JSON, regex, base64, URL encoding, datetime, notifications), security limits (max operations, max string size)
- **Tool Registry**: SQLite-backed with AST caching, execution logging, usage statistics, version management
- **RhaiExecuteTool**: rig Tool bridge that lets the agent invoke dynamic tools by name
- **Self-Programming Tools**: CreateToolTool, ReadToolTool, UpdateToolTool - agent writes Rhai code and iterates
- **Comprehensive System Prompt**: Includes self-programming instructions, Rhai language reference, and tool iteration workflow

## Implemented: Self-Programming Level 2 (Canvas Programs)

The agent can create and manage interactive HTML/CSS/JS applications:
- **Canvas Module** (`canvas/`): 4-file structure - mod.rs (path resolution + security), storage.rs (DB CRUD), protocol.rs (URL parsing + file serving), tools.rs (6 rig Tools)
- **7 Canvas Tools**: create_program, list_programs, open_program, program_ls, program_read_file, program_write_file, program_edit_file
- **Canvas Events**: `canvas:open_program` (backend emits to open a program in frontend), `canvas:program_updated` (backend emits after write/edit to trigger auto-reload)
- **Custom Protocol**: `ownai-program://localhost/{instance_id}/{program_name}/{path}` for serving files
- **Program Identity**: By name (not UUID), chosen by the agent; agent does NOT know its instance_id
- **Filesystem-like Tools**: Write and edit files within program directories (not monolithic save/update)
- **Version Tracking**: Each write/edit automatically increments the program version (semver patch bump)
- **Security**: Path traversal prevention, program name validation
- **Storage**: Programs table in SQLite + files at `~/.ownai/instances/{id}/programs/{program_name}/`
- **3 Tauri Commands**: list_programs, delete_program, get_program_url

## Implemented: Canvas Frontend

- **CanvasPanel** (`components/canvas/CanvasPanel.tsx`): Toolbar (program name, version, fullscreen toggle, close button) + sandboxed iframe with `sandbox="allow-scripts allow-forms allow-modals allow-same-origin"`
- **ProgramList** (`components/canvas/ProgramList.tsx`): Program selection list with inline delete confirmation, empty state
- **canvasStore** (`stores/canvasStore.ts`): programs, activeProgram, programUrl, viewMode (chat/split/canvas), loadPrograms/selectProgram/deleteProgram/clearCanvas
- **Split-View Layout** (in `App.tsx`): Three modes - chat (full-width), split (50/50 chat + canvas), canvas (full-width canvas)
- **Auto-Detection**: After streaming completes, compares program count; if new program detected, auto-opens split view with newest program
- **Header Toggle**: PanelRight icon visible when programs exist or canvas is open, accent-colored when active
- **Program URL**: Iframe loads `ownai-program://localhost/{instanceId}/{programName}/index.html`

## Planned Architecture (Not Yet Implemented)

### Bridge API (for Canvas Programs)
- postMessage communication between Canvas iframe and backend

### Implemented: Sub-Agent System
- **DelegateTaskTool**: Main agent creates temporary sub-agents on the fly with custom system prompts
- **Dynamic Architecture**: No predefined sub-agents -- main agent decides role and prompt per task
- **Tool Access**: Sub-agents get all 20 tools (everything except delegate_task to prevent recursion)
- **Memory Tools**: search_memory, add_memory, delete_memory available to all agents
- **ClientProvider**: Enum wrapping Anthropic/OpenAI/Ollama clients, passed to DelegateTaskTool
- **base_tools_prompt()**: Prompt for tool documentation, used by main + sub-agents

### Deep Agent System (Remaining)
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
      ownai.db              # SQLite: messages, summaries, memory, user_profile, tools, programs
      workspace/             # Agent workspace directory (for filesystem tools)
      programs/              # Generated HTML apps (Canvas)
        {program-name}/
          index.html
          style.css
          app.js
          ...
```
