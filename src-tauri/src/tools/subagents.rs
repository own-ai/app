//! Sub-agent system for task delegation.
//!
//! Provides `DelegateTaskTool`, a rig Tool that the main agent can call to
//! create temporary sub-agents for complex tasks. Sub-agents get their own
//! system prompt (written by the main agent) and access to all available tools,
//! keeping the main conversation context clean.

use rig::client::CompletionClient;
use rig::completion::{Prompt, ToolDefinition};
use rig::providers::{anthropic, ollama, openai};
use rig::tool::{Tool, ToolDyn};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{Pool, Sqlite};
use std::path::PathBuf;
use tauri::AppHandle;

use crate::canvas::tools::{
    CreateProgramTool, ListProgramsTool, OpenProgramTool, ProgramEditFileTool, ProgramLsTool,
    ProgramReadFileTool, ProgramWriteFileTool,
};
use crate::memory::SharedLongTermMemory;
use crate::tools::code_generation::{CreateToolTool, ReadToolTool, UpdateToolTool};
use crate::tools::filesystem::{EditFileTool, GrepTool, LsTool, ReadFileTool, WriteFileTool};
use crate::tools::memory_tools::{AddMemoryTool, DeleteMemoryTool, SearchMemoryTool};
use crate::tools::planning::{self, ReadTodosTool, WriteTodosTool};
use crate::tools::rhai_bridge_tool::{RhaiExecuteTool, SharedRegistry};
use crate::utils::paths;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct SubAgentError(String);

// ---------------------------------------------------------------------------
// ClientProvider -- wraps provider-specific rig clients
// ---------------------------------------------------------------------------

/// Provider-agnostic wrapper for LLM clients.
/// Used to create sub-agents with the same provider as the main agent.
#[derive(Clone)]
pub enum ClientProvider {
    Anthropic(anthropic::Client),
    OpenAI(openai::Client),
    Ollama(ollama::Client),
}

// ---------------------------------------------------------------------------
// Sub-agent tool builder
// ---------------------------------------------------------------------------

/// Maximum number of multi-turn iterations for sub-agent tool calling.
const SUB_AGENT_MAX_TURNS: usize = 25;

/// Build the full set of tools for a sub-agent.
/// Sub-agents get all tools except `delegate_task` (to prevent recursion).
/// Also used by the scheduler runner for task execution agents.
pub fn build_sub_agent_tools(
    instance_id: &str,
    registry: SharedRegistry,
    available_dynamic_tools: Vec<(String, String)>,
    db: Pool<Sqlite>,
    programs_root: PathBuf,
    long_term_memory: SharedLongTermMemory,
    app_handle: Option<AppHandle>,
) -> Vec<Box<dyn ToolDyn>> {
    let workspace =
        paths::get_instance_workspace_path(instance_id).unwrap_or_else(|_| PathBuf::from("."));
    let todo_list = planning::create_shared_todo_list();

    vec![
        // Filesystem tools
        Box::new(LsTool::new(workspace.clone())),
        Box::new(ReadFileTool::new(workspace.clone())),
        Box::new(WriteFileTool::new(workspace.clone())),
        Box::new(EditFileTool::new(workspace.clone())),
        Box::new(GrepTool::new(workspace.clone())),
        // Planning tools
        Box::new(ReadTodosTool::new(todo_list.clone())),
        Box::new(WriteTodosTool::new(todo_list)),
        // Dynamic Rhai tool executor
        Box::new(RhaiExecuteTool::new(
            registry.clone(),
            available_dynamic_tools,
        )),
        // Self-programming: create, read, and update dynamic tools
        Box::new(CreateToolTool::new(registry.clone(), workspace.clone())),
        Box::new(ReadToolTool::new(registry.clone())),
        Box::new(UpdateToolTool::new(registry, workspace)),
        // Canvas program tools
        Box::new(CreateProgramTool::new(
            db.clone(),
            instance_id.to_string(),
            programs_root.clone(),
            app_handle.clone(),
        )),
        Box::new(ListProgramsTool::new(db.clone(), instance_id.to_string())),
        Box::new(OpenProgramTool::new(
            db.clone(),
            instance_id.to_string(),
            app_handle.clone(),
        )),
        Box::new(ProgramLsTool::new(programs_root.clone())),
        Box::new(ProgramReadFileTool::new(programs_root.clone())),
        Box::new(ProgramWriteFileTool::new(
            db.clone(),
            instance_id.to_string(),
            programs_root.clone(),
            app_handle.clone(),
        )),
        Box::new(ProgramEditFileTool::new(
            db,
            instance_id.to_string(),
            programs_root,
            app_handle,
        )),
        // Memory tools
        Box::new(SearchMemoryTool::new(long_term_memory.clone())),
        Box::new(AddMemoryTool::new(long_term_memory.clone())),
        Box::new(DeleteMemoryTool::new(long_term_memory)),
    ]
}

// ---------------------------------------------------------------------------
// Shared tool documentation
// ---------------------------------------------------------------------------

/// Returns the shared tool documentation that is included in every agent's
/// system prompt (both the main agent and sub-agents).
pub fn base_tools_prompt() -> String {
    r#"## Available Tools

### Filesystem (Workspace)
- **ls**: List files and directories in your workspace
- **read_file**: Read file contents (supports line ranges)
- **write_file**: Write content to a file (creates dirs if needed)
- **edit_file**: Replace text in a file (old_text -> new_text)
- **grep**: Search for text patterns in files

IMPORTANT: You can ONLY access files within your workspace directory. If the user asks you to read, write, or access files at absolute paths or outside the workspace, you MUST decline and explain that for security reasons you can only access files within the workspace.
Tell the user they can click the "Open Workspace" button (📂) in the header to open the workspace folder in their file manager, and then copy or move the needed files into the workspace.

Use the workspace to:
- Save research results, notes, or data
- Create and manage files for the user
- Offload large information from context

### Planning
- **read_todos**: Read the current TODO list (all items with their statuses)
- **write_todos**: Create/update a TODO list for multi-step tasks

The active TODO list is automatically included in your context when it exists, so you always know your current plan. Use read_todos if you need to explicitly inspect the list.

Use write_todos when:
- A task requires more than 2-3 steps
- You need to track progress on complex work
- You discover new requirements mid-task

### Dynamic Tool Execution
- **execute_dynamic_tool**: Run a previously created dynamic tool by name

### Self-Programming (Tool Management)
- **create_tool**: Create a new dynamic tool from Rhai script code
- **read_tool**: Read the source code and metadata of an existing tool
- **update_tool**: Update/fix an existing tool's Rhai script code

### Long-Term Memory
- **search_memory**: Search long-term memory using semantic similarity
- **add_memory**: Store a new entry in long-term memory (facts, preferences, skills, context)
- **delete_memory**: Delete a memory entry by its ID

Use memory tools to:
- Remember important facts about the user or their projects
- Store and retrieve knowledge across conversations
- Organize information that should persist long-term
- Clean up outdated or incorrect memories

## Self-Programming

You can extend your own capabilities by creating dynamic tools written in Rhai (a lightweight scripting language). This is one of your most powerful features.

### When to Create a Tool
- A task requires calling external APIs (HTTP requests)
- Data processing that your built-in tools do not cover
- Recurring tasks that benefit from automation
- The user explicitly asks you to create a new capability
- You need to combine multiple operations into a single reusable step

### How to Create a Tool
1. Analyze what the tool needs to do
2. Write a Rhai script (see language reference below)
3. Call `create_tool` with a name, description, the script, and parameter definitions
4. Test the tool with `execute_dynamic_tool`
5. If it does not work correctly, use `read_tool` to inspect the code, then `update_tool` to fix it

### Iterating on Tools
If a dynamic tool produces unexpected results or errors:
1. Call `read_tool` to see the current source code and usage stats
2. Identify the issue in the Rhai script
3. Call `update_tool` with the corrected code
4. Re-test with `execute_dynamic_tool`

### Rhai Language Reference

Scripts receive parameters through the `params_json` scope variable:
```
let params = json_parse(params_json);
let value = params["key"];
```

Available built-in functions:
- **http_get(url)**: HTTPS GET request, returns response body
- **http_post(url, body)**: HTTPS POST with JSON body
- **http_request(method, url, headers, body)**: Flexible HTTP with custom method/headers
- **read_file(path)**: Read file from workspace
- **write_file(path, content)**: Write file to workspace
- **json_parse(text)**: Parse JSON string to object/array
- **json_stringify(value)**: Convert value to JSON string
- **regex_match(text, pattern)**: Find all regex matches
- **regex_replace(text, pattern, replacement)**: Replace regex matches
- **base64_encode(text)**: Encode string to Base64
- **base64_decode(text)**: Decode Base64 to string
- **url_encode(text)**: URL-encode a string
- **get_current_datetime()**: Get current UTC datetime (ISO 8601)
- **send_notification(title, body)**: Queue a system notification

Security constraints:
- All HTTP requests must use HTTPS
- File operations are restricted to the workspace directory
- Scripts are terminated after 100,000 operations (prevents infinite loops)

### Example: Creating a Simple Tool
To create a tool that fetches a URL and extracts the title:
1. Call create_tool with name="fetch_title", description="Fetches a URL and returns the page title"
2. Script: `let params = json_parse(params_json); let body = http_get(params["url"]); let matches = regex_match(body, "<title>(.*?)</title>"); if matches.len() > 0 { matches[0] } else { "No title found" }`
3. Parameters: [{"name": "url", "type_hint": "string", "description": "URL to fetch", "required": true}]

## Canvas Programs (Visual Apps)

You can create interactive HTML/CSS/JS applications that the user can see and interact with in an embedded view. These are called "Programs."

### Canvas Tools
- **create_program**: Create a new program with an initial index.html
- **list_programs**: List all programs you have created
- **open_program**: Open an existing program in the Canvas panel for the user to see
- **program_ls**: List files within a program directory
- **program_read_file**: Read the contents of a file in a program
- **program_write_file**: Write/create a file in a program (bumps version, auto-reloads in frontend)
- **program_edit_file**: Edit a file with search/replace (bumps version, auto-reloads in frontend)

### When to Use an Existing Program
IMPORTANT: Before creating a new program, always call `list_programs` first to check if a suitable
program already exists. If the user asks for something that matches an existing program, use
`open_program` to display it instead of creating a new one.

### When to Create a Program
- The user needs a visual interface (dashboard, form, game, chart)
- A task benefits from interactive HTML rather than plain text
- The user asks you to "show", "display", or "build" something visual
- No suitable existing program was found via `list_programs`
- Examples: chess board, expense tracker, data dashboard, quiz app

### How to Create a Program
1. Call `create_program` with a descriptive name, description, and initial HTML
2. The HTML should be a complete, self-contained page (inline CSS/JS or use separate files)
3. Use `program_write_file` to add CSS, JavaScript, or other files
4. Use `program_edit_file` for targeted modifications to existing files
5. Use `program_read_file` to inspect current file contents before editing

### Bridge API (window.ownai)

Every Canvas program automatically has access to `window.ownai`, a JavaScript API for communicating with the backend. Programs can use these methods:

- **window.ownai.chat(prompt)**: Send a message to you (the AI agent) and get a response. Useful for programs that need AI-generated content.
- **window.ownai.storeData(key, value)**: Persist a key-value pair for this program. Data is stored in the database and survives page reloads.
- **window.ownai.loadData(key)**: Load a previously stored value by key. Returns null if the key does not exist.
- **window.ownai.notify(message, delay_ms?)**: Show a notification to the user. Optional delay in milliseconds.
- **window.ownai.readFile(path)**: Read a file from the workspace directory. Path must be relative.
- **window.ownai.writeFile(path, content)**: Write a file to the workspace directory. Creates parent directories if needed.

All methods return Promises. Example usage in a program:
```javascript
// Save game state
await window.ownai.storeData("score", 42);
// Load it back
const score = await window.ownai.loadData("score");
// Ask the AI something
const answer = await window.ownai.chat("Suggest a next move");
// Read workspace data
const data = await window.ownai.readFile("data.json");
```

When creating programs that need persistence, use storeData/loadData. When programs need to interact with workspace files, use readFile/writeFile. The chat method is useful for AI-powered features within programs.

### Best Practices (Canvas)
- Use semantic, lowercase names with hyphens (e.g. "expense-tracker", "chess-board")
- Start with a working index.html, then iterate
- For complex apps, separate HTML, CSS, and JS into different files
- The user can view the program in a Canvas iframe beside the chat
- Use the Bridge API (window.ownai) for persistence, AI interaction, and file access

## Scheduled Tasks

You can create recurring tasks that run automatically on a cron schedule.

### Scheduler Tools
- **create_scheduled_task**: Create a new recurring task with a cron expression and a prompt
- **list_scheduled_tasks**: List all scheduled tasks for the current instance
- **delete_scheduled_task**: Delete a scheduled task by ID

### When to Use Scheduled Tasks
- The user wants something to happen regularly (e.g. daily summaries, weekly reports)
- A task should run autonomously without the user being present
- Periodic checks or maintenance operations

### How It Works
1. Call `create_scheduled_task` with a name, cron expression, and task prompt
2. Each time the schedule triggers, a temporary agent executes the prompt
3. The temporary agent has access to all tools (filesystem, memory, canvas, etc.)
4. Results are logged and can be viewed via `list_scheduled_tasks`

### Cron Expression Examples
- `0 8 * * *` -- every day at 8:00
- `0 9 * * 1` -- every Monday at 9:00
- `*/30 * * * *` -- every 30 minutes
- `0 0 1 * *` -- first day of every month at midnight
- `0 18 * * 1-5` -- weekdays at 18:00"#.to_string()
}

// ---------------------------------------------------------------------------
// DelegateTaskTool
// ---------------------------------------------------------------------------

/// Arguments for the delegate_task tool.
#[derive(Debug, Deserialize)]
pub struct DelegateTaskArgs {
    /// A short, descriptive name for the sub-agent task (for logging/tracking).
    task_name: String,
    /// The system prompt for the sub-agent. Should describe the sub-agent's
    /// role and approach. Tool documentation is appended automatically.
    system_prompt: String,
    /// The specific task to accomplish.
    task: String,
}

/// rig Tool that creates temporary sub-agents for task delegation.
///
/// The main agent calls this tool to delegate complex tasks to a sub-agent
/// that works independently with its own context window and has access to all
/// tools (except delegate_task itself).
#[derive(Clone, Serialize, Deserialize)]
pub struct DelegateTaskTool {
    #[serde(skip)]
    client: Option<ClientProvider>,
    #[serde(skip, default = "default_model")]
    model: String,
    #[serde(skip, default = "default_instance_id")]
    instance_id: String,
    #[serde(skip)]
    registry: Option<SharedRegistry>,
    #[serde(skip)]
    db: Option<Pool<Sqlite>>,
    #[serde(skip)]
    programs_root: Option<PathBuf>,
    #[serde(skip)]
    long_term_memory: Option<SharedLongTermMemory>,
    #[serde(skip)]
    app_handle: Option<AppHandle>,
}

fn default_model() -> String {
    String::new()
}

fn default_instance_id() -> String {
    String::new()
}

impl DelegateTaskTool {
    /// Create a new DelegateTaskTool with all required resources.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        client: ClientProvider,
        model: String,
        instance_id: String,
        registry: SharedRegistry,
        db: Pool<Sqlite>,
        programs_root: PathBuf,
        long_term_memory: SharedLongTermMemory,
        app_handle: Option<AppHandle>,
    ) -> Self {
        Self {
            client: Some(client),
            model,
            instance_id,
            registry: Some(registry),
            db: Some(db),
            programs_root: Some(programs_root),
            long_term_memory: Some(long_term_memory),
            app_handle,
        }
    }

    /// Build the full system prompt for a sub-agent by combining the custom
    /// prompt with the shared tool documentation.
    fn build_sub_agent_prompt(custom_prompt: &str) -> String {
        format!(
            "{custom}\n\n{tools}",
            custom = custom_prompt,
            tools = base_tools_prompt(),
        )
    }

    /// Run a sub-agent task with the given system prompt and task description.
    async fn run_sub_agent(
        &self,
        system_prompt: &str,
        task: &str,
        task_name: &str,
    ) -> Result<String, SubAgentError> {
        let client = self
            .client
            .as_ref()
            .ok_or_else(|| SubAgentError("Client not initialized".to_string()))?;
        let registry = self
            .registry
            .as_ref()
            .ok_or_else(|| SubAgentError("Registry not initialized".to_string()))?;
        let db = self
            .db
            .as_ref()
            .ok_or_else(|| SubAgentError("Database not initialized".to_string()))?;
        let programs_root = self
            .programs_root
            .as_ref()
            .ok_or_else(|| SubAgentError("Programs root not initialized".to_string()))?;
        let long_term_memory = self
            .long_term_memory
            .as_ref()
            .ok_or_else(|| SubAgentError("Long-term memory not initialized".to_string()))?;

        // Get available dynamic tools from registry
        let available_dynamic_tools = {
            let reg = registry.read().await;
            reg.tool_summary().await.unwrap_or_default()
        };

        // Build tools for the sub-agent (all tools except delegate_task)
        let tools = build_sub_agent_tools(
            &self.instance_id,
            registry.clone(),
            available_dynamic_tools,
            db.clone(),
            programs_root.clone(),
            long_term_memory.clone(),
            self.app_handle.clone(),
        );

        let full_prompt = Self::build_sub_agent_prompt(system_prompt);

        tracing::info!(
            "Starting sub-agent '{}' with {} tools",
            task_name,
            tools.len()
        );

        // Build and run provider-specific agent
        let result = match client {
            ClientProvider::Anthropic(c) => {
                let agent = c
                    .agent(&self.model)
                    .preamble(&full_prompt)
                    .max_tokens(32768)
                    .temperature(0.7)
                    .tools(tools)
                    .build();

                agent
                    .prompt(task)
                    .max_turns(SUB_AGENT_MAX_TURNS)
                    .await
                    .map_err(|e| SubAgentError(format!("Sub-agent execution failed: {}", e)))?
            }
            ClientProvider::OpenAI(c) => {
                let agent = c
                    .clone()
                    .completions_api()
                    .agent(&self.model)
                    .preamble(&full_prompt)
                    .temperature(0.7)
                    .tools(tools)
                    .build();

                agent
                    .prompt(task)
                    .max_turns(SUB_AGENT_MAX_TURNS)
                    .await
                    .map_err(|e| SubAgentError(format!("Sub-agent execution failed: {}", e)))?
            }
            ClientProvider::Ollama(c) => {
                let agent = c
                    .clone()
                    .agent(&self.model)
                    .preamble(&full_prompt)
                    .tools(tools)
                    .build();

                agent
                    .prompt(task)
                    .max_turns(SUB_AGENT_MAX_TURNS)
                    .await
                    .map_err(|e| SubAgentError(format!("Sub-agent execution failed: {}", e)))?
            }
        };

        tracing::info!(
            "Sub-agent '{}' completed (response length: {} chars)",
            task_name,
            result.len()
        );

        Ok(result)
    }
}

impl Tool for DelegateTaskTool {
    const NAME: &'static str = "delegate_task";
    type Error = SubAgentError;
    type Args = DelegateTaskArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "delegate_task".to_string(),
            description: "Delegate a complex task to a temporary sub-agent. The sub-agent \
                works independently with its own context window and has access to all \
                tools (filesystem, planning, dynamic tools, self-programming, canvas, \
                memory). Use this for tasks that require many tool calls or would clutter \
                the main conversation.\n\n\
                You only need to provide a focused system prompt describing the sub-agent's \
                role. The tool documentation is automatically appended.\n\n\
                Example system prompts:\n\
                - \"You are a Next.js code-writing specialist. Analyze the existing project structure, \
                  understand current components and patterns, and implement features following \
                  the established conventions. Always test changes and ensure type safety.\"\n\
                - \"You are a research specialist. Investigate the topic thoroughly, organize \
                  findings in workspace files, and provide a structured summary.\"\n\
                - \"You are a knowledge organizer. Read workspace files, extract key information, \
                  and store important facts in long-term memory using add_memory.\""
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "task_name": {
                        "type": "string",
                        "description": "A short name for tracking (e.g. 'refactor-code-project', 'organize-notes')"
                    },
                    "system_prompt": {
                        "type": "string",
                        "description": "System prompt defining the sub-agent's role and approach. Tool docs are appended automatically."
                    },
                    "task": {
                        "type": "string",
                        "description": "The specific task for the sub-agent to accomplish"
                    }
                },
                "required": ["task_name", "system_prompt", "task"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        tracing::info!("Delegating task '{}' to sub-agent", args.task_name);

        let result = self
            .run_sub_agent(&args.system_prompt, &args.task, &args.task_name)
            .await?;

        Ok(format!(
            "[Sub-agent '{}' completed]\n\n{}",
            args.task_name, result
        ))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_delegate_task_tool_name() {
        assert_eq!(DelegateTaskTool::NAME, "delegate_task");
    }

    #[tokio::test]
    async fn test_delegate_task_definition() {
        let tool = DelegateTaskTool {
            client: None,
            model: String::new(),
            instance_id: String::new(),
            registry: None,
            db: None,
            programs_root: None,
            long_term_memory: None,
            app_handle: None,
        };

        let def = Tool::definition(&tool, "test".to_string()).await;
        assert_eq!(def.name, "delegate_task");
        assert!(def.description.contains("sub-agent"));
        assert!(def.description.contains("code-writing specialist"));
        assert!(def.description.contains("research specialist"));
        assert!(def.description.contains("knowledge organizer"));
    }

    #[tokio::test]
    async fn test_delegate_task_no_client() {
        let tool = DelegateTaskTool {
            client: None,
            model: String::new(),
            instance_id: String::new(),
            registry: None,
            db: None,
            programs_root: None,
            long_term_memory: None,
            app_handle: None,
        };

        let result = Tool::call(
            &tool,
            DelegateTaskArgs {
                task_name: "test-task".to_string(),
                system_prompt: "You are a test agent.".to_string(),
                task: "Do something.".to_string(),
            },
        )
        .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not initialized"));
    }

    #[test]
    fn test_base_tools_prompt_contains_all_sections() {
        let prompt = base_tools_prompt();
        // Filesystem
        assert!(prompt.contains("### Filesystem"));
        assert!(prompt.contains("ls"));
        assert!(prompt.contains("read_file"));
        assert!(prompt.contains("write_file"));
        assert!(prompt.contains("edit_file"));
        assert!(prompt.contains("grep"));
        // Planning
        assert!(prompt.contains("### Planning"));
        assert!(prompt.contains("read_todos"));
        assert!(prompt.contains("write_todos"));
        // Dynamic tools
        assert!(prompt.contains("execute_dynamic_tool"));
        // Self-programming
        assert!(prompt.contains("### Self-Programming"));
        assert!(prompt.contains("create_tool"));
        assert!(prompt.contains("read_tool"));
        assert!(prompt.contains("update_tool"));
        // Memory
        assert!(prompt.contains("### Long-Term Memory"));
        assert!(prompt.contains("search_memory"));
        assert!(prompt.contains("add_memory"));
        assert!(prompt.contains("delete_memory"));
        // Rhai
        assert!(prompt.contains("### Rhai Language Reference"));
        assert!(prompt.contains("http_get"));
        assert!(prompt.contains("json_parse"));
        // Canvas
        assert!(prompt.contains("## Canvas Programs"));
        assert!(prompt.contains("create_program"));
        assert!(prompt.contains("Bridge API"));
        assert!(prompt.contains("window.ownai"));
    }

    #[test]
    fn test_build_sub_agent_prompt_combines_custom_and_tools() {
        let prompt = DelegateTaskTool::build_sub_agent_prompt("You are a code-writing specialist.");
        // Custom prompt at the beginning
        assert!(prompt.starts_with("You are a code-writing specialist."));
        // Tool docs appended
        assert!(prompt.contains("## Available Tools"));
        assert!(prompt.contains("### Filesystem"));
    }
}
