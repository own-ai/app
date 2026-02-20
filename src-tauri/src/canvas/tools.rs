//! Canvas program tools for the agent.
//!
//! Provides seven rig Tools that allow the agent to create and manage
//! HTML/CSS/JS programs (Canvas apps):
//! - `CreateProgramTool`: Create a new program with initial HTML
//! - `ListProgramsTool`: List all programs for the current instance
//! - `OpenProgramTool`: Open an existing program in the frontend
//! - `ProgramLsTool`: List files within a program directory
//! - `ProgramReadFileTool`: Read a file from a program
//! - `ProgramWriteFileTool`: Write/create a file in a program (emits update event)
//! - `ProgramEditFileTool`: Edit a file with search/replace (emits update event)

use rig::completion::ToolDefinition;
use rig::tool::Tool;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{Pool, Sqlite};
use std::path::PathBuf;
use tauri::{AppHandle, Emitter};
use tokio::fs;

use super::resolve_program_path;
use super::storage;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct CanvasToolError(String);

// ---------------------------------------------------------------------------
// CreateProgramTool
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct CreateProgramArgs {
    name: String,
    description: String,
    html_content: String,
}

/// Agent tool to create a new Canvas program with an initial index.html.
#[derive(Clone, Serialize, Deserialize)]
pub struct CreateProgramTool {
    #[serde(skip)]
    db: Option<Pool<Sqlite>>,
    #[serde(skip)]
    instance_id: Option<String>,
    #[serde(skip)]
    programs_root: Option<PathBuf>,
    #[serde(skip)]
    #[allow(dead_code)]
    app_handle: Option<AppHandle>,
}

impl CreateProgramTool {
    pub fn new(
        db: Pool<Sqlite>,
        instance_id: String,
        programs_root: PathBuf,
        app_handle: Option<AppHandle>,
    ) -> Self {
        Self {
            db: Some(db),
            instance_id: Some(instance_id),
            programs_root: Some(programs_root),
            app_handle,
        }
    }
}

impl Tool for CreateProgramTool {
    const NAME: &'static str = "create_program";
    type Error = CanvasToolError;
    type Args = CreateProgramArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "create_program".to_string(),
            description: "Create a new Canvas program (HTML/CSS/JS app). \
                This creates the program directory, writes the initial index.html, \
                and registers the program. Use a unique, descriptive name. \
                After creation, use program_write_file to add CSS/JS files \
                and program_edit_file for targeted modifications."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Unique program name (lowercase, hyphens allowed, e.g. 'chess-board', 'expense-tracker')"
                    },
                    "description": {
                        "type": "string",
                        "description": "Brief description of what the program does"
                    },
                    "html_content": {
                        "type": "string",
                        "description": "Initial HTML content for index.html"
                    }
                },
                "required": ["name", "description", "html_content"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let db = self
            .db
            .as_ref()
            .ok_or_else(|| CanvasToolError("Database not initialized".to_string()))?;
        let instance_id = self
            .instance_id
            .as_ref()
            .ok_or_else(|| CanvasToolError("Instance ID not set".to_string()))?;
        let programs_root = self
            .programs_root
            .as_ref()
            .ok_or_else(|| CanvasToolError("Programs root not set".to_string()))?;

        // Validate program name
        if args.name.is_empty()
            || args.name.contains('/')
            || args.name.contains('\\')
            || args.name.contains("..")
        {
            return Err(CanvasToolError(
                "Invalid program name. Use lowercase letters, numbers, and hyphens.".to_string(),
            ));
        }

        // Create program in DB + directory
        let metadata = storage::create_program_in_db(
            db,
            instance_id,
            &args.name,
            &args.description,
            programs_root,
        )
        .await
        .map_err(|e| CanvasToolError(format!("Failed to create program: {}", e)))?;

        // Write initial index.html
        let index_path = programs_root.join(&args.name).join("index.html");
        fs::write(&index_path, &args.html_content)
            .await
            .map_err(|e| CanvasToolError(format!("Failed to write index.html: {}", e)))?;

        tracing::info!("Agent created program '{}' ({})", args.name, metadata.id);

        Ok(format!(
            "Program '{}' created successfully (version {}).\n\
             Files:\n  index.html ({} bytes)\n\n\
             Use program_write_file to add more files (CSS, JS, etc.).\n\
             Use program_edit_file to make targeted changes.",
            args.name,
            metadata.version,
            args.html_content.len()
        ))
    }
}

// ---------------------------------------------------------------------------
// ListProgramsTool
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ListProgramsArgs {}

/// Agent tool to list all Canvas programs for the current instance.
#[derive(Clone, Serialize, Deserialize)]
pub struct ListProgramsTool {
    #[serde(skip)]
    db: Option<Pool<Sqlite>>,
    #[serde(skip)]
    instance_id: Option<String>,
}

impl ListProgramsTool {
    pub fn new(db: Pool<Sqlite>, instance_id: String) -> Self {
        Self {
            db: Some(db),
            instance_id: Some(instance_id),
        }
    }
}

impl Tool for ListProgramsTool {
    const NAME: &'static str = "list_programs";
    type Error = CanvasToolError;
    type Args = ListProgramsArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "list_programs".to_string(),
            description: "List all Canvas programs you have created. \
                Shows name, description, version, and last update time."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {}
            }),
        }
    }

    async fn call(&self, _args: Self::Args) -> Result<Self::Output, Self::Error> {
        let db = self
            .db
            .as_ref()
            .ok_or_else(|| CanvasToolError("Database not initialized".to_string()))?;
        let instance_id = self
            .instance_id
            .as_ref()
            .ok_or_else(|| CanvasToolError("Instance ID not set".to_string()))?;

        let programs = storage::list_programs_from_db(db, instance_id)
            .await
            .map_err(|e| CanvasToolError(format!("Failed to list programs: {}", e)))?;

        if programs.is_empty() {
            return Ok("No programs created yet. Use create_program to create one.".to_string());
        }

        let list: Vec<String> = programs
            .iter()
            .map(|p| {
                format!(
                    "- {} (v{}): {} [updated: {}]",
                    p.name, p.version, p.description, p.updated_at
                )
            })
            .collect();

        Ok(format!(
            "Programs ({}):\n{}",
            programs.len(),
            list.join("\n")
        ))
    }
}

// ---------------------------------------------------------------------------
// ProgramLsTool
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ProgramLsArgs {
    program_name: String,
    #[serde(default = "default_current_dir")]
    path: String,
}

fn default_current_dir() -> String {
    ".".to_string()
}

/// Agent tool to list files within a program directory.
#[derive(Clone, Serialize, Deserialize)]
pub struct ProgramLsTool {
    #[serde(skip)]
    programs_root: Option<PathBuf>,
}

impl ProgramLsTool {
    pub fn new(programs_root: PathBuf) -> Self {
        Self {
            programs_root: Some(programs_root),
        }
    }
}

impl Tool for ProgramLsTool {
    const NAME: &'static str = "program_ls";
    type Error = CanvasToolError;
    type Args = ProgramLsArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "program_ls".to_string(),
            description: "List files and directories within a Canvas program. \
                Shows file names with type indicators (DIR/FILE) and sizes."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "program_name": {
                        "type": "string",
                        "description": "Name of the program to list files for"
                    },
                    "path": {
                        "type": "string",
                        "description": "Relative directory path within the program (default: root)"
                    }
                },
                "required": ["program_name"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let programs_root = self
            .programs_root
            .as_ref()
            .ok_or_else(|| CanvasToolError("Programs root not set".to_string()))?;

        let path = resolve_program_path(programs_root, &args.program_name, &args.path)
            .map_err(CanvasToolError)?;

        if !path.exists() {
            return Err(CanvasToolError(format!(
                "Directory not found: {} in program '{}'",
                args.path, args.program_name
            )));
        }

        let mut entries = fs::read_dir(&path)
            .await
            .map_err(|e| CanvasToolError(format!("Failed to read directory: {}", e)))?;

        let mut output = Vec::new();
        while let Ok(Some(entry)) = entries.next_entry().await {
            let metadata = entry
                .metadata()
                .await
                .map_err(|e| CanvasToolError(format!("Failed to read metadata: {}", e)))?;
            let name = entry.file_name().to_string_lossy().to_string();
            let kind = if metadata.is_dir() { "DIR " } else { "FILE" };
            let size = if metadata.is_file() {
                format!("  ({} bytes)", metadata.len())
            } else {
                String::new()
            };
            output.push(format!("{} {}{}", kind, name, size));
        }

        if output.is_empty() {
            Ok(format!(
                "Program '{}': (empty directory)",
                args.program_name
            ))
        } else {
            Ok(format!(
                "Program '{}':\n{}",
                args.program_name,
                output.join("\n")
            ))
        }
    }
}

// ---------------------------------------------------------------------------
// ProgramReadFileTool
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ProgramReadFileArgs {
    program_name: String,
    path: String,
    start_line: Option<usize>,
    end_line: Option<usize>,
}

/// Agent tool to read a file from a Canvas program directory.
#[derive(Clone, Serialize, Deserialize)]
pub struct ProgramReadFileTool {
    #[serde(skip)]
    programs_root: Option<PathBuf>,
}

impl ProgramReadFileTool {
    pub fn new(programs_root: PathBuf) -> Self {
        Self {
            programs_root: Some(programs_root),
        }
    }
}

impl Tool for ProgramReadFileTool {
    const NAME: &'static str = "program_read_file";
    type Error = CanvasToolError;
    type Args = ProgramReadFileArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "program_read_file".to_string(),
            description: "Read the contents of a file within a Canvas program. \
                Optionally specify start_line and end_line to read only a portion."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "program_name": {
                        "type": "string",
                        "description": "Name of the program"
                    },
                    "path": {
                        "type": "string",
                        "description": "Relative file path within the program (e.g. 'index.html', 'js/app.js')"
                    },
                    "start_line": {
                        "type": "number",
                        "description": "First line to read (1-indexed, optional)"
                    },
                    "end_line": {
                        "type": "number",
                        "description": "Last line to read (inclusive, optional)"
                    }
                },
                "required": ["program_name", "path"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let programs_root = self
            .programs_root
            .as_ref()
            .ok_or_else(|| CanvasToolError("Programs root not set".to_string()))?;

        let path = resolve_program_path(programs_root, &args.program_name, &args.path)
            .map_err(CanvasToolError)?;

        let content = fs::read_to_string(&path).await.map_err(|e| {
            CanvasToolError(format!(
                "Failed to read '{}' in program '{}': {}",
                args.path, args.program_name, e
            ))
        })?;

        if args.start_line.is_none() && args.end_line.is_none() {
            return Ok(content);
        }

        let lines: Vec<&str> = content.lines().collect();
        let start = args.start_line.unwrap_or(1).saturating_sub(1);
        let end = args.end_line.unwrap_or(lines.len()).min(lines.len());

        if start >= lines.len() {
            return Ok("(no lines in range)".to_string());
        }

        Ok(lines[start..end].join("\n"))
    }
}

// ---------------------------------------------------------------------------
// ProgramWriteFileTool
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ProgramWriteFileArgs {
    program_name: String,
    path: String,
    content: String,
}

/// Agent tool to write/create a file in a Canvas program directory.
#[derive(Clone, Serialize, Deserialize)]
pub struct ProgramWriteFileTool {
    #[serde(skip)]
    db: Option<Pool<Sqlite>>,
    #[serde(skip)]
    instance_id: Option<String>,
    #[serde(skip)]
    programs_root: Option<PathBuf>,
    #[serde(skip)]
    app_handle: Option<AppHandle>,
}

impl ProgramWriteFileTool {
    pub fn new(
        db: Pool<Sqlite>,
        instance_id: String,
        programs_root: PathBuf,
        app_handle: Option<AppHandle>,
    ) -> Self {
        Self {
            db: Some(db),
            instance_id: Some(instance_id),
            programs_root: Some(programs_root),
            app_handle,
        }
    }
}

impl Tool for ProgramWriteFileTool {
    const NAME: &'static str = "program_write_file";
    type Error = CanvasToolError;
    type Args = ProgramWriteFileArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "program_write_file".to_string(),
            description: "Write content to a file in a Canvas program. \
                Creates the file and parent directories if they don't exist, \
                overwrites if the file exists. Increments the program version."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "program_name": {
                        "type": "string",
                        "description": "Name of the program"
                    },
                    "path": {
                        "type": "string",
                        "description": "Relative file path (e.g. 'style.css', 'js/app.js')"
                    },
                    "content": {
                        "type": "string",
                        "description": "Content to write to the file"
                    }
                },
                "required": ["program_name", "path", "content"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let db = self
            .db
            .as_ref()
            .ok_or_else(|| CanvasToolError("Database not initialized".to_string()))?;
        let instance_id = self
            .instance_id
            .as_ref()
            .ok_or_else(|| CanvasToolError("Instance ID not set".to_string()))?;
        let programs_root = self
            .programs_root
            .as_ref()
            .ok_or_else(|| CanvasToolError("Programs root not set".to_string()))?;

        let path = resolve_program_path(programs_root, &args.program_name, &args.path)
            .map_err(CanvasToolError)?;

        // Verify program exists in DB
        storage::get_program_by_name(db, instance_id, &args.program_name)
            .await
            .map_err(|e| CanvasToolError(format!("Database error: {}", e)))?
            .ok_or_else(|| {
                CanvasToolError(format!(
                    "Program '{}' not found. Create it first with create_program.",
                    args.program_name
                ))
            })?;

        // Create parent directories
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|e| CanvasToolError(format!("Failed to create directories: {}", e)))?;
        }

        fs::write(&path, &args.content)
            .await
            .map_err(|e| CanvasToolError(format!("Failed to write file: {}", e)))?;

        // Increment program version
        let new_version = storage::update_program_version(db, instance_id, &args.program_name)
            .await
            .map_err(|e| CanvasToolError(format!("Failed to update version: {}", e)))?;

        // Notify frontend that the program was updated
        if let Some(handle) = &self.app_handle {
            let _ = handle.emit(
                "canvas:program_updated",
                json!({ "program_name": args.program_name, "version": new_version }),
            );
        }

        Ok(format!(
            "File written: {} ({} bytes) in program '{}' (now v{})",
            args.path,
            args.content.len(),
            args.program_name,
            new_version
        ))
    }
}

// ---------------------------------------------------------------------------
// ProgramEditFileTool
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ProgramEditFileArgs {
    program_name: String,
    path: String,
    old_text: String,
    new_text: String,
}

/// Agent tool to edit a file in a Canvas program using search/replace.
#[derive(Clone, Serialize, Deserialize)]
pub struct ProgramEditFileTool {
    #[serde(skip)]
    db: Option<Pool<Sqlite>>,
    #[serde(skip)]
    instance_id: Option<String>,
    #[serde(skip)]
    programs_root: Option<PathBuf>,
    #[serde(skip)]
    app_handle: Option<AppHandle>,
}

impl ProgramEditFileTool {
    pub fn new(
        db: Pool<Sqlite>,
        instance_id: String,
        programs_root: PathBuf,
        app_handle: Option<AppHandle>,
    ) -> Self {
        Self {
            db: Some(db),
            instance_id: Some(instance_id),
            programs_root: Some(programs_root),
            app_handle,
        }
    }
}

impl Tool for ProgramEditFileTool {
    const NAME: &'static str = "program_edit_file";
    type Error = CanvasToolError;
    type Args = ProgramEditFileArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "program_edit_file".to_string(),
            description: "Edit a file in a Canvas program by replacing old_text with new_text. \
                The old_text must appear exactly once in the file. \
                Increments the program version."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "program_name": {
                        "type": "string",
                        "description": "Name of the program"
                    },
                    "path": {
                        "type": "string",
                        "description": "Relative file path to edit (e.g. 'index.html', 'style.css')"
                    },
                    "old_text": {
                        "type": "string",
                        "description": "Exact text to find and replace (must be unique in file)"
                    },
                    "new_text": {
                        "type": "string",
                        "description": "Text to replace it with"
                    }
                },
                "required": ["program_name", "path", "old_text", "new_text"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let db = self
            .db
            .as_ref()
            .ok_or_else(|| CanvasToolError("Database not initialized".to_string()))?;
        let instance_id = self
            .instance_id
            .as_ref()
            .ok_or_else(|| CanvasToolError("Instance ID not set".to_string()))?;
        let programs_root = self
            .programs_root
            .as_ref()
            .ok_or_else(|| CanvasToolError("Programs root not set".to_string()))?;

        let path = resolve_program_path(programs_root, &args.program_name, &args.path)
            .map_err(CanvasToolError)?;

        let content = fs::read_to_string(&path).await.map_err(|e| {
            CanvasToolError(format!(
                "Failed to read '{}' in program '{}': {}",
                args.path, args.program_name, e
            ))
        })?;

        let count = content.matches(&args.old_text).count();
        if count == 0 {
            return Err(CanvasToolError("old_text not found in file".to_string()));
        }
        if count > 1 {
            return Err(CanvasToolError(format!(
                "old_text appears {} times, must be unique",
                count
            )));
        }

        let new_content = content.replace(&args.old_text, &args.new_text);
        fs::write(&path, &new_content)
            .await
            .map_err(|e| CanvasToolError(format!("Failed to write file: {}", e)))?;

        // Increment program version
        let new_version = storage::update_program_version(db, instance_id, &args.program_name)
            .await
            .map_err(|e| CanvasToolError(format!("Failed to update version: {}", e)))?;

        // Notify frontend that the program was updated
        if let Some(handle) = &self.app_handle {
            let _ = handle.emit(
                "canvas:program_updated",
                json!({ "program_name": args.program_name, "version": new_version }),
            );
        }

        Ok(format!(
            "File edited: {} in program '{}' (now v{})",
            args.path, args.program_name, new_version
        ))
    }
}

// ---------------------------------------------------------------------------
// OpenProgramTool
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct OpenProgramArgs {
    program_name: String,
}

/// Agent tool to open/display an existing Canvas program in the frontend.
/// Emits a `canvas:open_program` event so the frontend shows the program.
#[derive(Clone, Serialize, Deserialize)]
pub struct OpenProgramTool {
    #[serde(skip)]
    db: Option<Pool<Sqlite>>,
    #[serde(skip)]
    instance_id: Option<String>,
    #[serde(skip)]
    app_handle: Option<AppHandle>,
}

impl OpenProgramTool {
    pub fn new(db: Pool<Sqlite>, instance_id: String, app_handle: Option<AppHandle>) -> Self {
        Self {
            db: Some(db),
            instance_id: Some(instance_id),
            app_handle,
        }
    }
}

impl Tool for OpenProgramTool {
    const NAME: &'static str = "open_program";
    type Error = CanvasToolError;
    type Args = OpenProgramArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "open_program".to_string(),
            description: "Open an existing Canvas program in the user's view. \
                Use this to display a program that was previously created, \
                instead of creating a new one. The program will appear in \
                the Canvas panel next to the chat."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "program_name": {
                        "type": "string",
                        "description": "Name of the program to open"
                    }
                },
                "required": ["program_name"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let db = self
            .db
            .as_ref()
            .ok_or_else(|| CanvasToolError("Database not initialized".to_string()))?;
        let instance_id = self
            .instance_id
            .as_ref()
            .ok_or_else(|| CanvasToolError("Instance ID not set".to_string()))?;

        // Verify program exists
        let program = storage::get_program_by_name(db, instance_id, &args.program_name)
            .await
            .map_err(|e| CanvasToolError(format!("Database error: {}", e)))?
            .ok_or_else(|| {
                CanvasToolError(format!(
                    "Program '{}' not found. Use list_programs to see available programs, \
                     or create_program to create a new one.",
                    args.program_name
                ))
            })?;

        // Emit event to open the program in the frontend
        if let Some(handle) = &self.app_handle {
            let _ = handle.emit(
                "canvas:open_program",
                json!({ "program_name": args.program_name }),
            );
        }

        Ok(format!(
            "Program '{}' (v{}) is now displayed in the Canvas panel.",
            program.name, program.version
        ))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;
    use tempfile::TempDir;

    async fn setup() -> (Pool<Sqlite>, TempDir) {
        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();
        crate::database::schema::create_tables(&pool).await.unwrap();
        let temp_dir = TempDir::new().unwrap();
        (pool, temp_dir)
    }

    #[tokio::test]
    async fn test_create_program_tool() {
        let (db, temp_dir) = setup().await;
        let programs_root = temp_dir.path().to_path_buf();

        let tool = CreateProgramTool::new(db, "inst-1".to_string(), programs_root.clone(), None);

        let result = tool
            .call(CreateProgramArgs {
                name: "chess".to_string(),
                description: "A chess game".to_string(),
                html_content: "<html><body>Chess</body></html>".to_string(),
            })
            .await
            .unwrap();

        assert!(result.contains("created successfully"));
        assert!(programs_root.join("chess").join("index.html").exists());

        let content =
            std::fs::read_to_string(programs_root.join("chess").join("index.html")).unwrap();
        assert_eq!(content, "<html><body>Chess</body></html>");
    }

    #[tokio::test]
    async fn test_create_program_invalid_name() {
        let (db, temp_dir) = setup().await;
        let tool = CreateProgramTool::new(
            db,
            "inst-1".to_string(),
            temp_dir.path().to_path_buf(),
            None,
        );

        let result = tool
            .call(CreateProgramArgs {
                name: "../evil".to_string(),
                description: "Bad".to_string(),
                html_content: "x".to_string(),
            })
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_list_programs_tool_empty() {
        let (db, _temp_dir) = setup().await;
        let tool = ListProgramsTool::new(db, "inst-1".to_string());

        let result = tool.call(ListProgramsArgs {}).await.unwrap();
        assert!(result.contains("No programs created yet"));
    }

    #[tokio::test]
    async fn test_list_programs_tool_with_programs() {
        let (db, temp_dir) = setup().await;
        let programs_root = temp_dir.path();

        storage::create_program_in_db(&db, "inst-1", "chess", "Chess game", programs_root)
            .await
            .unwrap();

        let tool = ListProgramsTool::new(db, "inst-1".to_string());
        let result = tool.call(ListProgramsArgs {}).await.unwrap();

        assert!(result.contains("chess"));
        assert!(result.contains("Chess game"));
        assert!(result.contains("Programs (1)"));
    }

    #[tokio::test]
    async fn test_program_ls_tool() {
        let (db, temp_dir) = setup().await;
        let programs_root = temp_dir.path();

        storage::create_program_in_db(&db, "inst-1", "chess", "Chess", programs_root)
            .await
            .unwrap();
        std::fs::write(programs_root.join("chess").join("index.html"), "<html>").unwrap();
        std::fs::write(programs_root.join("chess").join("style.css"), "body{}").unwrap();

        let tool = ProgramLsTool::new(programs_root.to_path_buf());
        let result = tool
            .call(ProgramLsArgs {
                program_name: "chess".to_string(),
                path: ".".to_string(),
            })
            .await
            .unwrap();

        assert!(result.contains("index.html"));
        assert!(result.contains("style.css"));
    }

    #[tokio::test]
    async fn test_program_read_file_tool() {
        let (db, temp_dir) = setup().await;
        let programs_root = temp_dir.path();

        storage::create_program_in_db(&db, "inst-1", "chess", "Chess", programs_root)
            .await
            .unwrap();
        std::fs::write(
            programs_root.join("chess").join("index.html"),
            "line1\nline2\nline3",
        )
        .unwrap();

        let tool = ProgramReadFileTool::new(programs_root.to_path_buf());

        // Read full file
        let result = tool
            .call(ProgramReadFileArgs {
                program_name: "chess".to_string(),
                path: "index.html".to_string(),
                start_line: None,
                end_line: None,
            })
            .await
            .unwrap();
        assert_eq!(result, "line1\nline2\nline3");

        // Read range
        let result = tool
            .call(ProgramReadFileArgs {
                program_name: "chess".to_string(),
                path: "index.html".to_string(),
                start_line: Some(2),
                end_line: Some(2),
            })
            .await
            .unwrap();
        assert_eq!(result, "line2");
    }

    #[tokio::test]
    async fn test_program_write_file_tool() {
        let (db, temp_dir) = setup().await;
        let programs_root = temp_dir.path();

        storage::create_program_in_db(&db, "inst-1", "chess", "Chess", programs_root)
            .await
            .unwrap();

        let tool = ProgramWriteFileTool::new(
            db.clone(),
            "inst-1".to_string(),
            programs_root.to_path_buf(),
            None,
        );

        let result = tool
            .call(ProgramWriteFileArgs {
                program_name: "chess".to_string(),
                path: "style.css".to_string(),
                content: "body { margin: 0; }".to_string(),
            })
            .await
            .unwrap();

        assert!(result.contains("File written: style.css"));
        assert!(result.contains("v1.0.1"));

        let content =
            std::fs::read_to_string(programs_root.join("chess").join("style.css")).unwrap();
        assert_eq!(content, "body { margin: 0; }");
    }

    #[tokio::test]
    async fn test_program_write_file_nonexistent_program() {
        let (db, temp_dir) = setup().await;
        let tool = ProgramWriteFileTool::new(
            db,
            "inst-1".to_string(),
            temp_dir.path().to_path_buf(),
            None,
        );

        let result = tool
            .call(ProgramWriteFileArgs {
                program_name: "nope".to_string(),
                path: "file.txt".to_string(),
                content: "x".to_string(),
            })
            .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[tokio::test]
    async fn test_program_edit_file_tool() {
        let (db, temp_dir) = setup().await;
        let programs_root = temp_dir.path();

        storage::create_program_in_db(&db, "inst-1", "chess", "Chess", programs_root)
            .await
            .unwrap();
        std::fs::write(
            programs_root.join("chess").join("index.html"),
            "<html><body>Hello</body></html>",
        )
        .unwrap();

        let tool = ProgramEditFileTool::new(
            db.clone(),
            "inst-1".to_string(),
            programs_root.to_path_buf(),
            None,
        );

        let result = tool
            .call(ProgramEditFileArgs {
                program_name: "chess".to_string(),
                path: "index.html".to_string(),
                old_text: "Hello".to_string(),
                new_text: "Chess Board".to_string(),
            })
            .await
            .unwrap();

        assert!(result.contains("File edited"));
        assert!(result.contains("v1.0.1"));

        let content =
            std::fs::read_to_string(programs_root.join("chess").join("index.html")).unwrap();
        assert_eq!(content, "<html><body>Chess Board</body></html>");
    }

    #[tokio::test]
    async fn test_program_edit_file_old_text_not_found() {
        let (db, temp_dir) = setup().await;
        let programs_root = temp_dir.path();

        storage::create_program_in_db(&db, "inst-1", "chess", "Chess", programs_root)
            .await
            .unwrap();
        std::fs::write(
            programs_root.join("chess").join("index.html"),
            "<html></html>",
        )
        .unwrap();

        let tool =
            ProgramEditFileTool::new(db, "inst-1".to_string(), programs_root.to_path_buf(), None);

        let result = tool
            .call(ProgramEditFileArgs {
                program_name: "chess".to_string(),
                path: "index.html".to_string(),
                old_text: "not found text".to_string(),
                new_text: "replacement".to_string(),
            })
            .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }
}
