# Implementation Plan

[Overview]
Complete the remaining memory system gaps (Phase 2), implement Rhai-based self-programming (Phase 3), and build Deep Agent features including Canvas, Sub-Agents, and Scheduled Tasks (Phase 4).

This plan covers the next three phases of ownAI development. Phase 1 (Foundation) is complete: the frontend chat UI, streaming, i18n, design system, multi-instance management, and basic agent with rig-core 0.30 are all functional. Phase 2 (Memory) is mostly complete: working memory, summarization via LLM Extractor, long-term memory with fastembed, and context builder all work. Filesystem tools (ls, read_file, write_file, edit_file, grep) and the planning tool (write_todos) are fully implemented and registered with the agent.

The remaining work falls into three categories:
1. **Phase 2 Completion**: Fix context duplication, reload working memory from DB, add automatic fact extraction, add missing memory Tauri commands, add importance scoring
2. **Phase 3 (Self-Programming)**: Activate Rhai scripting engine, create safe function wrappers, build Tool Registry with DB storage, implement Code Generation Agent, add Capability Detection
3. **Phase 4 (Deep Agent Features)**: Canvas system for HTML apps, Bridge API, Sub-Agent system, Scheduled Tasks (cron), Dynamic System Prompt

The implementation order is designed so each phase builds on the previous one. Memory fixes are prerequisites for everything. Rhai tools enable self-programming. Canvas and Sub-Agents leverage tools for visual and delegated workflows.

[Types]
New type definitions and modifications to existing types across both Rust backend and TypeScript frontend.

### Rust Backend - New Types

**Phase 2 - Memory Types** (in `src-tauri/src/memory/`):

```rust
// In working_memory.rs - add importance scoring
pub struct Message {
    pub id: String,
    pub role: String,
    pub content: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub importance_score: f32,  // NEW: 0.0-1.0, default 0.5
}

// In long_term.rs - new struct for fact extraction
pub struct ExtractedFact {
    pub content: String,
    pub entry_type: MemoryType,
    pub importance: f32,
    pub source_message_id: String,
}

// In long_term.rs - structured extraction response (for rig Extractor)
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct FactExtractionResponse {
    pub facts: Vec<ExtractedFactItem>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ExtractedFactItem {
    pub content: String,
    pub fact_type: String,  // "fact", "preference", "skill", "context"
    pub importance: f32,
}
```

**Phase 3 - Rhai/Tool Registry Types** (new file `src-tauri/src/tools/registry.rs`):

```rust
pub struct RhaiToolRegistry {
    engine: rhai::Engine,
    compiled_tools: HashMap<String, Arc<rhai::AST>>,
    db: Pool<Sqlite>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolRecord {
    pub id: String,
    pub name: String,
    pub description: String,
    pub version: String,
    pub script_content: String,       // Rhai script source code
    pub created_at: DateTime<Utc>,
    pub last_used: Option<DateTime<Utc>>,
    pub usage_count: i32,
    pub success_count: i32,
    pub failure_count: i32,
    pub status: ToolStatus,           // active, deprecated, testing
    pub parent_tool_id: Option<String>,
    pub parameters: Vec<ParameterDef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ToolStatus {
    Active,
    Deprecated,
    Testing,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParameterDef {
    pub name: String,
    pub type_hint: String,
    pub description: String,
    pub required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolExecutionRecord {
    pub id: String,
    pub tool_id: String,
    pub timestamp: DateTime<Utc>,
    pub success: bool,
    pub execution_time_ms: i64,
    pub error_message: Option<String>,
    pub input_params: serde_json::Value,
    pub output: Option<String>,
}
```

**Phase 3 - Code Generation Types** (new file `src-tauri/src/tools/code_generation.rs`):

```rust
pub struct CodeGenerationAgent {
    // Uses the same LLM provider as the main agent
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GeneratedTool {
    pub metadata: ToolMetadata,
    pub code: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ToolMetadata {
    pub name: String,
    pub description: String,
    pub parameters: Vec<ParameterDef>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CapabilityCheck {
    pub can_handle: bool,
    pub missing_capability: Option<String>,
    pub suggested_tool_name: Option<String>,
}
```

**Phase 4 - Canvas Types** (new file `src-tauri/src/canvas/mod.rs`):

```rust
pub struct ProgramMetadata {
    pub id: String,
    pub name: String,
    pub description: String,
    pub version: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub instance_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum BridgeRequest {
    Chat { prompt: String },
    StoreData { key: String, value: serde_json::Value },
    LoadData { key: String },
    Notify { message: String, delay_ms: Option<u64> },
    ReadFile { path: String },
    WriteFile { path: String, content: String },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BridgeResponse {
    pub request_id: String,
    pub success: bool,
    pub data: Option<serde_json::Value>,
    pub error: Option<String>,
}
```

**Phase 4 - Sub-Agent Types** (new file `src-tauri/src/tools/subagents.rs`):

```rust
pub struct SubAgentDefinition {
    pub name: String,
    pub description: String,
    pub system_prompt: String,
    pub model_override: Option<String>,
    pub max_iterations: usize,
}

pub struct SubAgentRegistry {
    definitions: HashMap<String, SubAgentDefinition>,
}
```

**Phase 4 - Scheduled Tasks Types** (new file `src-tauri/src/scheduler/mod.rs`):

```rust
pub struct ScheduledTask {
    pub id: String,
    pub instance_id: String,
    pub cron_expression: String,
    pub task_description: String,
    pub enabled: bool,
    pub last_run: Option<DateTime<Utc>>,
    pub next_run: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}
```

### TypeScript Frontend - New Types

```typescript
// In src/types/index.ts - additions

// Canvas/Program types
export interface Program {
  id: string;
  name: string;
  description: string;
  version: string;
  created_at: string;
  updated_at: string;
}

// Tool types (for UI display)
export interface ToolInfo {
  id: string;
  name: string;
  description: string;
  status: 'active' | 'deprecated' | 'testing';
  usage_count: number;
}

// Scheduled task types
export interface ScheduledTask {
  id: string;
  cron_expression: string;
  task_description: string;
  enabled: boolean;
  last_run: string | null;
  next_run: string | null;
}

// Memory search result
export interface MemorySearchResult {
  id: string;
  content: string;
  entry_type: string;
  importance: number;
  similarity: number;
}
```

[Files]
Files to be created, modified, and their purposes.

### New Files to Create

**Phase 2 - Memory Completion:**
- `src-tauri/src/memory/fact_extraction.rs` - Automatic fact extraction from conversations using rig Extractor

**Phase 3 - Self-Programming:**
- `src-tauri/src/tools/registry.rs` - RhaiToolRegistry: register, load, execute, list dynamic Rhai tools
- `src-tauri/src/tools/code_generation.rs` - CodeGenerationAgent: LLM generates Rhai scripts
- `src-tauri/src/tools/rhai_engine.rs` - Sandboxed Rhai engine setup with safe function wrappers
- `src-tauri/src/tools/rhai_bridge_tool.rs` - RhaiExecuteTool: rig Tool wrapper that executes Rhai scripts
- `src-tauri/src/commands/tools.rs` - Tauri commands for tool management (list, create, delete, execute)

**Phase 4 - Deep Agent Features:**
- `src-tauri/src/canvas/mod.rs` - Canvas module: program storage, metadata, serving
- `src-tauri/src/canvas/bridge.rs` - Bridge API handler: processes postMessage requests from iframe
- `src-tauri/src/tools/subagents.rs` - Sub-agent definitions and TaskTool
- `src-tauri/src/scheduler/mod.rs` - Scheduler module: cron task management
- `src-tauri/src/commands/canvas.rs` - Tauri commands for Canvas/program management
- `src-tauri/src/commands/scheduler.rs` - Tauri commands for scheduled tasks
- `src/components/canvas/Canvas.tsx` - Canvas iframe component
- `src/components/canvas/CanvasView.tsx` - Split-view layout (Chat + Canvas)
- `src/components/canvas/ProgramList.tsx` - List of saved programs
- `src/stores/canvasStore.ts` - Zustand store for Canvas state

### Existing Files to Modify

**Phase 2 - Memory:**
- `src-tauri/src/memory/mod.rs` - Add `pub mod fact_extraction;` and re-exports
- `src-tauri/src/memory/working_memory.rs` - Add `importance_score` field to Message, add `load_from_db()` method
- `src-tauri/src/memory/context_builder.rs` - Fix context duplication: remove working memory from context string (it's already sent as chat history)
- `src-tauri/src/memory/long_term.rs` - Add `delete()` and `search_by_type()` methods
- `src-tauri/src/agent/mod.rs` - Call `load_working_memory_from_db()` in `new()`, call fact extraction after each conversation turn
- `src-tauri/src/commands/memory.rs` - Add `search_memory`, `add_memory_entry`, `delete_memory_entry` commands
- `src-tauri/src/database/schema.rs` - Add `importance_score` column to messages table, add `instance_id` column

**Phase 3 - Self-Programming:**
- `src-tauri/src/tools/mod.rs` - Add `pub mod registry;`, `pub mod code_generation;`, `pub mod rhai_engine;`, `pub mod rhai_bridge_tool;`
- `src-tauri/src/agent/mod.rs` - Add RhaiExecuteTool to create_tools(), update system prompt with dynamic tool list
- `src-tauri/src/commands/mod.rs` - Add `pub mod tools;`
- `src-tauri/src/lib.rs` - Register new tool management Tauri commands
- `src-tauri/src/database/schema.rs` - Add `tools` and `tool_executions` tables
- `src-tauri/Cargo.toml` - Add `tokio-cron-scheduler` dependency (for Phase 4)

**Phase 4 - Deep Agent Features:**
- `src-tauri/src/lib.rs` - Add `pub mod canvas;`, `pub mod scheduler;`, register Canvas/scheduler commands
- `src-tauri/src/commands/mod.rs` - Add `pub mod canvas;`, `pub mod scheduler;`
- `src-tauri/src/agent/mod.rs` - Add TaskTool (sub-agents) to create_tools(), update system prompt for Canvas instructions
- `src-tauri/src/tools/mod.rs` - Add `pub mod subagents;`
- `src-tauri/src/database/schema.rs` - Add `programs`, `program_data`, `scheduled_tasks` tables
- `src-tauri/tauri.conf.json` - Add custom protocol for serving program files from instance directories
- `src-tauri/capabilities/default.json` - Add permissions for custom protocol
- `src/App.tsx` - Add Canvas view toggle, handle program display
- `src/components/layout/Header.tsx` - Add Canvas toggle button
- `src/types/index.ts` - Add Program, ToolInfo, ScheduledTask, MemorySearchResult types
- `src/locales/de/translation.json` - Add translations for Canvas, tools, scheduler
- `src/locales/en/translation.json` - Add translations for Canvas, tools, scheduler

[Functions]
New and modified functions organized by file.

### Phase 2 - Memory Completion

**New Functions:**

- `FactExtractionAgent::new(db: Pool<Sqlite>)` in `memory/fact_extraction.rs` - Initialize fact extractor
- `FactExtractionAgent::extract_facts(user_msg: &str, agent_response: &str) -> Result<Vec<ExtractedFact>>` in `memory/fact_extraction.rs` - Use rig Extractor to identify important facts from a conversation turn
- `FactExtractionAgent::store_extracted_facts(facts: Vec<ExtractedFact>)` in `memory/fact_extraction.rs` - Store extracted facts into long-term memory with embeddings
- `WorkingMemory::load_from_messages(messages: Vec<Message>)` in `memory/working_memory.rs` - Populate working memory from DB messages on agent init
- `LongTermMemory::delete(id: &str) -> Result<()>` in `memory/long_term.rs` - Delete a memory entry by ID
- `LongTermMemory::search_by_type(entry_type: MemoryType, limit: usize) -> Result<Vec<MemoryEntry>>` in `memory/long_term.rs` - Query memories by type
- `search_memory(instance_id, query, limit)` in `commands/memory.rs` - Tauri command for semantic memory search
- `add_memory_entry(instance_id, content, entry_type, importance)` in `commands/memory.rs` - Tauri command to manually add memory
- `delete_memory_entry(instance_id, entry_id)` in `commands/memory.rs` - Tauri command to delete memory entry

**Modified Functions:**

- `ContextBuilder::build_context()` in `memory/context_builder.rs` - Remove "## Recent Conversation" section that duplicates chat history. Keep only long-term memories and summaries in context string.
- `OwnAIAgent::new()` in `agent/mod.rs` - After creating the agent, load recent messages from DB into working memory via `load_from_messages()`
- `OwnAIAgent::chat()` in `agent/mod.rs` - After getting agent response, call fact extraction in background (tokio::spawn)
- `OwnAIAgent::stream_chat()` in `agent/mod.rs` - Same: call fact extraction after response complete
- `WorkingMemory::add_message()` in `memory/working_memory.rs` - Accept Message with importance_score field

### Phase 3 - Self-Programming (Rhai)

**New Functions:**

- `create_sandboxed_engine() -> Engine` in `tools/rhai_engine.rs` - Create Rhai engine with security limits and safe registered functions
- `safe_http_get(url: String) -> Result<String>` in `tools/rhai_engine.rs` - HTTPS-only GET with timeout
- `safe_http_post(url: String, body: String) -> Result<String>` in `tools/rhai_engine.rs` - HTTPS-only POST with timeout
- `safe_read_file(path: String) -> Result<String>` in `tools/rhai_engine.rs` - Read file within workspace
- `safe_write_file(path: String, content: String) -> Result<()>` in `tools/rhai_engine.rs` - Write file within workspace
- `RhaiToolRegistry::new(db) -> Result<Self>` in `tools/registry.rs` - Initialize registry
- `RhaiToolRegistry::register_tool(tool: ToolRecord) -> Result<()>` in `tools/registry.rs` - Save tool to DB and compile
- `RhaiToolRegistry::load_tool(tool_id: &str) -> Result<Arc<AST>>` in `tools/registry.rs` - Load and cache compiled tool
- `RhaiToolRegistry::execute_tool(tool_id, params) -> Result<String>` in `tools/registry.rs` - Execute a Rhai tool with params
- `RhaiToolRegistry::list_tools() -> Result<Vec<ToolRecord>>` in `tools/registry.rs` - List all active tools
- `RhaiToolRegistry::delete_tool(tool_id: &str) -> Result<()>` in `tools/registry.rs` - Deactivate a tool
- `RhaiToolRegistry::reload_all_tools() -> Result<()>` in `tools/registry.rs` - Clear cache and reload from DB
- `CodeGenerationAgent::generate_tool(requirement: &str) -> Result<GeneratedTool>` in `tools/code_generation.rs` - Use LLM to generate Rhai script
- `CodeGenerationAgent::validate_script(code: &str) -> Result<()>` in `tools/code_generation.rs` - Compile check + safety validation
- `RhaiExecuteTool` implementing `rig::tool::Tool` in `tools/rhai_bridge_tool.rs` - Agent-callable tool that finds and runs Rhai scripts from registry
- `list_dynamic_tools(instance_id)` in `commands/tools.rs` - Tauri command
- `create_dynamic_tool(instance_id, name, description, code)` in `commands/tools.rs` - Tauri command
- `delete_dynamic_tool(instance_id, tool_id)` in `commands/tools.rs` - Tauri command
- `execute_dynamic_tool(instance_id, tool_id, params)` in `commands/tools.rs` - Tauri command

**Modified Functions:**

- `create_tools()` in `agent/mod.rs` - Add RhaiExecuteTool to the tools vector, pass RhaiToolRegistry reference
- `OwnAIAgent::system_prompt()` in `agent/mod.rs` - Dynamically list available Rhai tools in the system prompt
- `schema::create_tables()` in `database/schema.rs` - Add CREATE TABLE for `tools` and `tool_executions`

### Phase 4 - Deep Agent Features

**New Functions:**

- `Canvas::save_program(instance_id, name, html_content) -> Result<ProgramMetadata>` in `canvas/mod.rs`
- `Canvas::load_program(instance_id, program_id) -> Result<String>` in `canvas/mod.rs`
- `Canvas::list_programs(instance_id) -> Result<Vec<ProgramMetadata>>` in `canvas/mod.rs`
- `Canvas::delete_program(instance_id, program_id) -> Result<()>` in `canvas/mod.rs`
- `Canvas::update_program(instance_id, program_id, html_content) -> Result<ProgramMetadata>` in `canvas/mod.rs`
- `BridgeHandler::handle_request(request: BridgeRequest) -> BridgeResponse` in `canvas/bridge.rs`
- `SubAgentRegistry::new(definitions) -> Self` in `tools/subagents.rs`
- `SubAgentRegistry::spawn_subagent(name, task) -> Result<String>` in `tools/subagents.rs`
- `TaskTool` implementing `rig::tool::Tool` in `tools/subagents.rs` - delegates to sub-agents
- `create_code_generation_subagent() -> SubAgentDefinition` in `tools/subagents.rs`
- `create_researcher_subagent() -> SubAgentDefinition` in `tools/subagents.rs`
- `create_memory_manager_subagent() -> SubAgentDefinition` in `tools/subagents.rs`
- `Scheduler::new(db) -> Result<Self>` in `scheduler/mod.rs`
- `Scheduler::add_task(task: ScheduledTask) -> Result<()>` in `scheduler/mod.rs`
- `Scheduler::remove_task(task_id: &str) -> Result<()>` in `scheduler/mod.rs`
- `Scheduler::list_tasks(instance_id: &str) -> Result<Vec<ScheduledTask>>` in `scheduler/mod.rs`
- `Scheduler::start() -> Result<()>` in `scheduler/mod.rs` - Start cron scheduler loop
- `save_program(instance_id, name, html)` in `commands/canvas.rs` - Tauri command
- `load_program(instance_id, program_id)` in `commands/canvas.rs` - Tauri command
- `list_programs(instance_id)` in `commands/canvas.rs` - Tauri command
- `delete_program(instance_id, program_id)` in `commands/canvas.rs` - Tauri command
- `create_scheduled_task(instance_id, cron, description)` in `commands/scheduler.rs` - Tauri command
- `list_scheduled_tasks(instance_id)` in `commands/scheduler.rs` - Tauri command
- `delete_scheduled_task(instance_id, task_id)` in `commands/scheduler.rs` - Tauri command
- `toggle_scheduled_task(instance_id, task_id, enabled)` in `commands/scheduler.rs` - Tauri command

**Modified Functions:**

- `create_tools()` in `agent/mod.rs` - Add TaskTool (sub-agents), Canvas tools (save_program, update_program)
- `OwnAIAgent::system_prompt()` in `agent/mod.rs` - Add Canvas/program creation instructions, sub-agent delegation instructions, cron task instructions
- `run()` in `lib.rs` - Register all new Tauri commands (canvas, scheduler, tools), initialize scheduler in setup

[Classes]
No traditional classes (Rust uses structs/impls, React uses functional components). See Types and Functions sections for struct definitions and their implementations.

Key new structs (acting as classes):
- `FactExtractionAgent` - Extracts facts from conversations into long-term memory
- `RhaiToolRegistry` - Manages dynamic Rhai tool lifecycle (register, compile, cache, execute)
- `CodeGenerationAgent` - Generates Rhai scripts via LLM
- `RhaiExecuteTool` - rig Tool wrapper for executing Rhai scripts
- `SubAgentRegistry` - Manages sub-agent definitions and spawning
- `TaskTool` - rig Tool that delegates to sub-agents
- `Canvas` - Program storage and management
- `BridgeHandler` - Handles postMessage requests from Canvas iframes
- `Scheduler` - Cron-based task scheduler

Key new React components:
- `Canvas` - iframe rendering component
- `CanvasView` - Split-view layout (Chat + Canvas)
- `ProgramList` - List of saved programs

[Dependencies]
New Rust and JavaScript dependencies needed.

### Rust (Cargo.toml additions)

```toml
# Phase 4 - Scheduled Tasks
tokio-cron-scheduler = "0.13"
```

No other new Rust dependencies needed - `rhai`, `reqwest`, `schemars` are already in Cargo.toml.

### JavaScript (package.json)

No new frontend dependencies needed for Phase 2-4. The existing stack (React, Zustand, Tailwind, react-markdown, etc.) covers all requirements.

[Testing]
Testing approach for each phase.

### Phase 2 - Memory Tests

- `src-tauri/src/memory/fact_extraction.rs` - Unit tests for fact parsing and type classification
- `src-tauri/src/memory/working_memory.rs` - Add tests for `load_from_messages()`, importance scoring
- `src-tauri/src/memory/context_builder.rs` - Test that context string no longer duplicates working memory
- `src-tauri/src/commands/memory.rs` - Test search_memory, add/delete memory entry commands

### Phase 3 - Rhai Tests

- `src-tauri/src/tools/rhai_engine.rs` - Test sandboxing: max operations, blocked unsafe features, safe function wrappers
- `src-tauri/src/tools/registry.rs` - Test tool CRUD: register, load, execute, delete, cache invalidation
- `src-tauri/src/tools/code_generation.rs` - Test script validation (compile check, safety check)
- `src-tauri/src/tools/rhai_bridge_tool.rs` - Test RhaiExecuteTool with mock scripts

### Phase 4 - Deep Agent Tests

- `src-tauri/src/canvas/mod.rs` - Test program CRUD: save, load, list, delete, update
- `src-tauri/src/canvas/bridge.rs` - Test bridge request/response handling
- `src-tauri/src/tools/subagents.rs` - Test sub-agent definition and delegation
- `src-tauri/src/scheduler/mod.rs` - Test task CRUD and cron expression parsing

### Validation Strategy

1. Each phase should compile and pass `cargo test` before moving to next
2. Run `cargo clippy` after each phase for lint checks
3. Manual testing via `pnpm tauri dev` to verify end-to-end integration
4. Verify all existing tests still pass after each modification

[Implementation Order]
The logical sequence of implementation to minimize conflicts and ensure each step builds on the previous.

### Phase 2: Memory System Completion (5 steps)

1. **Fix context duplication** - ✅ COMPLETE - Modified `ContextBuilder::build_context()` to remove the "## Recent Conversation" section. Working memory is already sent as chat history in `agent/mod.rs` via `with_history()`. The context string now only contains long-term memories and summaries.

2. **Add importance scoring to messages** - ✅ COMPLETE - Added `importance_score: f32` field to `working_memory::Message`. Updated `database/schema.rs` to include the column with DEFAULT 0.5. All message creations now set importance_score to 0.5.

3. **Reload working memory from DB on agent init** - ✅ COMPLETE - Added `WorkingMemory::load_from_messages()` method with token budget respect. In `OwnAIAgent::new()`, the agent now queries the most recent 100 messages from the database and loads them into working memory. Added `load_recent_messages_from_db()` helper function. This ensures conversation continuity across restarts.

4. **Implement automatic fact extraction** - ✅ COMPLETE - Created `memory/fact_extraction.rs` with `FactExtractionResponse` and helper functions. Added `FactExtractorProvider` enum in `agent/mod.rs`. Integrated fact extraction in both `chat()` and `stream_chat()` methods. Facts are automatically extracted and stored in long-term memory after each conversation turn.

5. **Add missing memory Tauri commands** - ✅ COMPLETE - Added `search_memory`, `add_memory_entry`, `delete_memory_entry` commands to `commands/memory.rs`. Registered all three in `lib.rs`. Implemented `LongTermMemory::delete()`, `LongTermMemory::search_by_type()`, and `LongTermMemory::count()` methods with full test coverage.

### Phase 3: Self-Programming with Rhai (6 steps)

6. **Create sandboxed Rhai engine** - ✅ COMPLETE - Created `tools/rhai_engine.rs` with `create_sandboxed_engine(workspace: PathBuf)`. Security limits: max_operations (100,000), max_string_size (1MB), max_array_size (10,000), max_map_size (5,000). Registered 14 safe functions: `http_get`, `http_post`, `http_request` (flexible with custom method/headers/body), `read_file` (workspace-scoped), `write_file` (workspace-scoped), `json_parse`, `json_stringify`, `regex_match`, `regex_replace`, `base64_encode`, `base64_decode`, `url_encode`, `get_current_datetime`, `send_notification`. All HTTP functions enforce HTTPS with 30s timeout. All file functions block path traversal and absolute paths. 22 unit tests.

7. **Create Tool Registry with DB storage** - ✅ COMPLETE - Created `tools/registry.rs` with `RhaiToolRegistry`. Added `tools` and `tool_executions` tables to `database/schema.rs` with index on tool_executions(tool_id). Implemented: `register_tool()` (validates script, stores in DB, caches compiled AST), `execute_tool()` (AST caching, scope injection via `params_json`, execution logging, usage stats), `list_tools()` (with status filter), `get_tool()`, `delete_tool()` (soft-delete to deprecated), `clear_cache()`, `tool_summary()`. 11 async tests with in-memory SQLite.

8. **Create RhaiExecuteTool** - ✅ COMPLETE - Created `tools/rhai_bridge_tool.rs` implementing `rig::tool::Tool`. NAME = "execute_dynamic_tool". Args: `tool_name` (String) + `parameters` (serde_json::Value). The `definition()` method dynamically lists available tools from the registry. The `call()` method delegates to `registry.execute_tool()`. Uses `SharedRegistry = Arc<RwLock<RhaiToolRegistry>>` for concurrent access. 4 tests.

9. **Create Code Generation Tools** - ✅ COMPLETE - Created `tools/code_generation.rs` with three rig Tools for self-programming: `CreateToolTool` (name="create_tool", validates Rhai script via sandboxed compilation, registers in registry), `ReadToolTool` (name="read_tool", returns source code + metadata + usage stats), `UpdateToolTool` (name="update_tool", validates new script, increments version, updates DB + cache). Added `validate_script()` function with compilation check and loop heuristic warnings. Added `update_tool()` method and `increment_version()` helper to `RhaiToolRegistry`. Architecture: instead of a separate LLM call within a tool, the agent itself writes Rhai code and uses these tools to register, inspect, and iterate. 11 tests covering create/read/update success and failure cases. Added `update_dynamic_tool` Tauri command (5 tool commands total, 24 Tauri commands total).

10. **Integrate Rhai tools with agent** - ✅ COMPLETE - Modified `create_tools()` in `agent/mod.rs` to accept `SharedRegistry` and `available_dynamic_tools` parameters, includes `RhaiExecuteTool` in the tools vector. Added `tool_registry: SharedRegistry` field to `OwnAIAgent` struct with public accessor. Registry initialized per-instance in `OwnAIAgent::new()`. Created `commands/tools.rs` with 4 Tauri commands (`list_dynamic_tools`, `create_dynamic_tool`, `delete_dynamic_tool`, `execute_dynamic_tool`) accessing registry through AgentCache (per-instance). Registered commands in `lib.rs` (now 23 total commands).

11. **Add Capability Detection** - ✅ COMPLETE - Completely rewrote `OwnAIAgent::system_prompt()` with comprehensive self-programming instructions. The system prompt now includes: Core Identity, Available Tools (filesystem, planning, dynamic tool execution, self-programming), Self-Programming section (When to Create a Tool, How to Create a Tool, Iterating on Tools), Rhai Language Reference (all 14 built-in functions with descriptions), Security Constraints, Example tool creation workflow, Memory System explanation, and Response Guidelines. The agent is now instructed to use `create_tool` -> `execute_dynamic_tool` -> `read_tool` -> `update_tool` workflow for self-programming. Agent has 10 tools total: 5 filesystem, 1 planning, 1 dynamic executor, 3 self-programming (create/read/update).

### Phase 4: Deep Agent Features (7 steps)

12. **Canvas System - Backend** - ✅ COMPLETE - Created `canvas/` module with 4-file structure: `mod.rs` (ProgramMetadata struct, resolve_program_path with path traversal prevention, 6 tests), `storage.rs` (DB CRUD: create_program_in_db, list_programs_from_db, get_program_by_name, delete_program_from_db, update_program_version with semver patch bump, 8 tests), `protocol.rs` (parse_protocol_url, load_program_file, guess_mime_type for 18 file types, 8 tests), `tools.rs` (6 rig Tools: CreateProgramTool, ListProgramsTool, ProgramLsTool, ProgramReadFileTool, ProgramWriteFileTool, ProgramEditFileTool, 10 tests). Programs identified by name (not UUID), chosen by the agent. Agent does NOT know its instance_id - embedded at tool construction time. Tools use `#[serde(skip)]` for non-serializable fields (Pool, PathBuf). Added `programs` table to DB schema with UNIQUE(instance_id, name) constraint and index. Added `get_instance_programs_path()` and `get_program_path()` to utils/paths.rs. Programs stored at `~/.ownai/instances/{id}/programs/{program_name}/`. Updated `create_tools()` to accept `db` and `programs_root` params (16 tools total). Updated system prompt with Canvas Programs section. Created 3 Tauri commands in `commands/canvas.rs`: list_programs, delete_program, get_program_url (27 commands total). All 115 tests pass, cargo clippy clean.

13. **Canvas System - Custom Protocol** - ✅ COMPLETE - Registered `ownai-program://` custom URI scheme via `register_asynchronous_uri_scheme_protocol` in `lib.rs`. Protocol parses URLs as `ownai-program://localhost/{instance_id}/{program_name}/{path}`. Uses synchronous file I/O (std::fs::read) for serving local files. Returns proper HTTP responses with Content-Type headers (MIME detection for 18 file types), CORS headers, and appropriate error status codes (400, 404, 500). Protocol handler resolves programs root via `paths::get_instance_programs_path()` and delegates to `protocol::load_program_file()` for secure file serving.

14. **Canvas System - Frontend** - ✅ COMPLETE - Created `CanvasPanel.tsx` (sandboxed iframe with toolbar, program name/version display, fullscreen toggle), `ProgramList.tsx` (program selection list with inline delete confirmation), and `canvasStore.ts` (Zustand store with programs, activeProgram, programUrl, viewMode state). Modified `App.tsx` for split-view layout (chat-only / split / canvas-fullscreen) with auto-detection of new programs after streaming. Modified `Header.tsx` with `PanelRight` toggle button (visible when programs exist or canvas is open, accent-colored when active). Added `Program` and `CanvasViewMode` types to `types/index.ts`. Added i18n translations (EN + DE) for canvas section. Iframe uses `sandbox="allow-scripts allow-forms allow-modals allow-same-origin"` with `ownai-program://` protocol. All frontend checks pass (tsc, eslint, prettier).

15. **Bridge API** - ✅ COMPLETE - Created `canvas/bridge.rs` with full postMessage-based communication between Canvas iframes and the Rust backend. Implemented 6 Bridge methods: chat() (delegates to OwnAIAgent via AgentCache), storeData()/loadData() (per-program key-value storage in new `program_data` DB table), notify() (logging placeholder), readFile()/writeFile() (workspace-scoped with path traversal prevention). The `bridge_script()` function returns a `<script>` block providing `window.ownai` API object with Promise-based methods. Bridge JavaScript is automatically injected into HTML files served via `ownai-program://` protocol (in `protocol.rs` `inject_bridge_script()`). Frontend `CanvasPanel.tsx` listens for `ownai-bridge-request` postMessage events from iframes, calls `invoke("bridge_request", ...)`, and sends `ownai-bridge-response` back. System prompt updated with Bridge API documentation. 28 Tauri commands total. 20 bridge-specific tests + 4 protocol injection tests. All 140 tests pass, cargo clippy clean.

16. **Sub-Agent System** - Create `tools/subagents.rs` with `SubAgentRegistry`, `TaskTool`, and predefined sub-agent definitions (code-writer, researcher, memory-manager). The TaskTool is a rig Tool that the main agent can call to delegate tasks. Each sub-agent gets its own system prompt and can use filesystem tools.

17. **Scheduled Tasks** - Create `scheduler/mod.rs` with `Scheduler` using `tokio-cron-scheduler`. Tasks trigger LLM calls or tool executions. Results are saved as system messages or trigger notifications. Add `scheduled_tasks` table. Create Tauri commands. Initialize scheduler in `lib.rs` setup.

18. **Dynamic System Prompt** - Final integration: update `OwnAIAgent::system_prompt()` to include all capabilities: available Rhai tools (from registry), Canvas program instructions, sub-agent delegation, cron task management. The prompt should be assembled dynamically based on what's available.

---

## Remaining Phases (Not Covered in This Plan)

**IMPORTANT**: When the implementation of Phases 2-4 is complete, please ask the user to request a new implementation plan for the remaining phases:

### Phase 5 - UI/UX Refinement (not planned yet)
- Message list virtualization (react-virtual) for performance with large histories
- Infinite scroll with lazy loading of older messages
- Onboarding flow for first-time users
- Canvas view toggle (split-view / fullscreen within app window)
- ToolCallIndicator component improvements (show tool name, status, duration)
- TodoList rendering in system messages (parse write_todos output)
- Program management UI (list, delete, open saved Canvas programs)

### Phase 6 - Testing & Stabilization (not planned yet)
- Frontend tests (Vitest + React Testing Library)
- Backend integration tests for all Tauri commands
- Rhai script validation tests
- Error handling improvements and user-friendly messages
- Retry logic for LLM calls
- Data migration system for schema updates

### Phase 7 - Polish & Release (not planned yet)
- User documentation (EN + DE)
- API keys setup guide
- Build & packaging (macOS .dmg, Windows .exe/.msi, Linux .deb/.AppImage)
- Performance optimization

### Future (Post-MVP)
- Mobile port via Tauri Mobile (iOS/Android)
- Voice interface (Whisper + TTS)
- Multimodal support (images, audio)
- Cloud sync (end-to-end encrypted)
- Tool marketplace (community tools sharing)
