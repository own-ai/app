use rig::tool::ToolDyn;
use sqlx::{Pool, Sqlite};
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

use crate::canvas::tools::{
    CreateProgramTool, ListProgramsTool, OpenProgramTool, ProgramEditFileTool, ProgramLsTool,
    ProgramReadFileTool, ProgramWriteFileTool,
};
use crate::memory::SharedLongTermMemory;
use crate::scheduler::{
    CreateScheduledTaskTool, DeleteScheduledTaskTool, ListScheduledTasksTool, SharedScheduler,
};
use crate::tools::code_generation::{CreateToolTool, ReadToolTool, UpdateToolTool};
use crate::tools::collection_tools::{
    CreateKnowledgeCollectionTool, DeleteKnowledgeCollectionTool, IngestDocumentTool,
    ListKnowledgeCollectionsTool,
};
use crate::tools::filesystem::{EditFileTool, GrepTool, LsTool, ReadFileTool, WriteFileTool};
use crate::tools::memory_tools::{AddMemoryTool, DeleteMemoryTool, SearchMemoryTool};
use crate::tools::planning::{ReadTodosTool, SharedTodoList, WriteTodosTool};
use crate::tools::rhai_bridge_tool::{RhaiExecuteTool, SharedRegistry};
use crate::tools::subagents::{ClientProvider, DelegateTaskTool};
use crate::utils::paths;

/// Helper: Create the set of tools for an instance.
/// Includes all tools: filesystem, planning, dynamic tools, self-programming,
/// canvas, memory, and task delegation (sub-agents).
#[allow(clippy::too_many_arguments)]
pub(super) fn create_tools(
    instance_id: &str,
    instance_name: &str,
    todo_list: SharedTodoList,
    registry: SharedRegistry,
    available_dynamic_tools: Vec<(String, String)>,
    db: Pool<Sqlite>,
    programs_root: PathBuf,
    long_term_memory: SharedLongTermMemory,
    client_provider: ClientProvider,
    model: String,
    app_handle: Option<AppHandle>,
) -> Vec<Box<dyn ToolDyn>> {
    let workspace =
        paths::get_instance_workspace_path(instance_id).unwrap_or_else(|_| PathBuf::from("."));

    let mut tools: Vec<Box<dyn ToolDyn>> = vec![
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
        Box::new(UpdateToolTool::new(registry.clone(), workspace.clone())),
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
            db.clone(),
            instance_id.to_string(),
            programs_root.clone(),
            app_handle.clone(),
        )),
        // Memory tools (long-term vector store)
        Box::new(SearchMemoryTool::new(long_term_memory.clone(), db.clone())),
        Box::new(AddMemoryTool::new(long_term_memory.clone(), db.clone())),
        Box::new(DeleteMemoryTool::new(long_term_memory.clone())),
        // Task delegation (sub-agents)
        Box::new(DelegateTaskTool::new(
            client_provider,
            model,
            instance_id.to_string(),
            instance_name.to_string(),
            registry,
            db.clone(),
            programs_root,
            long_term_memory.clone(),
            app_handle.clone(),
        )),
        // Knowledge collection tools (document ingestion & organization)
        Box::new(CreateKnowledgeCollectionTool::new(db.clone())),
        Box::new(ListKnowledgeCollectionsTool::new(db.clone())),
        Box::new(DeleteKnowledgeCollectionTool::new(db.clone())),
        Box::new(IngestDocumentTool::new(
            db.clone(),
            long_term_memory,
            workspace,
        )),
    ];

    // Scheduled task tools (only available when scheduler is initialized)
    if let Some(ref handle) = app_handle {
        if let Some(scheduler_state) = handle.try_state::<SharedScheduler>() {
            let scheduler = scheduler_state.inner().clone();
            if let Some(manager_state) =
                handle.try_state::<std::sync::Arc<tokio::sync::Mutex<crate::ai_instances::AIInstanceManager>>>()
            {
                let manager = manager_state.inner().clone();
                tools.push(Box::new(CreateScheduledTaskTool::new(
                    db.clone(),
                    instance_id.to_string(),
                    scheduler.clone(),
                    manager,
                    app_handle.clone(),
                )));
                tools.push(Box::new(ListScheduledTasksTool::new(
                    db.clone(),
                    instance_id.to_string(),
                )));
                tools.push(Box::new(DeleteScheduledTaskTool::new(db, scheduler)));
            }
        }
    }

    tools
}
