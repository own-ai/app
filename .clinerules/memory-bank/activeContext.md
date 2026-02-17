# Active Context - ownAI

## Current Work Focus

The project has **completed Phase 1 (Foundation)** and the **core of Phase 2 (Memory System)**. All basic infrastructure is functional including chat, streaming, multi-instance management, and the memory hierarchy. The **Filesystem Tools and Planning Tool are fully implemented**. The focus is now on completing remaining memory gaps and moving into Phase 3 (Self-Programming) and Phase 4 (Deep Agent Features including Canvas).

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
- SQLite database with schema (messages, user_profile; summaries and memory_entries created dynamically)
- AI Instance Manager with per-instance databases at `~/.ownai/instances/{id}/`
- API key storage via OS keychain (keyring crate)
- Working Memory (VecDeque with configurable token budget, default 50000 tokens, 30% eviction)
- Summarization via LLM Extractor (rig Extractor with SummaryResponse JsonSchema struct)
- Long-term Memory with local embeddings (fastembed with `Qwen3-Embedding-0.6B` model)
- Context Builder (assembles context from working memory + summaries + long-term memory)
- **Filesystem Tools**: ls, read_file, write_file, edit_file, grep - with security (path traversal prevention), tests, and registered with agent
- **Planning Tool**: write_todos with SharedTodoList, status tracking, markdown output, tests, and registered with agent
- 15 registered Tauri commands (instances CRUD, providers, API keys, chat, memory stats)
- AgentCache for multi-instance agent management
- Workspace directory per instance at `~/.ownai/instances/{id}/workspace/`

## Recent Changes

- **Phase 2 Steps 1-3 COMPLETED**:
  - Fixed context duplication: removed "Recent Conversation" from context_builder.rs (messages now only sent once via with_history())
  - Added importance_score field (f32, default 0.5) to Message struct and database schema
  - Implemented working memory reload from DB on agent initialization (load_from_messages() method)
  - Agent now loads last 100 messages from DB on startup for conversation continuity
- Filesystem tools fully implemented with directory traversal protection and recursive grep
- Planning tool fully implemented with TodoList, TodoItem, TodoStatus
- Agent system prompt includes instructions for using filesystem and planning tools
- Multi-turn tool calling enabled (MAX_TOOL_TURNS = 50)
- Summarization uses rig Extractor for structured LLM output (SummaryResponse)
- Ollama provider support added alongside Anthropic and OpenAI

## Next Steps

### Immediate (Phase 2 Completion - Memory Gaps)
- Implement automatic fact extraction from conversations to long-term memory (Step 4)
- Add missing memory Tauri commands: search_memory, add_memory_entry, delete_memory_entry (Step 5)
- Add LongTermMemory::delete() and search_by_type() methods

### Near-term (Phase 3 - Self-Programming with Rhai)
- Rhai sandboxed engine setup (dependency exists but no code)
- Safe Rust function wrappers (http_get, http_post, etc.)
- Tool Registry with DB storage (tools + tool_executions tables)
- Dynamic tool loading and caching
- Code Generation Agent (LLM generates Rhai scripts)
- Capability Detection (identify when new tool is needed)

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

## Important Patterns and Preferences

- All UI text must go through i18n (react-i18next)
- No emojis in UI, code, or commits
- Typography-driven design: different fonts for different "voices"
- Rust-first backend: no Node.js sidecar (iOS compatibility)
- All data local, privacy by default
- CSS variables for theming, Tailwind for utility classes
- Agent uses `process_stream!` macro for uniform streaming across providers
