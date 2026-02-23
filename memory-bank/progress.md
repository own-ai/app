# Progress - ownAI

## Overall Status

**Phase 1 (Foundation)**: Complete
**Phase 2 (Memory System)**: Complete
**Phase 3 (Self-Programming / Rhai)**: Complete
**Phase 4 (Deep Agent Features)**: Steps 12-15 complete (Canvas Backend + Protocol + Frontend + Bridge API), next: Step 16 (Sub-Agent System)
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
- [x] 28 Tauri commands registered and functional
- [x] Working Memory with VecDeque and token budget (50000 tokens default, 30% eviction)
- [x] Summarization via LLM Extractor (rig Extractor with SummaryResponse JsonSchema struct)
- [x] Long-term Memory with fastembed (local Qwen3-Embedding-0.6B embeddings)
- [x] Context Builder (assembles context from all memory tiers)
- [x] Filesystem Tools (ls, read_file, write_file, edit_file, grep) with tests
- [x] Planning Tool (write_todos with SharedTodoList) with tests
- [x] Tools registered with agent via create_tools() helper (17 tools total)
- [x] process_stream! macro for uniform streaming across providers
- [x] Path utilities for cross-platform file management
- [x] Workspace directory per instance at `~/.ownai/instances/{id}/workspace/`
- [x] Programs directory per instance at `~/.ownai/instances/{id}/programs/`
- [x] Tracing/logging setup
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

## What's Left to Build

### Phase 4 - Deep Agent Features
- [ ] **Sub-Agent System**: TaskTool for delegating to specialized sub-agents
  - [ ] code-writer sub-agent
  - [ ] researcher sub-agent
  - [ ] memory-manager sub-agent
- [ ] **Scheduled Tasks**: Cron-like system
  - [ ] tokio-cron-scheduler integration
  - [ ] Task creation/management via chat
  - [ ] System notifications for task results
- [ ] **Dynamic System Prompt**: Include available tools listing

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
- send_notification is a placeholder (logs only) until Tauri notification plugin is integrated

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
