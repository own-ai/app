# Active Context - ownAI

## Current Work Focus

The project has **completed Phase 1 (Foundation)**, **Phase 2 (Memory System)**, and the **core of Phase 3 (Self-Programming)**. Steps 6-8 and 10 of Phase 3 are complete: the sandboxed Rhai scripting engine, Tool Registry with DB storage, RhaiExecuteTool bridge, and full agent integration with Tauri commands. The remaining Phase 3 work is Step 9 (Code Generation Agent) and Step 11 (Capability Detection). After that, Phase 4 (Deep Agent Features including Canvas) is next.

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
- SQLite database with schema (messages, user_profile, tools, tool_executions; summaries and memory_entries created dynamically)
- AI Instance Manager with per-instance databases at `~/.ownai/instances/{id}/`
- API key storage via OS keychain (keyring crate)
- Working Memory (VecDeque with configurable token budget, default 50000 tokens, 30% eviction)
- Summarization via LLM Extractor (rig Extractor with SummaryResponse JsonSchema struct)
- Long-term Memory with local embeddings (fastembed with `Qwen3-Embedding-0.6B` model)
- Context Builder (assembles context from working memory + summaries + long-term memory)
- **Filesystem Tools**: ls, read_file, write_file, edit_file, grep - with security (path traversal prevention), tests, and registered with agent
- **Planning Tool**: write_todos with SharedTodoList, status tracking, markdown output, tests, and registered with agent
- **Rhai Scripting Engine**: Sandboxed engine with 14 safe functions (HTTP, filesystem, JSON, regex, base64, URL encoding, datetime, notifications)
- **Tool Registry**: RhaiToolRegistry with SQLite storage, AST caching, execution logging, usage stats tracking
- **RhaiExecuteTool**: rig Tool bridge that lets the LLM invoke dynamic Rhai tools by name
- **Tool Commands**: 4 Tauri commands for dynamic tool management (list, create, delete, execute) via AgentCache
- 23 registered Tauri commands (instances CRUD, providers, API keys, chat, memory stats, memory CRUD, dynamic tools)
- AgentCache for multi-instance agent management
- Workspace directory per instance at `~/.ownai/instances/{id}/workspace/`

## Recent Changes

- **Test Organization**: Added `#[ignore]` to slow/external tests (5 fastembed tests in `long_term.rs`, 1 keychain test in `keychain.rs`). `cargo test` now runs only fast tests. `cargo test -- --ignored` runs slow tests. `ci.sh` supports `RUN_ALL_TESTS=1` flag. Documentation updated in README.md, AGENTS.md, and memory bank.
- **Phase 3 Steps 6-8 + 10 COMPLETED**:
  - Created `tools/rhai_engine.rs` with sandboxed Rhai engine (14 safe functions, security limits)
  - Created `tools/registry.rs` with RhaiToolRegistry (register, execute, list, delete, cache, stats)
  - Created `tools/rhai_bridge_tool.rs` with RhaiExecuteTool implementing rig::tool::Tool
  - Added `tools` and `tool_executions` tables to database schema
  - Integrated RhaiExecuteTool into agent's `create_tools()` with SharedRegistry
  - Added `tool_registry: SharedRegistry` field to OwnAIAgent struct
  - Created `commands/tools.rs` with 4 Tauri commands accessing registry per-instance via AgentCache
  - Registered all commands in `lib.rs` (now 23 total)
  - Added `regex` and `base64` crate dependencies, `blocking` feature for reqwest
  - All 77 tests pass, cargo clippy clean, cargo fmt applied

## Next Steps

### Near-term (Phase 3 remaining)
- Step 9: Code Generation Agent (`tools/code_generation.rs`) - LLM generates Rhai scripts
- Step 11: Capability Detection - system prompt enhancement for self-programming

### Medium-term (Phase 4 - Deep Agent Features)
- Canvas System (iframe-based HTML app rendering) - HIGH PRIORITY for use cases
- Bridge API (postMessage communication between Canvas apps and backend)
- Sub-Agent System (code-writer, researcher, memory-manager)
- Scheduled Tasks (cron-like system with tokio-cron-scheduler)
- Dynamic System Prompt with available tools listing

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

## Important Patterns and Preferences

- All UI text must go through i18n (react-i18next)
- No emojis in UI, code, or commits
- Typography-driven design: different fonts for different "voices"
- Rust-first backend: no Node.js sidecar (iOS compatibility)
- All data local, privacy by default
- CSS variables for theming, Tailwind for utility classes
- Agent uses `process_stream!` macro for uniform streaming across providers
- Rhai scripts receive parameters via `params_json` scope variable (parsed with `json_parse()`)
