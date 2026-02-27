# Progress - ownAI

## Overall Status

**Phase 1 (Foundation)**: Complete
**Phase 2 (Memory System)**: Complete
**Phase 3 (Self-Programming / Rhai)**: Complete
**Phase 4 (Deep Agent Features)**: Complete (all 7 steps: Canvas Backend + Protocol + Frontend + Bridge API + Sub-Agent System + Scheduled Tasks + Dynamic System Prompt)
**Phase 5-7 (Polish, Testing, Release)**: Not started

## What Works

### Frontend - Fully Functional
- [x] Tauri 2.0 + React + TypeScript + Vite setup
- [x] Tailwind CSS v4 design system with light/dark mode (prefers-color-scheme)
- [x] Complete chat interface (Message, MessageContent, MessageInput, MessageList)
- [x] Streaming display of agent responses (token-by-token via Tauri events)
- [x] Markdown rendering with syntax highlighting in agent messages
- [x] Auto-growing textarea with Enter to send, Shift+Enter for newline
- [x] Role-based typography (Noto Serif for agent, Noto Sans for user, Noto Sans Mono for system)
- [x] AI Instance selector (dropdown in header) and creation dialog
- [x] Settings panel (provider selection, model selection, API key input)
- [x] i18n with German and English translations
- [x] Header with search icon, settings, and instance selector
- [x] Zustand stores for chat and instance state
- [x] CSS animations (typing indicator pulse, tool execution pulse, smooth transitions)
- [x] Custom scrollbar styling
- [x] Welcome screen when no instances exist

### Backend - Core Functional
- [x] Rust backend with Tauri 2.0 integration
- [x] OwnAIAgent with rig-core 0.30 (supports Anthropic, OpenAI, Ollama)
- [x] Streaming chat via `agent:token` events with multi-turn support
- [x] Multi-turn tool calling (MAX_TOOL_TURNS = 50)
- [x] SQLite database with migrations (messages, user_profile, tools, tool_executions, programs; summaries + memory_entries created dynamically)
- [x] AI Instance Manager (create, list, update, delete instances)
- [x] Per-instance databases at `~/.ownai/instances/{id}/ownai.db`
- [x] API key storage via OS keychain (keyring crate)
- [x] AgentCache for lazy agent initialization per instance
- [x] 31 Tauri commands registered and functional
- [x] Working Memory with VecDeque and token budget (50000 tokens default, 30% eviction)
- [x] Summarization via LLM Extractor (rig Extractor with SummaryResponse JsonSchema struct)
- [x] Long-term Memory with fastembed (local Qwen3-Embedding-0.6B embeddings)
- [x] Context Builder (assembles context from all memory tiers)
- [x] Filesystem Tools (ls, read_file, write_file, edit_file, grep) with tests
- [x] Planning Tool (write_todos with SharedTodoList) with tests
- [x] Tools registered with agent via create_tools() helper (24 tools total for main agent, 20 for sub-agents)
- [x] process_stream! macro for uniform streaming across providers
- [x] Path utilities for cross-platform file management
- [x] Workspace directory per instance at `~/.ownai/instances/{id}/workspace/`
- [x] Programs directory per instance at `~/.ownai/instances/{id}/programs/`
- [x] Tracing/logging setup
- [x] Native OS notifications via tauri-plugin-notification (Rhai send_notification + Canvas Bridge notify)
- [x] Sandboxed Rhai scripting engine (14 safe functions, security limits)
- [x] Tool Registry (RhaiToolRegistry with SQLite, AST caching, execution logging, usage stats)
- [x] RhaiExecuteTool bridge (rig Tool -> Rhai script execution)
- [x] Dynamic tool Tauri commands (list, create, update, delete, execute via AgentCache)
- [x] Self-programming tools (CreateToolTool, ReadToolTool, UpdateToolTool)
- [x] Comprehensive system prompt with self-programming instructions, Rhai reference, and Canvas section
- [x] Canvas System backend (7 agent tools incl. open_program, DB table, filesystem storage)
- [x] Canvas Custom Protocol (`ownai-program://` URI scheme for serving files)
- [x] Canvas Tauri commands (list_programs, delete_program, get_program_url)
- [x] Canvas Frontend (CanvasPanel with iframe, ProgramList, canvasStore, split-view layout, auto-detection)
- [x] open_program tool (backend emits event, frontend opens program in split view)
- [x] Auto-reload on program updates (ProgramWriteFile/EditFile emit event, frontend refreshes iframe)
- [x] Bridge API (postMessage communication: window.ownai with chat, storeData, loadData, notify, readFile, writeFile)
- [x] Bridge script injection into HTML files served via ownai-program:// protocol
- [x] Per-program key-value storage (program_data table)
- [x] Bridge API system prompt documentation
- [x] Sub-Agent System (dynamic sub-agents via DelegateTaskTool, ClientProvider, base_tools_prompt)
- [x] Memory Tools (search_memory, add_memory, delete_memory as rig Tools for all agents)
- [x] SharedLongTermMemory (Arc<Mutex<LongTermMemory>>) for concurrent tool access
- [x] System prompt refactored to use base_tools_prompt() (no duplication)
- [x] Scheduled Tasks (tokio-cron-scheduler, cron validation via croner, SharedScheduler as Tauri state)
- [x] 3 scheduler rig Tools (create_scheduled_task, list_scheduled_tasks, delete_scheduled_task)
- [x] 3 scheduler Tauri commands (list_scheduled_tasks, delete_scheduled_task, toggle_scheduled_task)
- [x] Task execution via temporary agents with build_sub_agent_tools()
- [x] Tasks loaded and registered for all instances on app startup
- [x] Scheduled task result delivery: results saved as DB messages, OS notifications, live chat UI updates
- [x] `notify` flag on scheduled tasks (default true, LLM can set false for silent tasks)
- [x] Dynamic system prompt (all capabilities documented via base_tools_prompt())
- [x] SummarizationAgent with SummaryExtractor trait (clean architecture, owns summarization logic)
- [x] Summary embeddings (embedding BLOB column, auto-computed via fastembed on save)
- [x] Key facts from summaries auto-stored as MemoryEntry in long-term memory
- [x] Semantic summary search (cosine similarity on summary embeddings)
- [x] Context builder: 3 recent summaries + semantically relevant older summary (threshold 0.6)
- [x] Summary dates shown in context ("[YYYY-MM-DD] summary text")

## What's Left to Build

### Phase 5 - UI/UX Refinement
- [ ] Message list virtualization for performance (large conversation histories)
- [ ] Infinite scroll with lazy loading of older messages
- [ ] Onboarding flow for first-time users
- [ ] Canvas view toggle (split-view / fullscreen)
- [ ] ToolCallIndicator component improvements
- [ ] TodoList rendering in system messages
- [ ] Program management UI (list saved programs)

### Phase 6 - Testing & Stabilization
- [ ] Frontend tests (Vitest + React Testing Library)
- [ ] Backend integration tests for Tauri commands
- [ ] Rhai script validation tests
- [ ] Error handling improvements and user-friendly messages
- [ ] Retry logic for LLM calls
- [ ] Data migration system for schema updates

### Phase 7 - Polish & Release
- [ ] User documentation (EN + DE)
- [ ] API keys setup guide
- [ ] Build & packaging (macOS .dmg, Windows .exe/.msi, Linux .deb/.AppImage)
- [ ] Performance optimization

### Future (Post-MVP)
- [ ] Mobile port via Tauri Mobile (iOS/Android)
- [ ] Voice interface (Whisper + TTS)
- [ ] Multimodal support (images, audio)
- [ ] Cloud sync (end-to-end encrypted)
- [ ] Tool marketplace (community tools sharing)

## Known Issues

- No known blocking issues at this time

## Evolution of Project Decisions

1. **Fonts**: Changed from Newsreader/Inter/JetBrains Mono (design doc) to Noto Serif/Sans/Mono (implementation) for universal language support across 100+ scripts
2. **Vector Store**: Changed from rig-sqlite (plan) to fastembed + raw SQLite (implementation) for fully local, privacy-preserving embeddings
3. **Tailwind**: Using v4 syntax (`@theme`, `@utility`) instead of v3 config file approach
4. **State Management**: Chose Zustand over Jotai for simplicity
5. **Embedding Model**: Using Qwen3-Embedding-0.6B via fastembed for local on-device embedding generation
6. **Summarization**: Uses rig Extractor with JsonSchema struct (SummaryResponse) instead of raw LLM prompting
7. **Tool Registration**: Uses create_tools() helper + `.tools()` builder method instead of individual `.tool()` calls
8. **Provider Support**: Added Ollama alongside Anthropic and OpenAI from early on
9. **Rhai Safe Functions**: Expanded from 6 to 14 functions after reviewing use cases (added regex, base64, url_encode, datetime, notification, flexible http_request with headers)
10. **Per-Instance Tool Registry**: Tool commands access registry through AgentCache (not global Tauri state) for proper isolation
11. **Self-Programming Architecture**: Agent writes Rhai code directly and uses create_tool/read_tool/update_tool instead of a separate CodeGenerationAgent with internal LLM calls
12. **Canvas Program Identity**: Programs identified by name (not UUID), chosen by the agent, similar to dynamic Rhai tools
13. **Canvas Tool Design**: Filesystem-like tools (program_write_file, program_edit_file) instead of monolithic save/update, agent does not know instance_id
14. **Bridge API File Scope**: readFile/writeFile in Bridge API scoped to workspace directory (not program directory), giving Canvas programs access to shared workspace data
15. **Sub-Agent Architecture**: Evolved from predefined sub-agents to fully dynamic -- main agent creates sub-agents on the fly via delegate_task with custom system prompts. Sub-agents get ALL tools except delegate_task (prevents recursion). Tool documentation centralized in base_tools_prompt().
16. **SharedLongTermMemory**: Refactored LongTermMemory to Arc<Mutex<>> for concurrent access by memory tools, context builder, and fact extraction.
