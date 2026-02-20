# Active Context - ownAI

## Current Work Focus

The project has **completed Phase 1 (Foundation)**, **Phase 2 (Memory System)**, **Phase 3 (Self-Programming)**, and **Steps 12-14 of Phase 4 (Canvas System Backend + Custom Protocol + Frontend)**. Additional features have been added: **open_program tool** and **auto-reload on program updates**. The next work is **Phase 4, Step 15 (Bridge API)**.

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

- **open_program Tool + Auto-Reload COMPLETED**:
  - Added `OpenProgramTool` to `src-tauri/src/canvas/tools.rs` (rig Tool that emits `canvas:open_program` event)
  - Added `app_handle: Option<AppHandle>` to `CreateProgramTool`, `ProgramWriteFileTool`, `ProgramEditFileTool`
  - `ProgramWriteFileTool` and `ProgramEditFileTool` now emit `canvas:program_updated` event after successful operations
  - Updated `create_tools()` in `agent/mod.rs` to accept and pass `app_handle`
  - Updated `OwnAIAgent::new()` to accept `app_handle: Option<AppHandle>`
  - Updated `commands/chat.rs` (`send_message` + `stream_message`) to pass `AppHandle` to agent creation
  - Updated system prompt Canvas section: agent instructed to call `list_programs` first, use `open_program` for existing programs
  - Added `refreshActiveProgram(newVersion?)` action to `canvasStore.ts` (cache-busting iframe reload via `?v=timestamp`)
  - Added event listener for `canvas:open_program` in `App.tsx` (selectProgram + setViewMode('split'))
  - Added event listener for `canvas:program_updated` in `App.tsx` (iframe auto-reload + version update)
  - All checks pass: `cargo build`, `cargo test` (115 passed), `cargo clippy`, `cargo fmt`, `pnpm tsc --noEmit`, `pnpm lint`, `pnpm format`

- **Step 14 COMPLETED** (Canvas System - Frontend):
  - Added `Program` and `CanvasViewMode` types to `src/types/index.ts`
  - Created `src/stores/canvasStore.ts` (Zustand store with programs, activeProgram, programUrl, viewMode, loadPrograms, selectProgram, deleteProgram, clearCanvas)
  - Created `src/components/canvas/ProgramList.tsx` (program selection list with inline delete confirmation, empty state message)
  - Created `src/components/canvas/CanvasPanel.tsx` (toolbar with program name/version, fullscreen/minimize toggle, close button, sandboxed iframe, program list fallback)
  - Modified `src/components/layout/Header.tsx` (added PanelRight icon for canvas toggle, visible when programs exist or canvas is open, accent-colored when active)
  - Modified `src/App.tsx` for split-view layout:
    - Three view modes: `chat` (full-width chat), `split` (50/50 chat + canvas), `canvas` (full-width canvas)
    - Auto-detection of new programs after streaming completes (compares program count, auto-opens newest in split view)
    - Programs loaded when active instance changes, canvas cleared when no instance
  - Added i18n translations (EN + DE) for canvas section (12 keys each)
  - Iframe uses `sandbox="allow-scripts allow-forms allow-modals allow-same-origin"` with `ownai-program://` protocol
  - All frontend checks pass: `pnpm tsc --noEmit`, `pnpm lint`, `pnpm format`

## Next Steps

### Near-term (Phase 4 - Deep Agent Features)
- Step 15: Bridge API (postMessage communication between Canvas and backend)
- Step 16: Sub-Agent System (code-writer, researcher, memory-manager)
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
