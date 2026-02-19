pub mod protocol;
pub mod storage;
pub mod tools;

use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};

/// Metadata for a Canvas program (HTML/CSS/JS app)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgramMetadata {
    pub id: String,
    pub instance_id: String,
    pub name: String,
    pub description: String,
    pub version: String,
    pub created_at: String,
    pub updated_at: String,
}

/// Resolves a user-provided relative file path within a program directory.
/// Prevents directory traversal attacks and ensures the path stays within the program root.
pub fn resolve_program_path(
    programs_root: &Path,
    program_name: &str,
    user_path: &str,
) -> Result<PathBuf, String> {
    // Validate program name (no path separators or traversal)
    if program_name.contains('/')
        || program_name.contains('\\')
        || program_name.contains("..")
        || program_name.is_empty()
    {
        return Err("Invalid program name".to_string());
    }

    let path = Path::new(user_path);

    if path.is_absolute() {
        return Err("Absolute paths are not allowed".to_string());
    }

    if path.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err("Parent directory traversal (..) is not allowed".to_string());
    }

    Ok(programs_root.join(program_name).join(path))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_program_path_normal() {
        let root = PathBuf::from("/programs");
        assert_eq!(
            resolve_program_path(&root, "chess", "index.html").unwrap(),
            PathBuf::from("/programs/chess/index.html")
        );
    }

    #[test]
    fn test_resolve_program_path_subdirectory() {
        let root = PathBuf::from("/programs");
        assert_eq!(
            resolve_program_path(&root, "chess", "js/app.js").unwrap(),
            PathBuf::from("/programs/chess/js/app.js")
        );
    }

    #[test]
    fn test_resolve_program_path_blocks_traversal() {
        let root = PathBuf::from("/programs");
        assert!(resolve_program_path(&root, "chess", "../etc/passwd").is_err());
    }

    #[test]
    fn test_resolve_program_path_blocks_absolute() {
        let root = PathBuf::from("/programs");
        assert!(resolve_program_path(&root, "chess", "/etc/passwd").is_err());
    }

    #[test]
    fn test_resolve_program_path_blocks_invalid_name() {
        let root = PathBuf::from("/programs");
        assert!(resolve_program_path(&root, "../evil", "index.html").is_err());
        assert!(resolve_program_path(&root, "foo/bar", "index.html").is_err());
        assert!(resolve_program_path(&root, "", "index.html").is_err());
    }

    #[test]
    fn test_resolve_program_path_current_dir() {
        let root = PathBuf::from("/programs");
        assert_eq!(
            resolve_program_path(&root, "chess", ".").unwrap(),
            PathBuf::from("/programs/chess/.")
        );
    }
}
