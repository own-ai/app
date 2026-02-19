# Active Context - ownAI

## Current Work Focus

The project has **completed Phase 1 (Foundation)**, **Phase 2 (Memory System)**, **Phase 3 (Self-Programming)**, and **Steps 12-13 of Phase 4 (Canvas System Backend + Custom Protocol)**. The next work is **Phase 4, Step 14 (Canvas System - Frontend)**.

## What Has Been Built

### Frontend (React + TypeScript + Tailwind CSS v4)
- Complete chat interface with streaming support (token-by-token display)
- Message components with role-based typography (Noto Serif/Sans/Mono)
- Markdown rendering with syntax highlighting for agent messages (react-markdown + react-syntax-highlighter)
- Auto-growing message input with Enter/Shift+Enter behavior
- AI Instance selector and creation dialog
- Settings panel for provider/model/API key configuration
- Header with search icon, settings, and instance selector
- i18n system with German and English translations
- Full design system implemented in CSS (light + dark mode via prefers-color-scheme, CSS variables, animations)
- Zustand stores for chat state and instance state

### Backend (Rust + Tauri 2.0)
- OwnAIAgent with rig-core 0.30 integration (Anthropic, OpenAI, Ollama providers)
- Multi-turn tool calling support (up to 50 turns)
- Streaming chat via Tauri events (`agent:token`) with multi-turn stream processing
- SQLite database with schema (messages, user_profile, tools, tool_executions, programs; summaries and memory_entries created dynamically)
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
- **Canvas System (Backend)**: 6 agent tools for creating/managing HTML programs, DB table, filesystem storage at `~/.ownai/instances/{id}/programs/`
- **Canvas Custom Protocol**: `ownai-program://` URI scheme for serving program files from local filesystem
- **Canvas Tauri Commands**: list_programs, delete_program, get_program_url (3 commands)
- **Comprehensive System Prompt**: Includes self-programming instructions, Rhai language reference, Canvas programs section, tool iteration workflow
- **Tool Commands**: 5 Tauri commands for dynamic tool management (list, create, update, delete, execute) via AgentCache
- 27 registered Tauri commands (instances CRUD, providers, API keys, chat, memory stats, memory CRUD, dynamic tools, canvas programs)
- AgentCache for multi-instance agent management
- Workspace directory per instance at `~/.ownai/instances/{id}/workspace/`
- Programs directory per instance at `~/.ownai/instances/{id}/programs/`

## Recent Changes

- **Steps 12-13 COMPLETED** (Canvas System Backend + Custom Protocol):
  - Added `programs` table to DB schema with UNIQUE(instance_id, name) constraint
  - Added `get_instance_programs_path()` and `get_program_path()` to utils/paths.rs
  - Created `canvas/` module with 4 files: mod.rs (path resolution + security), storage.rs (DB CRUD), protocol.rs (URL parsing + file serving), tools.rs (6 rig Tools)
  - 6 Canvas agent tools: create_program, list_programs, program_ls, program_read_file, program_write_file, program_edit_file
  - Tools use `#[serde(skip)]` for non-serializable fields (Pool, PathBuf, etc.)
  - Programs identified by name (not UUID), chosen by the agent
  - Agent does NOT know its instance_id - embedded at tool construction time
  - Registered `ownai-program://` custom protocol via `register_asynchronous_uri_scheme_protocol`
  - Protocol parses URLs as `ownai-program://localhost/{instance_id}/{program_name}/{path}`
  - MIME type detection for 18 file extensions (HTML, CSS, JS, images, fonts, etc.)
  - 3 Canvas Tauri commands: list_programs, delete_program, get_program_url
  - Updated `create_tools()` to accept `db` and `programs_root` params and include 6 canvas tools (16 tools total)
  - Updated system prompt with Canvas Programs section
  - All 115 tests pass, cargo clippy clean, cargo fmt applied

## Next Steps

### Near-term (Phase 4 - Deep Agent Features)
- Step 14: Canvas System - Frontend (iframe component, split-view, store)
- Step 15: Bridge API (postMessage communication between Canvas and backend)
- Step 16: Sub-Agent System (code-writer, researcher, memory-manager)
- Step 17: Scheduled Tasks (tokio-cron-scheduler)
- Step 18: Dynamic System Prompt (final integration with all capabilities)

### Later (Phase 5-7)
- UI/UX refinement (message virtualization, Canvas split-view, ToolCallIndicator, TodoList rendering)
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
- **Tool Registration**: Tools are created in `create_tools()` helper and passed to agent builder via `.tools()`
- **Dynamic Tools**: SharedRegistry = `Arc<RwLock<RhaiToolRegistry>>` per agent instance for concurrent access
- **Rhai Safety**: HTTPS-only HTTP, workspace-scoped filesystem, max operations limit, path traversal prevention
- **Per-Instance Registry**: Tool commands access registry through AgentCache (not global Tauri state)
- **Self-Programming Architecture**: Agent writes Rhai code directly and uses create_tool/read_tool/update_tool (not a separate LLM call within a tool)
- **Tool Iteration**: Agent can read source code with read_tool, then fix/improve with update_tool (version auto-incremented)
- **Canvas Programs**: Identified by name (not UUID), filesystem-like tools, agent does not know instance_id
- **Canvas Protocol**: `ownai-program://localhost/{instance_id}/{program_name}/{path}` for serving files

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
