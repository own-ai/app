use rig::completion::ToolDefinition;
use rig::tool::Tool;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::{Component, Path, PathBuf};
use tokio::fs;

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// Resolves a user-provided relative path within the workspace root.
/// Prevents directory traversal attacks.
fn resolve_path(root: &Path, user_path: &str) -> Result<PathBuf, String> {
    let path = Path::new(user_path);

    if path.is_absolute() {
        return Err("Absolute paths are not allowed".to_string());
    }

    if path
        .components()
        .any(|c| matches!(c, Component::ParentDir))
    {
        return Err("Parent directory traversal (..) is not allowed".to_string());
    }

    Ok(root.join(path))
}

// ---------------------------------------------------------------------------
// ls
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct LsArgs {
    #[serde(default = "default_current_dir")]
    path: String,
}

fn default_current_dir() -> String {
    ".".to_string()
}

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct ToolError(String);

#[derive(Clone, Serialize, Deserialize)]
pub struct LsTool {
    root: PathBuf,
}

impl LsTool {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
}

impl Tool for LsTool {
    const NAME: &'static str = "ls";
    type Error = ToolError;
    type Args = LsArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "ls".to_string(),
            description: "List files and directories in the workspace. Returns names with type indicators (DIR/FILE) and file sizes.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Relative directory path to list (default: current directory)"
                    }
                }
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let path = resolve_path(&self.root, &args.path).map_err(ToolError)?;

        if !path.exists() {
            return Err(ToolError(format!("Directory not found: {}", args.path)));
        }

        let mut entries = fs::read_dir(&path)
            .await
            .map_err(|e| ToolError(format!("Failed to read directory: {}", e)))?;

        let mut output = Vec::new();
        while let Ok(Some(entry)) = entries.next_entry().await {
            let metadata = entry
                .metadata()
                .await
                .map_err(|e| ToolError(format!("Failed to read metadata: {}", e)))?;
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
            Ok("(empty directory)".to_string())
        } else {
            Ok(output.join("\n"))
        }
    }
}

// ---------------------------------------------------------------------------
// read_file
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ReadFileArgs {
    path: String,
    start_line: Option<usize>,
    end_line: Option<usize>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ReadFileTool {
    root: PathBuf,
}

impl ReadFileTool {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
}

impl Tool for ReadFileTool {
    const NAME: &'static str = "read_file";
    type Error = ToolError;
    type Args = ReadFileArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "read_file".to_string(),
            description: "Read the contents of a file in the workspace. Optionally specify start_line and end_line to read only a portion.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Relative path to the file to read"
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
                "required": ["path"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let path = resolve_path(&self.root, &args.path).map_err(ToolError)?;

        let content = fs::read_to_string(&path)
            .await
            .map_err(|e| ToolError(format!("Failed to read file '{}': {}", args.path, e)))?;

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
// write_file
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct WriteFileArgs {
    path: String,
    content: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct WriteFileTool {
    root: PathBuf,
}

impl WriteFileTool {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
}

impl Tool for WriteFileTool {
    const NAME: &'static str = "write_file";
    type Error = ToolError;
    type Args = WriteFileArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "write_file".to_string(),
            description: "Write content to a file in the workspace. Creates the file and parent directories if they don't exist, overwrites if the file exists.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Relative path where to write the file"
                    },
                    "content": {
                        "type": "string",
                        "description": "Content to write to the file"
                    }
                },
                "required": ["path", "content"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let path = resolve_path(&self.root, &args.path).map_err(ToolError)?;

        // Create parent directories
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|e| ToolError(format!("Failed to create directories: {}", e)))?;
        }

        fs::write(&path, &args.content)
            .await
            .map_err(|e| ToolError(format!("Failed to write file: {}", e)))?;

        Ok(format!(
            "File written: {} ({} bytes)",
            args.path,
            args.content.len()
        ))
    }
}

// ---------------------------------------------------------------------------
// edit_file
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct EditFileArgs {
    path: String,
    old_text: String,
    new_text: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct EditFileTool {
    root: PathBuf,
}

impl EditFileTool {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
}

impl Tool for EditFileTool {
    const NAME: &'static str = "edit_file";
    type Error = ToolError;
    type Args = EditFileArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "edit_file".to_string(),
            description: "Edit a file by replacing old_text with new_text. The old_text must appear exactly once in the file.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Relative path to the file to edit"
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
                "required": ["path", "old_text", "new_text"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let path = resolve_path(&self.root, &args.path).map_err(ToolError)?;

        let content = fs::read_to_string(&path)
            .await
            .map_err(|e| ToolError(format!("Failed to read file '{}': {}", args.path, e)))?;

        let count = content.matches(&args.old_text).count();
        if count == 0 {
            return Err(ToolError("old_text not found in file".to_string()));
        }
        if count > 1 {
            return Err(ToolError(format!(
                "old_text appears {} times, must be unique",
                count
            )));
        }

        let new_content = content.replace(&args.old_text, &args.new_text);
        fs::write(&path, &new_content)
            .await
            .map_err(|e| ToolError(format!("Failed to write file: {}", e)))?;

        Ok(format!("File edited: {}", args.path))
    }
}

// ---------------------------------------------------------------------------
// grep
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct GrepArgs {
    pattern: String,
    path: String,
    #[serde(default)]
    recursive: bool,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct GrepTool {
    root: PathBuf,
}

impl GrepTool {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
}

impl Tool for GrepTool {
    const NAME: &'static str = "grep";
    type Error = ToolError;
    type Args = GrepArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "grep".to_string(),
            description: "Search for a text pattern in files within the workspace. Returns matching lines with file paths and line numbers.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "pattern": {
                        "type": "string",
                        "description": "Text pattern to search for"
                    },
                    "path": {
                        "type": "string",
                        "description": "File or directory to search in"
                    },
                    "recursive": {
                        "type": "boolean",
                        "description": "Search recursively in directories (default: false)"
                    }
                },
                "required": ["pattern", "path"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let path = resolve_path(&self.root, &args.path).map_err(ToolError)?;
        let mut results = Vec::new();

        if path.is_file() {
            search_file(&path, &args.pattern, &mut results).await?;
        } else if path.is_dir() {
            search_directory(&path, &args.pattern, args.recursive, &mut results).await?;
        } else {
            return Err(ToolError(format!("Path not found: {}", args.path)));
        }

        if results.is_empty() {
            Ok("No matches found".to_string())
        } else {
            Ok(results.join("\n"))
        }
    }
}

async fn search_file(path: &Path, pattern: &str, results: &mut Vec<String>) -> Result<(), ToolError> {
    if let Ok(content) = fs::read_to_string(path).await {
        for (line_num, line) in content.lines().enumerate() {
            if line.contains(pattern) {
                results.push(format!(
                    "{}:{}: {}",
                    path.display(),
                    line_num + 1,
                    line.trim()
                ));
            }
        }
    }
    Ok(())
}

async fn search_directory(
    dir: &Path,
    pattern: &str,
    recursive: bool,
    results: &mut Vec<String>,
) -> Result<(), ToolError> {
    let mut entries = fs::read_dir(dir)
        .await
        .map_err(|e| ToolError(format!("Failed to read directory: {}", e)))?;

    while let Ok(Some(entry)) = entries.next_entry().await {
        let entry_path = entry.path();
        if entry_path.is_file() {
            search_file(&entry_path, pattern, results).await?;
        } else if entry_path.is_dir() && recursive {
            // Use Box::pin() for recursive async
            Box::pin(search_directory(&entry_path, pattern, recursive, results)).await?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_path_normal() {
        let root = PathBuf::from("/workspace");
        assert_eq!(
            resolve_path(&root, "test.txt").unwrap(),
            PathBuf::from("/workspace/test.txt")
        );
    }

    #[test]
    fn test_resolve_path_subdirectory() {
        let root = PathBuf::from("/workspace");
        assert_eq!(
            resolve_path(&root, "sub/test.txt").unwrap(),
            PathBuf::from("/workspace/sub/test.txt")
        );
    }

    #[test]
    fn test_resolve_path_blocks_traversal() {
        let root = PathBuf::from("/workspace");
        assert!(resolve_path(&root, "../etc/passwd").is_err());
    }

    #[test]
    fn test_resolve_path_blocks_absolute() {
        let root = PathBuf::from("/workspace");
        assert!(resolve_path(&root, "/etc/passwd").is_err());
    }
}
