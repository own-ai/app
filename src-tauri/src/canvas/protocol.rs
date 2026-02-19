use std::path::{Component, Path, PathBuf};

/// Load a file from a program directory.
/// Returns the file bytes and MIME type.
pub fn load_program_file(
    programs_root: &Path,
    program_name: &str,
    file_path: &str,
) -> Result<(Vec<u8>, String), String> {
    // Validate program name
    if program_name.contains('/')
        || program_name.contains('\\')
        || program_name.contains("..")
        || program_name.is_empty()
    {
        return Err("Invalid program name".to_string());
    }

    // Validate file path
    let rel_path = Path::new(file_path);
    if rel_path.is_absolute() {
        return Err("Absolute paths are not allowed".to_string());
    }
    if rel_path
        .components()
        .any(|c| matches!(c, Component::ParentDir))
    {
        return Err("Parent directory traversal (..) is not allowed".to_string());
    }

    let full_path = programs_root.join(program_name).join(rel_path);

    if !full_path.exists() || !full_path.is_file() {
        return Err(format!("File not found: {}", file_path));
    }

    let bytes = std::fs::read(&full_path).map_err(|e| format!("Failed to read file: {}", e))?;

    let mime = guess_mime_type(file_path);

    Ok((bytes, mime))
}

/// Parse a protocol URL into (instance_id, program_name, file_path).
///
/// Expected URL format:
///   `ownai-program://localhost/{instance_id}/{program_name}/{file_path}`
///
/// The `localhost` authority is required by Tauri's custom protocol handling.
pub fn parse_protocol_url(url: &str) -> Result<(String, String, String), String> {
    // Strip the scheme
    let rest = url
        .strip_prefix("ownai-program://localhost/")
        .or_else(|| url.strip_prefix("ownai-program://"))
        .ok_or_else(|| format!("Invalid protocol URL: {}", url))?;

    let parts: Vec<&str> = rest.splitn(3, '/').collect();
    if parts.len() < 2 {
        return Err(format!(
            "URL must contain at least instance_id and program_name: {}",
            url
        ));
    }

    let instance_id = parts[0].to_string();
    let program_name = parts[1].to_string();
    let file_path = if parts.len() > 2 && !parts[2].is_empty() {
        parts[2].to_string()
    } else {
        "index.html".to_string()
    };

    if instance_id.is_empty() || program_name.is_empty() {
        return Err("instance_id and program_name must not be empty".to_string());
    }

    Ok((instance_id, program_name, file_path))
}

/// Guess MIME type from file extension.
pub fn guess_mime_type(file_path: &str) -> String {
    let path = PathBuf::from(file_path);
    match path.extension().and_then(|e| e.to_str()) {
        Some("html") | Some("htm") => "text/html".to_string(),
        Some("css") => "text/css".to_string(),
        Some("js") | Some("mjs") => "text/javascript".to_string(),
        Some("json") => "application/json".to_string(),
        Some("svg") => "image/svg+xml".to_string(),
        Some("png") => "image/png".to_string(),
        Some("jpg") | Some("jpeg") => "image/jpeg".to_string(),
        Some("gif") => "image/gif".to_string(),
        Some("webp") => "image/webp".to_string(),
        Some("ico") => "image/x-icon".to_string(),
        Some("woff") => "font/woff".to_string(),
        Some("woff2") => "font/woff2".to_string(),
        Some("ttf") => "font/ttf".to_string(),
        Some("otf") => "font/otf".to_string(),
        Some("xml") => "application/xml".to_string(),
        Some("txt") => "text/plain".to_string(),
        Some("md") => "text/markdown".to_string(),
        Some("wasm") => "application/wasm".to_string(),
        _ => "application/octet-stream".to_string(),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_guess_mime_type() {
        assert_eq!(guess_mime_type("index.html"), "text/html");
        assert_eq!(guess_mime_type("style.css"), "text/css");
        assert_eq!(guess_mime_type("app.js"), "text/javascript");
        assert_eq!(guess_mime_type("data.json"), "application/json");
        assert_eq!(guess_mime_type("icon.svg"), "image/svg+xml");
        assert_eq!(guess_mime_type("photo.png"), "image/png");
        assert_eq!(guess_mime_type("font.woff2"), "font/woff2");
        assert_eq!(guess_mime_type("unknown.xyz"), "application/octet-stream");
        assert_eq!(guess_mime_type("noext"), "application/octet-stream");
    }

    #[test]
    fn test_parse_protocol_url_full() {
        let (inst, prog, file) =
            parse_protocol_url("ownai-program://localhost/inst-123/chess/index.html").unwrap();
        assert_eq!(inst, "inst-123");
        assert_eq!(prog, "chess");
        assert_eq!(file, "index.html");
    }

    #[test]
    fn test_parse_protocol_url_nested_path() {
        let (inst, prog, file) =
            parse_protocol_url("ownai-program://localhost/inst-1/myapp/js/app.js").unwrap();
        assert_eq!(inst, "inst-1");
        assert_eq!(prog, "myapp");
        assert_eq!(file, "js/app.js");
    }

    #[test]
    fn test_parse_protocol_url_default_index() {
        let (_, _, file) = parse_protocol_url("ownai-program://localhost/inst-1/chess/").unwrap();
        assert_eq!(file, "index.html");

        let (_, _, file2) = parse_protocol_url("ownai-program://localhost/inst-1/chess").unwrap();
        assert_eq!(file2, "index.html");
    }

    #[test]
    fn test_parse_protocol_url_invalid() {
        assert!(parse_protocol_url("https://example.com").is_err());
        assert!(parse_protocol_url("ownai-program://localhost/").is_err());
    }

    #[test]
    fn test_load_program_file_success() {
        let temp_dir = TempDir::new().unwrap();
        let programs_root = temp_dir.path();

        // Create program directory with a file
        let program_dir = programs_root.join("chess");
        fs::create_dir_all(&program_dir).unwrap();
        fs::write(program_dir.join("index.html"), "<html>Chess</html>").unwrap();

        let (bytes, mime) = load_program_file(programs_root, "chess", "index.html").unwrap();
        assert_eq!(String::from_utf8(bytes).unwrap(), "<html>Chess</html>");
        assert_eq!(mime, "text/html");
    }

    #[test]
    fn test_load_program_file_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let result = load_program_file(temp_dir.path(), "chess", "index.html");
        assert!(result.is_err());
    }

    #[test]
    fn test_load_program_file_blocks_traversal() {
        let temp_dir = TempDir::new().unwrap();
        let result = load_program_file(temp_dir.path(), "chess", "../secret.txt");
        assert!(result.is_err());
    }

    #[test]
    fn test_load_program_file_blocks_invalid_name() {
        let temp_dir = TempDir::new().unwrap();
        assert!(load_program_file(temp_dir.path(), "../evil", "index.html").is_err());
        assert!(load_program_file(temp_dir.path(), "", "index.html").is_err());
    }
}
