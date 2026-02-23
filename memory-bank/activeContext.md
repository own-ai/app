# Active Context - ownAI

## Current Work Focus

The project has **completed Phase 1 (Foundation)**, **Phase 2 (Memory System)**, **Phase 3 (Self-Programming)**, and **Steps 12-16 of Phase 4 (Canvas System + Bridge API + Sub-Agent System)**. The next work is **Phase 4, Step 17 (Scheduled Tasks)**.

## What Has Been Built

### Frontend (React + TypeScript + Tailwind CSS v4)
- Complete chat interface with streaming support (token-by-token display)
- Message components with role-based typography (Noto Serif/Sans/Mono)
- Markdown rendering with syntax highlighting for agent messages (react-markdown + react-syntax-highlighter)
- Auto-growing message input with Enter/Shift+Enter behavior
- AI Instance selector and creation dialog
- Settings panel for provider/model/API key configuration
- Header with search icon, Canvas toggle, settings, and instance selector
- i18n system with German and English translations
- Full design system implemented in CSS (light + dark mode via prefers-color-scheme, CSS variables, animations)
- Zustand stores for chat state, instance state, and canvas state
- **Canvas Frontend**: Split-view layout (chat + canvas), sandboxed iframe, program list, auto-detection of new programs
- **Bridge API Frontend**: CanvasPanel listens for `ownai-bridge-request` postMessage events from iframes, forwards to Tauri backend, sends responses back

### Backend (Rust + Tauri 2.0)
- OwnAIAgent with rig-core 0.30 integration (Anthropic, OpenAI, Ollama providers)
- Multi-turn tool calling support (up to 50 turns)
- Streaming chat via Tauri events (`agent:token`) with multi-turn stream processing
- SQLite database with schema (messages, user_profile, tools, tool_executions, programs, program_data; summaries and memory_entries created dynamically)
- AI Instance Manager with per-instance databases at `~/.ownai/instances/{id}/`
- API key storage via OS keychain (keyring crate)
- Working Memory (VecDeque with configurable token budget, default 50000 tokens, 30% eviction)
- Summarization via LLM Extractor (rig Extractor with SummaryResponse JsonSchema struct)
- Long-term Memory with local embeddings (fastembed with `Qwen3-Embedding-0.6B` model)
- Context Builder (assembles context from working memory + summaries + long-term memory)
- **Filesystem Tools**: ls, read_file, write_file, edit_file, grep - with security (path traversal prevention), tests, and registered with agent
- **Planning Tool**: write_todos with SharedTodoList, status tracking, markdown output, tests, and registered with agent
- **Rhai Scripting Engine**: Sandboxed engine with 14 safe functions (HTTP, filesystem, JSON, regex, base64, URL encoding, datetime, notifications)
- **Tool Registry**: RhaiToolRegistry with SQLite storage, AST caching, execution logging, usage stats, update_tool with version increment
- **RhaiExecuteTool**: rig Tool bridge that lets the LLM invoke dynamic Rhai tools by name
- **Self-Programming Tools**: CreateToolTool, ReadToolTool, UpdateToolTool - the agent can create, inspect, and iterate on dynamic Rhai tools
- **Canvas System (Backend)**: 7 agent tools for creating/managing HTML programs, DB table, filesystem storage at `~/.ownai/instances/{id}/programs/`
- **Canvas Custom Protocol**: `ownai-program://` URI scheme for serving program files from local filesystem
- **Canvas Tauri Commands**: list_programs, delete_program, get_program_url (3 commands)
- **Bridge API**: Full postMessage-based communication between Canvas iframes and Rust backend
  - 6 bridge methods: chat(), storeData(), loadData(), notify(), readFile(), writeFile()
  - `window.ownai` JavaScript API automatically injected into served HTML files
  - Per-program key-value storage in `program_data` DB table
  - readFile/writeFile scoped to workspace directory (not program directory)
  - `bridge_request` Tauri command dispatches to bridge handlers
- **Comprehensive System Prompt**: Includes self-programming instructions, Rhai language reference, Canvas programs section with Bridge API documentation, tool iteration workflow
- **Tool Commands**: 5 Tauri commands for dynamic tool management (list, create, update, delete, execute) via AgentCache
- 28 registered Tauri commands (instances CRUD, providers, API keys, chat, memory stats, memory CRUD, dynamic tools, canvas programs, bridge)
- AgentCache for multi-instance agent management
- Workspace directory per instance at `~/.ownai/instances/{id}/workspace/`
- Programs directory per instance at `~/.ownai/instances/{id}/programs/`

## Recent Changes

- **Step 16 (Sub-Agent System) COMPLETED**:
  - Created `src-tauri/src/tools/subagents.rs` with dynamic sub-agent system:
    - `ClientProvider` enum wrapping Anthropic/OpenAI/Ollama rig clients
    - `DelegateTaskTool` (name="delegate_task") - main agent creates sub-agents on the fly
    - `base_tools_prompt()` - prompt for tool documentation
    - `build_sub_agent_tools()` - gives sub-agents ALL tools except delegate_task
    - 5 unit tests
  - Created `src-tauri/src/tools/memory_tools.rs` with 3 rig Tools:
    - `SearchMemoryTool` (name="search_memory") - semantic search via SharedLongTermMemory
    - `AddMemoryTool` (name="add_memory") - stores entries in long-term memory
    - `DeleteMemoryTool` (name="delete_memory") - deletes memory entries by ID
    - 9 unit tests
  - Refactored `SharedLongTermMemory = Arc<Mutex<LongTermMemory>>` in `memory/long_term.rs`
  - Updated `memory/context_builder.rs` to use SharedLongTermMemory
  - Refactored `agent/mod.rs`:
    - `create_tools()` now takes 10 args (added SharedLongTermMemory, ClientProvider, model)
    - All 3 provider match arms updated (Anthropic/OpenAI/Ollama)
    - `extract_and_store_facts()` uses shared lock pattern
    - `system_prompt()` uses `base_tools_prompt()` (no more tool doc duplication)
  - Updated `commands/memory.rs` to use SharedLongTermMemory with proper lock management
  - 21 tools for main agent, 20 for sub-agents (all except delegate_task)
  - All 154 tests pass, cargo clippy clean, cargo fmt clean

- **Step 15 (Bridge API) COMPLETED**:
  - Created `src-tauri/src/canvas/bridge.rs` with full bridge module:
    - `BridgeRequest` enum (Chat, StoreData, LoadData, Notify, ReadFile, WriteFile)
    - `BridgeResponse` struct with ok(), ok_empty(), err() constructors
    - `store_program_data()` / `load_program_data()` - DB CRUD for program_data table
    - `handle_store_data()`, `handle_load_data()`, `handle_notify()` handlers
    - `handle_read_file()` / `handle_write_file()` - workspace-scoped with path traversal prevention
    - `resolve_workspace_path()` - same security pattern as filesystem tools
    - `bridge_script()` - returns JavaScript `<script>` block with `window.ownai` API
    - 20 unit tests covering all handlers, data isolation, path traversal prevention
  - Updated `src-tauri/src/canvas/mod.rs` - added `pub mod bridge;`
  - Updated `src-tauri/src/database/schema.rs` - added `program_data` table (program_name, key, value, updated_at)
  - Updated `src-tauri/src/canvas/protocol.rs`:
    - Added `inject_bridge_script()` function (inserts before `</head>`, or `</body>`, or prepends)
    - Modified `load_program_file()` to inject bridge script for HTML files
    - 4 new tests for injection scenarios
  - Updated `src-tauri/src/commands/canvas.rs` - added `bridge_request` Tauri command
  - Updated `src-tauri/src/lib.rs` - registered `bridge_request` command (28 total)
  - Updated `src/components/canvas/CanvasPanel.tsx`:
    - Added `iframeRef` for direct iframe communication
    - Added `useEffect` with postMessage listener for `ownai-bridge-request` events
    - Forwards requests to Tauri backend, sends `ownai-bridge-response` back to iframe
  - Updated system prompt in `agent/mod.rs` with Bridge API section documenting all 6 methods with usage examples
  - All checks pass: `cargo build`, `cargo test` (140 passed), `cargo clippy`, `cargo fmt`, `pnpm tsc --noEmit`, `pnpm lint`

## Next Steps

### Near-term (Phase 4 - Deep Agent Features)
- Step 17: Scheduled Tasks (tokio-cron-scheduler)
- Step 18: Dynamic System Prompt (final integration with all capabilities)

### Later (Phase 5-7)
- UI/UX refinement (message virtualization, onboarding, ToolCallIndicator, TodoList rendering)
- Testing & stabilization
- Build & packaging for release

## Active Decisions and Considerations

- **Embedding Model**: Using fastembed with `Qwen3-Embedding-0.6B` for local embeddings (no external API needed)
- **Provider Support**: Anthropic, OpenAI, and Ollama supported via rig-core
- **Font Choice**: Noto font family (Serif Variable, Sans Variable, Sans Mono) for universal language support
- **Tailwind CSS v4**: Using `@theme` and `@utility` syntax with `@import "tailwindcss"`
- **Token Budget**: Working memory defaults to 50000 tokens; eviction removes 30% of messages when budget exceeded
- **Database**: One SQLite file per AI instance for complete isolation
- **Summarization**: Uses rig Extractor (not raw prompting) for structured JSON output
- **Sub-Agent Architecture**: Dynamic (not predefined) - main agent creates sub-agents on the fly via delegate_task with custom system prompts
- **Tool Documentation**: Via `base_tools_prompt()` in `subagents.rs` - used by both main agent and sub-agents
- **SharedLongTermMemory**: `Arc<Mutex<LongTermMemory>>` for concurrent access by tools and context builder
- **Memory Tools**: search_memory, add_memory, delete_memory available to ALL agents (main + sub-agents)
- **Tool Registration**: Tools are created in `create_tools()` helper and passed to agent builder via `.tools()`
- **Dynamic Tools**: SharedRegistry = `Arc<RwLock<RhaiToolRegistry>>` per agent instance for concurrent access
- **Rhai Safety**: HTTPS-only HTTP, workspace-scoped filesystem, max operations limit, path traversal prevention
- **Per-Instance Registry**: Tool commands access registry through AgentCache (not global Tauri state)
- **Self-Programming Architecture**: Agent writes Rhai code directly and uses create_tool/read_tool/update_tool (not a separate LLM call within a tool)
- **Tool Iteration**: Agent can read source code with read_tool, then fix/improve with update_tool (version auto-incremented)
- **Canvas Programs**: Identified by name (not UUID), filesystem-like tools, agent does not know instance_id
- **Canvas Protocol**: `ownai-program://localhost/{instance_id}/{program_name}/{path}` for serving files
- **Canvas Frontend**: Split-view with three modes (chat/split/canvas), auto-detection of new programs via program count comparison after streaming
- **Canvas Events**: `canvas:open_program` (backend -> frontend to open a program), `canvas:program_updated` (backend -> frontend for auto-reload)
- **Agent instructs to reuse programs**: System prompt tells agent to check `list_programs` first and use `open_program` for existing programs
- **Bridge API**: postMessage-based communication, `window.ownai` injected into HTML, workspace-scoped file access (not program-scoped)
- **Bridge Data Storage**: Per-program key-value storage in `program_data` table, isolated by program name

## Important Patterns and Preferences

- All UI text must go through i18n (react-i18next)
- No emojis in UI, code, or commits
- Typography-driven design: different fonts for different "voices"
- Rust-first backend: no Node.js sidecar (iOS compatibility)
- All data local, privacy by default
- CSS variables for theming, Tailwind for utility classes
- Agent uses `process_stream!` macro for uniform streaming across providers
- Rhai scripts receive parameters via `params_json` scope variable (parsed with `json_parse()`)
- Canvas tools use `#[serde(skip)]` pattern for non-serializable fields (Pool, PathBuf)
- Canvas store uses `useCanvasStore.getState()` for reading state outside React components (in checkForNewPrograms callback)
- Bridge script injection: before `</head>`, fallback to `</body>`, fallback to prepend
