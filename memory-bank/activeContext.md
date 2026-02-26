# Active Context - ownAI

## Current Work Focus

The project has **completed Phase 1 (Foundation)**, **Phase 2 (Memory System)**, **Phase 3 (Self-Programming)**, and **Phase 4 (Deep Agent Features)** in its entirety. All 18 implementation steps are complete. The next work is **Phase 5 (UI/UX Refinement)** -- a new implementation plan needs to be created.

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
- SQLite database with schema (messages, user_profile, tools, tool_executions, programs, program_data, scheduled_tasks; summaries and memory_entries created dynamically)
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
- **Sub-Agent System**: Dynamic sub-agents via DelegateTaskTool with custom system prompts
  - Sub-agents get ALL tools except delegate_task (prevents recursion)
  - Memory tools (search_memory, add_memory, delete_memory) for all agents
  - base_tools_prompt() for tool documentation
  - ClientProvider enum wrapping Anthropic/OpenAI/Ollama clients
- **Scheduled Tasks System**: Cron-based recurring task execution
  - tokio-cron-scheduler with croner for cron expression validation
  - SharedScheduler as Tauri state, initialized on startup
  - Tasks loaded and registered for all instances on app launch
  - Temporary agents created for each task execution (same tools as sub-agents)
  - 3 scheduler rig Tools: create_scheduled_task, list_scheduled_tasks, delete_scheduled_task
  - 3 scheduler Tauri commands: list_scheduled_tasks, delete_scheduled_task, toggle_scheduled_task
  - Events emitted to frontend on task completion/failure
- **Dynamic System Prompt**: Fully dynamic, assembled from base_tools_prompt() + identity + delegation + scheduling
- **Comprehensive System Prompt**: Includes self-programming instructions, Rhai language reference, Canvas programs section with Bridge API documentation, tool iteration workflow, scheduled tasks documentation
- **Tool Commands**: 5 Tauri commands for dynamic tool management (list, create, update, delete, execute) via AgentCache
- 31 registered Tauri commands (instances CRUD, providers, API keys, chat, memory stats, memory CRUD, dynamic tools, canvas programs, bridge, scheduler)
- AgentCache for multi-instance agent management
- Workspace directory per instance at `~/.ownai/instances/{id}/workspace/`
- Programs directory per instance at `~/.ownai/instances/{id}/programs/`

## Recent Changes

- **Memory System Enhancement (Post-Phase 4)**:
  - **SummarizationAgent refactored**: Moved LLM summarization logic from scattered code in `agent/mod.rs` into clean `SummarizationAgent` with `SummaryExtractor` trait abstraction
  - **Summary Embeddings**: Summaries now get an `embedding BLOB` column, computed automatically via fastembed when saved. Migration handled gracefully in `init_table()`
  - **Key Facts -> MemoryEntry**: When a summary is created, each `key_fact` is automatically stored as a `MemoryEntry` (type: Fact, importance: 0.6) in long-term memory
  - **Semantic Summary Search**: New `search_similar_summaries()` method on `SummarizationAgent` enables cosine-similarity-based retrieval of older summaries
  - **Context Builder Enhanced**: Now includes 3 most recent summaries (with date) AND the most semantically relevant older summary (if similarity >= 0.6 and not already in recent 3), displayed as "Relevant Earlier Conversation" with date and relevance percentage
  - **long_term.rs**: `cosine_similarity`, `vec_to_bytes`, `bytes_to_vec` made `pub(crate)`, new `embed_text()` method exposed for other memory components
  - **agent/mod.rs**: Passes `SharedLongTermMemory` to `SummarizationAgent` via `set_long_term_memory()`
  - All 179 tests pass, clippy clean

- **Step 17 (Scheduled Tasks) COMPLETED**:
  - Created `src-tauri/src/scheduler/` module with 4-file structure:
    - `mod.rs`: Scheduler struct wrapping tokio-cron-scheduler JobScheduler, SharedScheduler type alias, ScheduledTask struct, validate_cron_expression using croner v3 FromStr API, 5 tests
    - `storage.rs`: DB CRUD (load_tasks, save_task, delete_task, update_task_last_run, set_task_enabled, get_task), 7 tests
    - `runner.rs`: register_task_job (cron job registration with closure), execute_task (temporary agent creation), run_task_agent (provider-specific agent with explicit type annotations for Anthropic/OpenAI/Ollama), load_and_register_instance_tasks (startup loader)
    - `tools.rs`: CreateScheduledTaskTool, ListScheduledTasksTool, DeleteScheduledTaskTool as rig Tools, 9 tests
  - Added `scheduled_tasks` table to database schema
  - Added `tokio-cron-scheduler` v0.15.1 and `croner` v3.0.1 to Cargo.toml
  - Scheduler initialized as SharedScheduler in `lib.rs` async setup, tasks loaded for all instances
  - 3 Tauri commands in `commands/scheduler.rs`: list_scheduled_tasks, delete_scheduled_task, toggle_scheduled_task
  - 3 scheduler tools added to agent in `agent/mod.rs` via `app_handle.try_state::<SharedScheduler>()`
  - System prompt updated with Scheduled Tasks section in `base_tools_prompt()`
  - Frontend: ScheduledTask TypeScript interface, i18n translations (EN + DE)
  - All 175 tests pass, cargo clippy clean, cargo fmt clean

- **Step 18 (Dynamic System Prompt) COMPLETED**:
  - System prompt is now fully dynamic, assembled from `base_tools_prompt()` which documents all tool categories
  - Available Rhai tools dynamically listed from registry in system prompt
  - All capabilities documented: filesystem, planning, self-programming, Canvas, Bridge API, memory, delegation, scheduling

## Next Steps

### Immediate
- **Phase 4 is complete** -- all 18 implementation steps done
- Request a new implementation plan for Phase 5-7 (UI/UX Refinement, Testing, Polish)

### Phase 5 - UI/UX Refinement
- Message list virtualization (react-virtual) for performance with large histories
- Infinite scroll with lazy loading of older messages
- Onboarding flow for first-time users
- ToolCallIndicator component improvements
- TodoList rendering in system messages
- Program management UI improvements

### Phase 6-7 - Testing, Polish, Release
- Frontend tests (Vitest + React Testing Library)
- Backend integration tests for Tauri commands
- Error handling improvements
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
- **Scheduler**: SharedScheduler as Tauri state, loaded on startup, scheduler tools added via `app_handle.try_state()`
- **Scheduler Task Execution**: Temporary agents with `build_sub_agent_tools()` (same tools as sub-agents, no delegate_task)
- **Cron Validation**: croner v3.0.1 with FromStr trait (`expr.parse::<croner::Cron>()`)

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
- Scheduler tools use `app_handle.try_state()` for optional state retrieval (no signature changes needed)
- Provider clients need explicit type annotations in runner.rs (`let client: anthropic::Client = ...`)
